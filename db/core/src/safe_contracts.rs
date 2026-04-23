use std::collections::{HashMap, HashSet};

use async_trait::async_trait;
use blokli_db_entity::{
    hopr_safe_contract, hopr_safe_contract_state,
    prelude::{HoprSafeContract, HoprSafeContractState},
};
use hopr_types::{crypto::types::Hash, primitive::prelude::Address};
use sea_orm::{ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter, QueryOrder, Set};
use sea_query::OnConflict;

use crate::{
    BlokliDb, BlokliDbGeneralModelOperations, DbSqlError, OptTx, Result, safe_history::BlokliDbSafeHistoryOperations,
};

/// Combined safe contract entry with identity and current state.
///
/// This struct provides a unified view of a safe contract, combining
/// the immutable identity (address) with the latest mutable state
/// (module_address, chain_key, published coordinates).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SafeContractEntry {
    /// Database ID of the safe contract identity
    pub id: i64,
    /// Safe contract address (immutable identity)
    pub address: Vec<u8>,
    /// Current module address (from latest state)
    pub module_address: Vec<u8>,
    /// Current chain key / owner (from latest state)
    pub chain_key: Vec<u8>,
    /// Block number where current state was published
    pub published_block: i64,
    /// Transaction index where current state was published
    pub published_tx_index: i64,
    /// Log index where current state was published
    pub published_log_index: i64,
}

/// Pre-seeded block number marker.
///
/// Safes with this block number were loaded from CSV files during migration,
/// not from real blockchain events. This allows identifying which safes
/// need their module addresses refreshed on startup.
pub const PRESEEDED_BLOCK: i64 = 30_000_000;

struct SafeCsvEntry {
    address: Address,
    module_address: Address,
    chain_key: Address,
    owners: Vec<Address>,
    threshold: Option<u64>,
    deployed_tx_index: u64,
    deployed_log_index: u64,
}

fn parse_optional_u64_field(value: &str, field_name: &str, line_number: usize) -> Result<Option<u64>> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }

    let parsed: u64 = trimmed
        .parse()
        .map_err(|e| DbSqlError::Construction(format!("invalid {field_name} on line {line_number}: {e}")))?;
    Ok(Some(parsed))
}

fn parse_owner_list(value: &str, line_number: usize) -> Result<Vec<Address>> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Ok(Vec::new());
    }

    trimmed
        .split(';')
        .map(str::trim)
        .filter(|owner| !owner.is_empty())
        .map(|owner| {
            owner
                .parse()
                .map_err(|e| DbSqlError::Construction(format!("invalid safe owner on line {line_number}: {e}")))
        })
        .collect()
}

fn parse_csv_entry(line: &str, line_number: usize) -> Result<Option<SafeCsvEntry>> {
    let fields: Vec<&str> = line.split(',').map(str::trim).collect();
    if fields.is_empty() || fields.iter().all(|field| field.is_empty()) {
        return Ok(None);
    }
    if fields.len() != 6 && fields.len() != 8 {
        return Err(DbSqlError::Construction(format!(
            "invalid number of safe CSV fields on line {line_number}: expected 6 or 8, got {}",
            fields.len()
        )));
    }

    let address: Address = fields[0]
        .parse()
        .map_err(|e| DbSqlError::Construction(format!("invalid safe address on line {line_number}: {e}")))?;
    let module_address: Address = fields[1]
        .parse()
        .map_err(|e| DbSqlError::Construction(format!("invalid module address on line {line_number}: {e}")))?;
    let chain_key: Address = fields[2]
        .parse()
        .map_err(|e| DbSqlError::Construction(format!("invalid chain key on line {line_number}: {e}")))?;
    let owners: Vec<Address> = if fields.len() == 8 {
        parse_owner_list(fields[4], line_number)?
    } else {
        Vec::new()
    };
    let threshold: Option<u64> = if fields.len() == 8 {
        parse_optional_u64_field(fields[5], "threshold", line_number)?
    } else {
        None
    };
    let deployed_tx_index: u64 = if fields.len() == 8 { fields[6] } else { fields[4] }
        .parse()
        .map_err(|e| DbSqlError::Construction(format!("invalid tx index on line {line_number}: {e}")))?;
    let deployed_log_index: u64 = if fields.len() == 8 { fields[7] } else { fields[5] }
        .parse()
        .map_err(|e| DbSqlError::Construction(format!("invalid log index on line {line_number}: {e}")))?;

    Ok(Some(SafeCsvEntry {
        address,
        module_address,
        chain_key,
        owners,
        threshold,
        deployed_tx_index,
        deployed_log_index,
    }))
}

async fn persist_preseeded_safe_history<'a, Db>(db: &'a Db, tx: OptTx<'a>, entry: &SafeCsvEntry) -> Result<()>
where
    Db: BlokliDbSafeHistoryOperations + Sync,
{
    if let Some(threshold) = entry.threshold {
        db.record_safe_setup(
            tx,
            entry.address,
            Hash::default(),
            entry.owners.clone(),
            threshold.to_string(),
            None,
            PRESEEDED_BLOCK as u64,
            entry.deployed_tx_index,
            entry.deployed_log_index,
        )
        .await?;
    }

    for owner in &entry.owners {
        db.upsert_safe_owner_state(
            tx,
            entry.address,
            *owner,
            true,
            PRESEEDED_BLOCK as u64,
            entry.deployed_tx_index,
            entry.deployed_log_index,
        )
        .await?;
    }

    Ok(())
}

fn preseeded_csv_for_network(network_name: &str) -> Option<&'static str> {
    if network_name.eq_ignore_ascii_case("rotsee") {
        Some(include_str!("../../migration/src/data/safe-v3-rotsee.csv"))
    } else if network_name.eq_ignore_ascii_case("jura") {
        Some(include_str!("../../migration/src/data/safe-v3-jura.csv"))
    } else {
        None
    }
}

async fn load_preseeded_safes_from_csv<'a, Db>(db: &'a Db, tx: OptTx<'a>, csv_data: &str) -> Result<usize>
where
    Db: BlokliDbSafeContractOperations + BlokliDbSafeHistoryOperations + Sync,
{
    let mut loaded = 0;

    let tx = db.nest_transaction(tx).await?;

    for (index, line) in csv_data.lines().enumerate() {
        if index == 0 {
            continue;
        }

        let Some(entry) = parse_csv_entry(line, index + 1)? else {
            continue;
        };

        let existing = HoprSafeContract::find()
            .filter(hopr_safe_contract::Column::Address.eq(entry.address.as_ref().to_vec()))
            .one(tx.as_ref())
            .await?;
        if existing.is_some() {
            continue;
        }

        db.upsert_safe_contract(
            Some(&tx),
            entry.address,
            entry.module_address,
            entry.chain_key,
            PRESEEDED_BLOCK as u64,
            entry.deployed_tx_index,
            entry.deployed_log_index,
        )
        .await?;
        persist_preseeded_safe_history(db, Some(&tx), &entry).await?;
        loaded += 1;
    }

    tx.commit().await?;
    Ok(loaded)
}

#[async_trait]
pub trait BlokliDbSafeContractOperations: BlokliDbGeneralModelOperations {
    /// Create safe contract entry from deployment event (temporal pattern).
    ///
    /// Creates both the identity record (if needed) and appends a new state record.
    ///
    /// # Arguments
    /// * `safe_address` - Safe contract address from event
    /// * `module_address` - Module contract address from event
    /// * `chain_key` - Transaction sender (owner address)
    /// * `block` - Deployment block number
    /// * `tx_index` - Transaction index
    /// * `log_index` - Log index
    ///
    /// # Idempotency
    /// Uses ON CONFLICT DO NOTHING with unique constraint on state position.
    #[allow(clippy::too_many_arguments)]
    async fn create_safe_contract<'a>(
        &'a self,
        tx: OptTx<'a>,
        safe_address: Address,
        module_address: Address,
        chain_key: Address,
        block: u64,
        tx_index: u64,
        log_index: u64,
    ) -> Result<i64>;

    /// Upsert safe contract entry - create identity if needed, always append state.
    ///
    /// This follows the temporal pattern:
    /// - If safe identity doesn't exist, creates it
    /// - Always appends a new state record (never updates existing states)
    ///
    /// # Arguments
    /// * `safe_address` - Safe contract address (lookup key)
    /// * `module_address` - Module contract address
    /// * `chain_key` - Node address binding
    /// * `block` - Event block number
    /// * `tx_index` - Event transaction index
    /// * `log_index` - Event log index
    ///
    /// # Returns
    /// The database id of the safe contract identity
    #[allow(clippy::too_many_arguments)]
    async fn upsert_safe_contract<'a>(
        &'a self,
        tx: OptTx<'a>,
        safe_address: Address,
        module_address: Address,
        chain_key: Address,
        block: u64,
        tx_index: u64,
        log_index: u64,
    ) -> Result<i64>;

    /// Verify safe contract exists and chain_key matches expected value.
    ///
    /// Uses the latest state to check the chain_key.
    ///
    /// # Returns
    /// * `Ok(true)` - Safe exists and chain_key matches
    /// * `Ok(false)` - Safe exists but chain_key does NOT match
    /// * `Err(_)` - Safe does not exist or query failed
    async fn verify_safe_contract<'a>(
        &'a self,
        tx: OptTx<'a>,
        safe_address: Address,
        expected_chain_key: Address,
    ) -> Result<bool>;

    /// Delete a safe contract entry and all its state records.
    ///
    /// Due to CASCADE on the foreign key, deleting the identity
    /// automatically deletes all associated state records.
    ///
    /// # Returns
    /// * `Ok(())` - Safe was deleted successfully
    /// * `Err(_)` - Safe does not exist or deletion failed
    async fn delete_safe_contract<'a>(&'a self, tx: OptTx<'a>, safe_address: Address) -> Result<()>;

    /// Get safe contract by address with current (latest) state.
    ///
    /// Returns the combined identity and latest state as a SafeContractEntry.
    ///
    /// # Returns
    /// * `Ok(Some(entry))` - Safe exists with state
    /// * `Ok(None)` - Safe does not exist or has no state
    async fn get_safe_contract_by_address<'a>(
        &'a self,
        tx: OptTx<'a>,
        safe_address: Address,
    ) -> Result<Option<SafeContractEntry>>;

    /// Get safe contract state at a specific block.
    ///
    /// Returns the most recent state that was published at or before the given block.
    ///
    /// # Arguments
    /// * `safe_address` - Safe contract address
    /// * `block` - Block number to query state at
    ///
    /// # Returns
    /// * `Ok(Some(entry))` - State exists at or before block
    /// * `Ok(None)` - No state exists before the given block
    async fn get_safe_contract_at_block<'a>(
        &'a self,
        tx: OptTx<'a>,
        safe_address: Address,
        block: u64,
    ) -> Result<Option<SafeContractEntry>>;

    /// Get complete state history for a safe contract.
    ///
    /// Returns all state records ordered chronologically by
    /// (block, tx_index, log_index) from earliest to latest.
    async fn get_safe_contract_history<'a>(
        &'a self,
        tx: OptTx<'a>,
        safe_address: Address,
    ) -> Result<Vec<SafeContractEntry>>;

    /// Returns all known Safe addresses ordered by insertion order.
    async fn get_safe_contract_addresses<'a>(&'a self, tx: OptTx<'a>) -> Result<Vec<Address>>;

    /// Get all safes that have only pre-seeded state.
    ///
    /// Pre-seeded safes are those whose only state record has
    /// `published_block = PRESEEDED_BLOCK` (30_000_000).
    /// These safes may have stale module addresses that need refreshing.
    ///
    /// # Returns
    /// Vector of safe entries that only have pre-seeded state
    async fn get_preseeded_safes<'a>(&'a self, tx: OptTx<'a>) -> Result<Vec<SafeContractEntry>>;

    async fn load_preseeded_safes<'a>(&'a self, tx: OptTx<'a>, network_name: &str) -> Result<usize>;

    /// Get safe contract by module address with current (latest) state.
    ///
    /// Returns the combined identity and latest state as a SafeContractEntry,
    /// looking up the safe by its associated module address instead of the safe address.
    ///
    /// # Returns
    /// * `Ok(Some(entry))` - Safe exists with matching module address
    /// * `Ok(None)` - No safe found with this module address
    async fn get_safe_contract_by_module_address<'a>(
        &'a self,
        tx: OptTx<'a>,
        module_address: Address,
    ) -> Result<Option<SafeContractEntry>>;

    #[allow(clippy::too_many_arguments)]
    async fn update_safe_module_address<'a>(
        &'a self,
        tx: OptTx<'a>,
        safe_address: Address,
        new_module_address: Address,
        block: u64,
        tx_index: u64,
        log_index: u64,
    ) -> Result<()>;
}

/// Combine identity and state into a SafeContractEntry.
fn combine_entry(identity: &hopr_safe_contract::Model, state: &hopr_safe_contract_state::Model) -> SafeContractEntry {
    SafeContractEntry {
        id: identity.id,
        address: identity.address.clone(),
        module_address: state.module_address.clone(),
        chain_key: state.chain_key.clone(),
        published_block: state.published_block,
        published_tx_index: state.published_tx_index,
        published_log_index: state.published_log_index,
    }
}

async fn get_latest_safe_state<C: ConnectionTrait>(
    conn: &C,
    safe_contract_id: i64,
) -> Result<Option<hopr_safe_contract_state::Model>> {
    HoprSafeContractState::find()
        .filter(hopr_safe_contract_state::Column::HoprSafeContractId.eq(safe_contract_id))
        .order_by_desc(hopr_safe_contract_state::Column::PublishedBlock)
        .order_by_desc(hopr_safe_contract_state::Column::PublishedTxIndex)
        .order_by_desc(hopr_safe_contract_state::Column::PublishedLogIndex)
        .one(conn)
        .await
        .map_err(Into::into)
}

#[async_trait]
impl BlokliDbSafeContractOperations for BlokliDb {
    #[allow(clippy::too_many_arguments)]
    #[allow(clippy::cast_possible_wrap)]
    async fn create_safe_contract<'a>(
        &'a self,
        tx: OptTx<'a>,
        safe_address: Address,
        module_address: Address,
        chain_key: Address,
        block: u64,
        tx_index: u64,
        log_index: u64,
    ) -> Result<i64> {
        // Delegate to upsert which handles both identity creation and state append
        self.upsert_safe_contract(tx, safe_address, module_address, chain_key, block, tx_index, log_index)
            .await
    }

    #[allow(clippy::too_many_arguments)]
    #[allow(clippy::cast_possible_wrap)]
    async fn upsert_safe_contract<'a>(
        &'a self,
        tx: OptTx<'a>,
        safe_address: Address,
        module_address: Address,
        chain_key: Address,
        block: u64,
        tx_index: u64,
        log_index: u64,
    ) -> Result<i64> {
        let tx = self.nest_transaction(tx).await?;

        // Step 1: Find or create safe identity
        let existing_safe = HoprSafeContract::find()
            .filter(hopr_safe_contract::Column::Address.eq(safe_address.as_ref().to_vec()))
            .one(tx.as_ref())
            .await?;

        let safe_id = match existing_safe {
            Some(safe) => safe.id,
            None => {
                // Create new identity
                let identity_model = hopr_safe_contract::ActiveModel {
                    address: Set(safe_address.as_ref().to_vec()),
                    ..Default::default()
                };
                let result = HoprSafeContract::insert(identity_model).exec(tx.as_ref()).await?;
                result.last_insert_id
            }
        };

        // Step 2: Insert new state record (append-only)
        let state_model = hopr_safe_contract_state::ActiveModel {
            hopr_safe_contract_id: Set(safe_id),
            module_address: Set(module_address.as_ref().to_vec()),
            chain_key: Set(chain_key.as_ref().to_vec()),
            published_block: Set(block as i64),
            published_tx_index: Set(tx_index as i64),
            published_log_index: Set(log_index as i64),
            ..Default::default()
        };

        // Use ON CONFLICT DO NOTHING for idempotency on state position
        match HoprSafeContractState::insert(state_model)
            .on_conflict(
                OnConflict::columns([
                    hopr_safe_contract_state::Column::HoprSafeContractId,
                    hopr_safe_contract_state::Column::PublishedBlock,
                    hopr_safe_contract_state::Column::PublishedTxIndex,
                    hopr_safe_contract_state::Column::PublishedLogIndex,
                ])
                .do_nothing()
                .to_owned(),
            )
            .exec(tx.as_ref())
            .await
        {
            Ok(_) => {}
            Err(sea_orm::DbErr::RecordNotInserted) => {
                // Expected: ON CONFLICT DO NOTHING prevented duplicate insert (idempotent)
            }
            Err(e) => return Err(e.into()),
        }

        tx.commit().await?;
        Ok(safe_id)
    }

    async fn verify_safe_contract<'a>(
        &'a self,
        tx: OptTx<'a>,
        safe_address: Address,
        expected_chain_key: Address,
    ) -> Result<bool> {
        // Get the safe with its latest state
        let entry = self.get_safe_contract_by_address(tx, safe_address).await?;

        match entry {
            Some(safe) => {
                let chain_key_match = safe.chain_key == expected_chain_key.as_ref().to_vec();
                Ok(chain_key_match)
            }
            None => Err(DbSqlError::EntityNotFound(format!(
                "Safe contract not found: {}",
                safe_address
            ))),
        }
    }

    async fn delete_safe_contract<'a>(&'a self, tx: OptTx<'a>, safe_address: Address) -> Result<()> {
        let tx = self.nest_transaction(tx).await?;

        // Find the safe identity
        let safe = HoprSafeContract::find()
            .filter(hopr_safe_contract::Column::Address.eq(safe_address.as_ref().to_vec()))
            .one(tx.as_ref())
            .await?
            .ok_or_else(|| DbSqlError::EntityNotFound(format!("Safe contract not found: {}", safe_address)))?;

        // Delete the identity (CASCADE will delete all state records)
        sea_orm::ModelTrait::delete(safe, tx.as_ref()).await?;

        tx.commit().await?;
        Ok(())
    }

    async fn get_safe_contract_by_address<'a>(
        &'a self,
        tx: OptTx<'a>,
        safe_address: Address,
    ) -> Result<Option<SafeContractEntry>> {
        let tx = self.nest_transaction(tx).await?;
        let Some(identity) = HoprSafeContract::find()
            .filter(hopr_safe_contract::Column::Address.eq(safe_address.as_ref().to_vec()))
            .one(tx.as_ref())
            .await?
        else {
            return Ok(None);
        };

        let Some(state) = get_latest_safe_state(tx.as_ref(), identity.id).await? else {
            return Ok(None);
        };

        Ok(Some(combine_entry(&identity, &state)))
    }

    #[allow(clippy::cast_possible_wrap)]
    async fn get_safe_contract_at_block<'a>(
        &'a self,
        tx: OptTx<'a>,
        safe_address: Address,
        block: u64,
    ) -> Result<Option<SafeContractEntry>> {
        let tx = self.nest_transaction(tx).await?;

        // Step 1: Find safe identity by address
        let identity = HoprSafeContract::find()
            .filter(hopr_safe_contract::Column::Address.eq(safe_address.as_ref().to_vec()))
            .one(tx.as_ref())
            .await?;

        let identity = match identity {
            Some(i) => i,
            None => return Ok(None),
        };

        // Step 2: Get the most recent state at or before the given block
        let state = HoprSafeContractState::find()
            .filter(hopr_safe_contract_state::Column::HoprSafeContractId.eq(identity.id))
            .filter(hopr_safe_contract_state::Column::PublishedBlock.lte(block as i64))
            .order_by_desc(hopr_safe_contract_state::Column::PublishedBlock)
            .order_by_desc(hopr_safe_contract_state::Column::PublishedTxIndex)
            .order_by_desc(hopr_safe_contract_state::Column::PublishedLogIndex)
            .one(tx.as_ref())
            .await?;

        match state {
            Some(s) => Ok(Some(combine_entry(&identity, &s))),
            None => Ok(None), // No state at or before this block
        }
    }

    async fn get_safe_contract_history<'a>(
        &'a self,
        tx: OptTx<'a>,
        safe_address: Address,
    ) -> Result<Vec<SafeContractEntry>> {
        let tx = self.nest_transaction(tx).await?;

        // Step 1: Find safe identity by address
        let identity = HoprSafeContract::find()
            .filter(hopr_safe_contract::Column::Address.eq(safe_address.as_ref().to_vec()))
            .one(tx.as_ref())
            .await?;

        let identity = match identity {
            Some(i) => i,
            None => return Ok(vec![]),
        };

        // Step 2: Get all state records ordered chronologically
        let states = HoprSafeContractState::find()
            .filter(hopr_safe_contract_state::Column::HoprSafeContractId.eq(identity.id))
            .order_by_asc(hopr_safe_contract_state::Column::PublishedBlock)
            .order_by_asc(hopr_safe_contract_state::Column::PublishedTxIndex)
            .order_by_asc(hopr_safe_contract_state::Column::PublishedLogIndex)
            .all(tx.as_ref())
            .await?;

        // Combine identity with each state
        Ok(states.iter().map(|s| combine_entry(&identity, s)).collect())
    }

    async fn get_safe_contract_addresses<'a>(&'a self, tx: OptTx<'a>) -> Result<Vec<Address>> {
        let tx = self.nest_transaction(tx).await?;

        let safes = HoprSafeContract::find()
            .order_by_asc(hopr_safe_contract::Column::Id)
            .all(tx.as_ref())
            .await?;

        let addresses = safes
            .into_iter()
            .map(|safe| {
                Address::try_from(safe.address.as_slice())
                    .map_err(|e| DbSqlError::Construction(format!("invalid safe address length: {e}")))
            })
            .collect::<Result<Vec<_>>>()?;

        tx.commit().await?;
        Ok(addresses)
    }

    async fn load_preseeded_safes<'a>(&'a self, tx: OptTx<'a>, network_name: &str) -> Result<usize> {
        let Some(csv_data) = preseeded_csv_for_network(network_name) else {
            return Ok(0);
        };
        load_preseeded_safes_from_csv(self, tx, csv_data).await
    }

    async fn get_safe_contract_by_module_address<'a>(
        &'a self,
        tx: OptTx<'a>,
        module_address: Address,
    ) -> Result<Option<SafeContractEntry>> {
        let tx = self.nest_transaction(tx).await?;
        let candidate_states = HoprSafeContractState::find()
            .filter(hopr_safe_contract_state::Column::ModuleAddress.eq(module_address.as_ref().to_vec()))
            .order_by_desc(hopr_safe_contract_state::Column::PublishedBlock)
            .order_by_desc(hopr_safe_contract_state::Column::PublishedTxIndex)
            .order_by_desc(hopr_safe_contract_state::Column::PublishedLogIndex)
            .all(tx.as_ref())
            .await?;

        let mut seen_safe_ids = HashSet::new();
        for candidate_state in candidate_states {
            if !seen_safe_ids.insert(candidate_state.hopr_safe_contract_id) {
                continue;
            }

            let Some(latest_state) = get_latest_safe_state(tx.as_ref(), candidate_state.hopr_safe_contract_id).await?
            else {
                continue;
            };
            if latest_state.id != candidate_state.id {
                continue;
            }

            let identity = HoprSafeContract::find_by_id(candidate_state.hopr_safe_contract_id)
                .one(tx.as_ref())
                .await?
                .ok_or_else(|| {
                    DbSqlError::EntityNotFound(format!(
                        "safe contract identity not found for id {}",
                        candidate_state.hopr_safe_contract_id
                    ))
                })?;

            return Ok(Some(combine_entry(&identity, &latest_state)));
        }

        Ok(None)
    }

    async fn get_preseeded_safes<'a>(&'a self, tx: OptTx<'a>) -> Result<Vec<SafeContractEntry>> {
        let tx = self.nest_transaction(tx).await?;
        let states = HoprSafeContractState::find()
            .order_by_asc(hopr_safe_contract_state::Column::HoprSafeContractId)
            .order_by_asc(hopr_safe_contract_state::Column::PublishedBlock)
            .order_by_asc(hopr_safe_contract_state::Column::PublishedTxIndex)
            .order_by_asc(hopr_safe_contract_state::Column::PublishedLogIndex)
            .all(tx.as_ref())
            .await?;

        let mut grouped_states = Vec::new();
        let mut current_safe_id = None;
        let mut first_block = PRESEEDED_BLOCK;
        let mut last_state: Option<hopr_safe_contract_state::Model> = None;

        for state in states {
            match current_safe_id {
                Some(safe_id) if safe_id != state.hopr_safe_contract_id => {
                    if first_block == PRESEEDED_BLOCK {
                        if let Some(latest) = last_state.take() {
                            if latest.published_block == PRESEEDED_BLOCK {
                                grouped_states.push((safe_id, latest));
                            }
                        }
                    }
                    current_safe_id = Some(state.hopr_safe_contract_id);
                    first_block = state.published_block;
                    last_state = Some(state);
                }
                Some(_) => {
                    last_state = Some(state);
                }
                None => {
                    current_safe_id = Some(state.hopr_safe_contract_id);
                    first_block = state.published_block;
                    last_state = Some(state);
                }
            }
        }

        if let Some(safe_id) = current_safe_id {
            if first_block == PRESEEDED_BLOCK {
                if let Some(latest) = last_state.take() {
                    if latest.published_block == PRESEEDED_BLOCK {
                        grouped_states.push((safe_id, latest));
                    }
                }
            }
        }

        if grouped_states.is_empty() {
            return Ok(Vec::new());
        }

        let identities = HoprSafeContract::find()
            .filter(hopr_safe_contract::Column::Id.is_in(grouped_states.iter().map(|(id, _)| *id).collect::<Vec<_>>()))
            .all(tx.as_ref())
            .await?;
        let identities_by_id = identities
            .into_iter()
            .map(|identity| (identity.id, identity))
            .collect::<HashMap<_, _>>();

        Ok(grouped_states
            .into_iter()
            .filter_map(|(safe_id, state)| {
                identities_by_id
                    .get(&safe_id)
                    .map(|identity| combine_entry(identity, &state))
            })
            .collect())
    }

    #[allow(clippy::too_many_arguments)]
    async fn update_safe_module_address<'a>(
        &'a self,
        tx: OptTx<'a>,
        safe_address: Address,
        new_module_address: Address,
        block: u64,
        tx_index: u64,
        log_index: u64,
    ) -> Result<()> {
        let tx = self.nest_transaction(tx).await?;

        let identity = HoprSafeContract::find()
            .filter(hopr_safe_contract::Column::Address.eq(safe_address.as_ref().to_vec()))
            .one(tx.as_ref())
            .await?
            .ok_or_else(|| DbSqlError::EntityNotFound(format!("Safe contract not found: {}", safe_address)))?;

        let current_state = HoprSafeContractState::find()
            .filter(hopr_safe_contract_state::Column::HoprSafeContractId.eq(identity.id))
            .order_by_desc(hopr_safe_contract_state::Column::PublishedBlock)
            .order_by_desc(hopr_safe_contract_state::Column::PublishedTxIndex)
            .order_by_desc(hopr_safe_contract_state::Column::PublishedLogIndex)
            .one(tx.as_ref())
            .await?
            .ok_or_else(|| DbSqlError::EntityNotFound(format!("Safe contract state not found: {}", safe_address)))?;

        let published_block = i64::try_from(block).map_err(|_| {
            DbSqlError::Construction(format!("Block number {} out of range for safe module update", block))
        })?;
        let published_tx_index = i64::try_from(tx_index).map_err(|_| {
            DbSqlError::Construction(format!(
                "Transaction index {} out of range for safe module update",
                tx_index
            ))
        })?;
        let published_log_index = i64::try_from(log_index).map_err(|_| {
            DbSqlError::Construction(format!("Log index {} out of range for safe module update", log_index))
        })?;

        let state_model = hopr_safe_contract_state::ActiveModel {
            hopr_safe_contract_id: Set(identity.id),
            module_address: Set(new_module_address.as_ref().to_vec()),
            chain_key: Set(current_state.chain_key),
            published_block: Set(published_block),
            published_tx_index: Set(published_tx_index),
            published_log_index: Set(published_log_index),
            ..Default::default()
        };

        match HoprSafeContractState::insert(state_model)
            .on_conflict(
                OnConflict::columns([
                    hopr_safe_contract_state::Column::HoprSafeContractId,
                    hopr_safe_contract_state::Column::PublishedBlock,
                    hopr_safe_contract_state::Column::PublishedTxIndex,
                    hopr_safe_contract_state::Column::PublishedLogIndex,
                ])
                .do_nothing()
                .to_owned(),
            )
            .exec(tx.as_ref())
            .await
        {
            Ok(_) => {}
            Err(sea_orm::DbErr::RecordNotInserted) => {
                // Expected: ON CONFLICT DO NOTHING prevented duplicate insert (idempotent)
            }
            Err(e) => return Err(e.into()),
        }

        tx.commit().await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use hopr_types::primitive::prelude::ToHex;
    use sea_orm::PaginatorTrait;

    use super::*;
    use crate::{
        db::BlokliDb,
        safe_history::{BlokliDbSafeHistoryOperations, SafeActivityKind},
    };

    /// Generates a new random `Address` from cryptographically secure random bytes.
    fn random_address() -> Address {
        Address::from(hopr_types::crypto_random::random_bytes())
    }

    #[test]
    fn test_preseeded_csv_for_network() {
        assert!(preseeded_csv_for_network("rotsee").is_some());
        assert!(preseeded_csv_for_network("ROTSEE").is_some());
        assert!(preseeded_csv_for_network("unknown").is_none());
        assert!(preseeded_csv_for_network("jURA").is_some());
    }

    #[tokio::test]
    async fn test_load_preseeded_safes_from_csv() -> anyhow::Result<()> {
        let db = BlokliDb::new_in_memory().await?;

        let safe_address = random_address();
        let module_address = random_address();
        let chain_key = random_address();
        let owner_one: Address = "1111111111111111111111111111111111111111".parse()?;
        let owner_two: Address = "2222222222222222222222222222222222222222".parse()?;

        let headers =
            "address,module_address,chain_key,deployed_block,owners,threshold,deployed_tx_index,deployed_log_index";

        let values = format!(
            "{},{},{},30000000,{};{},2,0,1",
            safe_address.to_hex(),
            module_address.to_hex(),
            chain_key.to_hex(),
            owner_one.to_hex(),
            owner_two.to_hex()
        );

        let csv_data = vec![headers, &values].join("\n");

        let loaded = load_preseeded_safes_from_csv(&db, None, &csv_data).await?;
        assert_eq!(loaded, 1);

        let owners = db.get_safe_owners(None, safe_address).await?;
        assert_eq!(owners, vec![owner_one, owner_two]);

        let activity = db.get_safe_activity(None, safe_address).await?;
        assert_eq!(activity.len(), 1);
        assert_eq!(activity[0].event_kind, SafeActivityKind::SafeSetup.as_str());
        assert_eq!(activity[0].threshold.as_deref(), Some("2"));
        assert_eq!(activity[0].published_tx_index, 0);
        assert_eq!(activity[0].published_log_index, 1);

        Ok(())
    }

    // ============================================================================
    // TDD Tests for Temporal Safe Contract Operations
    // ============================================================================

    /// Test that module address changes are tracked in history (temporal pattern).
    #[tokio::test]
    async fn test_safe_contract_state_history() -> anyhow::Result<()> {
        let db = BlokliDb::new_in_memory().await?;

        let safe_addr = random_address();
        let module_v1 = random_address();
        let module_v2 = random_address();
        let chain_key = random_address();

        // Create initial state at block 100
        db.upsert_safe_contract(None, safe_addr, module_v1, chain_key, 100, 0, 0)
            .await?;

        // Update module (should append new state, not overwrite)
        db.upsert_safe_contract(None, safe_addr, module_v2, chain_key, 200, 0, 0)
            .await?;

        // Get current (should be v2)
        let current = db
            .get_safe_contract_by_address(None, safe_addr)
            .await?
            .expect("safe should exist");
        assert_eq!(
            current.module_address,
            module_v2.as_ref().to_vec(),
            "current module should be v2"
        );

        // Get state at block 100 (should be v1)
        let at_100 = db
            .get_safe_contract_at_block(None, safe_addr, 100)
            .await?
            .expect("historical state should exist");
        assert_eq!(
            at_100.module_address,
            module_v1.as_ref().to_vec(),
            "historical module at block 100 should be v1"
        );

        Ok(())
    }

    /// Test that pre-seeded safes can be identified by their special block number.
    #[tokio::test]
    async fn test_get_preseeded_safes() -> anyhow::Result<()> {
        let db = BlokliDb::new_in_memory().await?;

        let safe1 = random_address();
        let module1 = random_address();
        let key1 = random_address();

        let safe2 = random_address();
        let module2 = random_address();
        let key2 = random_address();

        // Create pre-seeded safe (block 30000000)
        db.upsert_safe_contract(None, safe1, module1, key1, PRESEEDED_BLOCK as u64, 0, 0)
            .await?;

        // Create indexed safe (real block)
        db.upsert_safe_contract(None, safe2, module2, key2, 44_000_000, 0, 0)
            .await?;

        // Get only pre-seeded safes
        let preseeded = db.get_preseeded_safes(None).await?;

        assert_eq!(preseeded.len(), 1, "should only return pre-seeded safe");
        assert_eq!(
            preseeded[0].address,
            safe1.as_ref().to_vec(),
            "pre-seeded safe should be safe1"
        );

        Ok(())
    }

    /// Test that full history can be retrieved for a safe contract.
    #[tokio::test]
    async fn test_get_safe_contract_history() -> anyhow::Result<()> {
        let db = BlokliDb::new_in_memory().await?;

        let safe_addr = random_address();
        let chain_key = random_address();

        // Create 3 state changes
        for i in 0..3 {
            let module = random_address();
            db.upsert_safe_contract(None, safe_addr, module, chain_key, 100 + i * 100, 0, 0)
                .await?;
        }

        // Get full history
        let history = db.get_safe_contract_history(None, safe_addr).await?;

        assert_eq!(history.len(), 3, "should have 3 state records");

        Ok(())
    }

    /// Test that pre-seeded safe with updated module is no longer returned by get_preseeded_safes.
    #[tokio::test]
    async fn test_preseeded_safe_with_update_not_returned() -> anyhow::Result<()> {
        let db = BlokliDb::new_in_memory().await?;

        let safe1 = random_address();
        let module1 = random_address();
        let key1 = random_address();

        // Create pre-seeded safe
        db.upsert_safe_contract(None, safe1, module1, key1, PRESEEDED_BLOCK as u64, 0, 0)
            .await?;

        // Verify it shows up in pre-seeded list
        let preseeded = db.get_preseeded_safes(None).await?;
        assert_eq!(preseeded.len(), 1);

        // Update the safe with a real indexed event
        let module2 = random_address();
        db.upsert_safe_contract(None, safe1, module2, key1, 44_000_000, 5, 0)
            .await?;

        // Now it should NOT show up in pre-seeded list (it has more than one state)
        let preseeded_after = db.get_preseeded_safes(None).await?;
        assert_eq!(
            preseeded_after.len(),
            0,
            "safe with indexed events should not be in pre-seeded list"
        );

        Ok(())
    }

    #[tokio::test]
    async fn test_create_safe_contract() -> anyhow::Result<()> {
        let db = BlokliDb::new_in_memory().await?;

        let safe_address = random_address();
        let module_address = random_address();
        let chain_key = random_address();

        // Create safe contract
        let id = db
            .create_safe_contract(None, safe_address, module_address, chain_key, 100, 0, 0)
            .await?;

        // Verify it was created
        let safe = db
            .get_safe_contract_by_address(None, safe_address)
            .await?
            .expect("safe should exist");

        assert_eq!(safe.id, id);
        assert_eq!(safe.address, safe_address.as_ref().to_vec());
        assert_eq!(safe.module_address, module_address.as_ref().to_vec());
        assert_eq!(safe.chain_key, chain_key.as_ref().to_vec());
        assert_eq!(safe.published_block, 100);

        Ok(())
    }

    #[tokio::test]
    async fn test_create_safe_contract_idempotency() -> anyhow::Result<()> {
        let db = BlokliDb::new_in_memory().await?;

        let safe_address = random_address();
        let module_address = random_address();
        let chain_key = random_address();

        // Count pre-seeded safes (from migration CSV data)
        let initial_count = HoprSafeContract::find().count(db.conn(crate::TargetDb::Index)).await?;

        // Create safe contract
        let id1 = db
            .create_safe_contract(None, safe_address, module_address, chain_key, 100, 0, 0)
            .await?;

        // Verify one new identity was created
        let count_after_create = HoprSafeContract::find().count(db.conn(crate::TargetDb::Index)).await?;
        assert_eq!(count_after_create, initial_count + 1);

        // Try to create same safe contract (same block/tx/log) - should be idempotent
        let id2 = db
            .create_safe_contract(None, safe_address, module_address, chain_key, 100, 0, 0)
            .await?;

        // Should return same ID
        assert_eq!(id1, id2);

        // Verify no additional identities were created
        let count_after_duplicate = HoprSafeContract::find().count(db.conn(crate::TargetDb::Index)).await?;
        assert_eq!(count_after_duplicate, initial_count + 1);

        // Verify only one state exists (due to ON CONFLICT DO NOTHING)
        let history = db.get_safe_contract_history(None, safe_address).await?;
        assert_eq!(history.len(), 1, "should have only 1 state record");

        Ok(())
    }

    #[tokio::test]
    async fn test_verify_safe_contract() -> anyhow::Result<()> {
        let db = BlokliDb::new_in_memory().await?;

        let safe_address = random_address();
        let module_address = random_address();
        let chain_key = random_address();
        let other_chain_key = random_address();

        // Case 1: Safe doesn't exist
        let result = db.verify_safe_contract(None, safe_address, chain_key).await;
        assert!(matches!(result, Err(DbSqlError::EntityNotFound(_))));

        // Create safe contract
        db.create_safe_contract(None, safe_address, module_address, chain_key, 100, 0, 0)
            .await?;

        // Case 2: Safe exists and chain_key matches
        let result = db.verify_safe_contract(None, safe_address, chain_key).await?;
        assert!(result);

        // Case 3: Safe exists but chain_key mismatch
        let result = db.verify_safe_contract(None, safe_address, other_chain_key).await?;
        assert!(!result);

        Ok(())
    }

    #[tokio::test]
    async fn test_delete_safe_contract() -> anyhow::Result<()> {
        let db = BlokliDb::new_in_memory().await?;

        let safe_address = random_address();
        let module_address = random_address();
        let chain_key = random_address();

        // Case 1: Safe doesn't exist
        let result = db.delete_safe_contract(None, safe_address).await;
        assert!(matches!(result, Err(DbSqlError::EntityNotFound(_))));

        // Create safe contract
        let id = db
            .create_safe_contract(None, safe_address, module_address, chain_key, 100, 0, 0)
            .await?;

        // Verify safe exists
        let safe = db.get_safe_contract_by_address(None, safe_address).await?;
        assert!(safe.is_some());

        // Delete the safe
        db.delete_safe_contract(None, safe_address).await?;

        // Verify safe is deleted
        let safe = db.get_safe_contract_by_address(None, safe_address).await?;
        assert!(safe.is_none());

        // Verify identity is deleted
        let identity = HoprSafeContract::find_by_id(id)
            .one(db.conn(crate::TargetDb::Index))
            .await?;
        assert!(identity.is_none());

        Ok(())
    }

    #[tokio::test]
    async fn test_state_at_block_boundary() -> anyhow::Result<()> {
        let db = BlokliDb::new_in_memory().await?;

        let safe_addr = random_address();
        let module_v1 = random_address();
        let module_v2 = random_address();
        let chain_key = random_address();

        // Create states at blocks 100 and 200
        db.upsert_safe_contract(None, safe_addr, module_v1, chain_key, 100, 0, 0)
            .await?;
        db.upsert_safe_contract(None, safe_addr, module_v2, chain_key, 200, 0, 0)
            .await?;

        // Query at block 50 - should return None (no state before block 100)
        let at_50 = db.get_safe_contract_at_block(None, safe_addr, 50).await?;
        assert!(at_50.is_none(), "no state should exist before block 100");

        // Query at block 100 - should return v1
        let at_100 = db
            .get_safe_contract_at_block(None, safe_addr, 100)
            .await?
            .expect("state should exist at block 100");
        assert_eq!(at_100.module_address, module_v1.as_ref().to_vec());

        // Query at block 150 - should return v1 (most recent before 150)
        let at_150 = db
            .get_safe_contract_at_block(None, safe_addr, 150)
            .await?
            .expect("state should exist at block 150");
        assert_eq!(at_150.module_address, module_v1.as_ref().to_vec());

        // Query at block 200 - should return v2
        let at_200 = db
            .get_safe_contract_at_block(None, safe_addr, 200)
            .await?
            .expect("state should exist at block 200");
        assert_eq!(at_200.module_address, module_v2.as_ref().to_vec());

        // Query at block 300 - should return v2 (most recent)
        let at_300 = db
            .get_safe_contract_at_block(None, safe_addr, 300)
            .await?
            .expect("state should exist at block 300");
        assert_eq!(at_300.module_address, module_v2.as_ref().to_vec());

        Ok(())
    }

    #[tokio::test]
    async fn test_get_safe_contract_by_module_address() -> anyhow::Result<()> {
        let db = BlokliDb::new_in_memory().await?;

        let safe_addr = random_address();
        let module_addr = random_address();
        let chain_key = random_address();

        // Module address not found initially
        let result = db.get_safe_contract_by_module_address(None, module_addr).await?;
        assert!(
            result.is_none(),
            "should not find safe by module address before creation"
        );

        // Create safe contract
        db.upsert_safe_contract(None, safe_addr, module_addr, chain_key, 100, 0, 0)
            .await?;

        // Look up by module address
        let entry = db
            .get_safe_contract_by_module_address(None, module_addr)
            .await?
            .expect("should find safe by module address");
        assert_eq!(entry.address, safe_addr.as_ref().to_vec());
        assert_eq!(entry.module_address, module_addr.as_ref().to_vec());

        // Look up by a different module address returns None
        let other_module = random_address();
        let result = db.get_safe_contract_by_module_address(None, other_module).await?;
        assert!(result.is_none(), "should not find safe by unrelated module address");

        Ok(())
    }

    #[tokio::test]
    async fn test_multiple_safes_isolation() -> anyhow::Result<()> {
        let db = BlokliDb::new_in_memory().await?;

        let safe1 = random_address();
        let safe2 = random_address();
        let module1 = random_address();
        let module2 = random_address();
        let chain_key = random_address();

        // Create two different safes
        db.upsert_safe_contract(None, safe1, module1, chain_key, 100, 0, 0)
            .await?;
        db.upsert_safe_contract(None, safe2, module2, chain_key, 100, 0, 0)
            .await?;

        // Verify they are distinct
        let entry1 = db
            .get_safe_contract_by_address(None, safe1)
            .await?
            .expect("safe1 should exist");
        let entry2 = db
            .get_safe_contract_by_address(None, safe2)
            .await?
            .expect("safe2 should exist");

        assert_ne!(entry1.id, entry2.id);
        assert_eq!(entry1.module_address, module1.as_ref().to_vec());
        assert_eq!(entry2.module_address, module2.as_ref().to_vec());

        // Update safe1 - should not affect safe2
        let module1_v2 = random_address();
        db.upsert_safe_contract(None, safe1, module1_v2, chain_key, 200, 0, 0)
            .await?;

        let entry1_updated = db
            .get_safe_contract_by_address(None, safe1)
            .await?
            .expect("safe1 should exist");
        let entry2_unchanged = db
            .get_safe_contract_by_address(None, safe2)
            .await?
            .expect("safe2 should exist");

        assert_eq!(entry1_updated.module_address, module1_v2.as_ref().to_vec());
        assert_eq!(entry2_unchanged.module_address, module2.as_ref().to_vec());

        Ok(())
    }
}
