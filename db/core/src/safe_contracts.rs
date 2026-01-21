use async_trait::async_trait;
use blokli_db_entity::{
    hopr_safe_contract, hopr_safe_contract_state,
    prelude::{HoprSafeContract, HoprSafeContractState},
};
use hopr_primitive_types::prelude::Address;
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter, QueryOrder, Set};
use sea_query::OnConflict;

use crate::{BlokliDb, BlokliDbGeneralModelOperations, DbSqlError, OptTx, Result};

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

    /// Get all safes that have only pre-seeded state.
    ///
    /// Pre-seeded safes are those whose only state record has
    /// `published_block = PRESEEDED_BLOCK` (30_000_000).
    /// These safes may have stale module addresses that need refreshing.
    ///
    /// # Returns
    /// Vector of safe entries that only have pre-seeded state
    async fn get_preseeded_safes<'a>(&'a self, tx: OptTx<'a>) -> Result<Vec<SafeContractEntry>>;
}

/// Combine identity and state into a SafeContractEntry.
fn combine_entry(
    identity: &hopr_safe_contract::Model,
    state: &hopr_safe_contract_state::Model,
) -> SafeContractEntry {
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
        let _ = HoprSafeContractState::insert(state_model)
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
            .await;

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

        // Step 1: Find safe identity by address
        let identity = HoprSafeContract::find()
            .filter(hopr_safe_contract::Column::Address.eq(safe_address.as_ref().to_vec()))
            .one(tx.as_ref())
            .await?;

        let identity = match identity {
            Some(i) => i,
            None => return Ok(None),
        };

        // Step 2: Get the latest state
        let state = HoprSafeContractState::find()
            .filter(hopr_safe_contract_state::Column::HoprSafeContractId.eq(identity.id))
            .order_by_desc(hopr_safe_contract_state::Column::PublishedBlock)
            .order_by_desc(hopr_safe_contract_state::Column::PublishedTxIndex)
            .order_by_desc(hopr_safe_contract_state::Column::PublishedLogIndex)
            .one(tx.as_ref())
            .await?;

        match state {
            Some(s) => Ok(Some(combine_entry(&identity, &s))),
            None => Ok(None), // Identity exists but no state - shouldn't happen normally
        }
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

    async fn get_preseeded_safes<'a>(&'a self, tx: OptTx<'a>) -> Result<Vec<SafeContractEntry>> {
        let tx = self.nest_transaction(tx).await?;

        // Find all safes that have exactly one state record at PRESEEDED_BLOCK
        // This is a bit complex - we need safes where:
        // 1. They have a state at PRESEEDED_BLOCK
        // 2. They don't have any state at other blocks

        // First, get all safe IDs that have ONLY pre-seeded state
        // We do this by finding safes where max(published_block) == min(published_block) == PRESEEDED_BLOCK

        // For simplicity and correctness, we'll query all safes and filter in Rust
        // This could be optimized with raw SQL if performance becomes an issue

        let all_identities = HoprSafeContract::find().all(tx.as_ref()).await?;

        let mut preseeded = Vec::new();

        for identity in all_identities {
            // Get all states for this safe
            let states = HoprSafeContractState::find()
                .filter(hopr_safe_contract_state::Column::HoprSafeContractId.eq(identity.id))
                .all(tx.as_ref())
                .await?;

            // Check if all states are at PRESEEDED_BLOCK
            if !states.is_empty() && states.iter().all(|s| s.published_block == PRESEEDED_BLOCK) {
                // Use the first (and only) state
                if let Some(state) = states.first() {
                    preseeded.push(combine_entry(&identity, state));
                }
            }
        }

        Ok(preseeded)
    }
}

#[cfg(test)]
mod tests {
    use sea_orm::PaginatorTrait;

    use super::*;
    use crate::db::BlokliDb;

    /// Generates a new random `Address` from cryptographically secure random bytes.
    fn random_address() -> Address {
        Address::from(hopr_crypto_random::random_bytes())
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
