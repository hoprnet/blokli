use async_trait::async_trait;
use hopr_types::{
    crypto::types::Hash,
    primitive::prelude::{Address, ToHex},
};
use sea_orm::{ConnectionTrait, DatabaseBackend, QueryResult, Statement};

use crate::{
    BlokliDb, BlokliDbGeneralModelOperations, DbSqlError, OptTx, Result, TargetDb,
    safe_contracts::BlokliDbSafeContractOperations,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SafeActivityKind {
    SafeSetup,
    AddedOwner,
    RemovedOwner,
    ChangedThreshold,
    ExecutionSuccess,
    ExecutionFailure,
}

impl SafeActivityKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::SafeSetup => "SAFE_SETUP",
            Self::AddedOwner => "ADDED_OWNER",
            Self::RemovedOwner => "REMOVED_OWNER",
            Self::ChangedThreshold => "CHANGED_THRESHOLD",
            Self::ExecutionSuccess => "EXECUTION_SUCCESS",
            Self::ExecutionFailure => "EXECUTION_FAILURE",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SafeActivityEntry {
    pub safe_address: Vec<u8>,
    pub event_kind: String,
    pub chain_tx_hash: Vec<u8>,
    pub safe_tx_hash: Option<Vec<u8>>,
    pub owner_address: Option<Vec<u8>>,
    pub threshold: Option<String>,
    pub payment: Option<String>,
    pub initiator_address: Option<Vec<u8>>,
    pub published_block: i64,
    pub published_tx_index: i64,
    pub published_log_index: i64,
}

fn sql_placeholder(backend: DatabaseBackend, position: usize) -> String {
    if backend == DatabaseBackend::Postgres {
        format!("${position}")
    } else {
        "?".to_string()
    }
}

fn safe_owner_state_insert_statement(
    backend: DatabaseBackend,
    safe_contract_id: i64,
    owner_address: Address,
    is_current_owner: bool,
    block: u64,
    tx_index: u64,
    log_index: u64,
) -> Statement {
    let placeholders = (1..=6)
        .map(|position| sql_placeholder(backend, position))
        .collect::<Vec<_>>();

    let sql = format!(
        "INSERT INTO hopr_safe_owner_state (
            hopr_safe_contract_id,
            owner_address,
            is_current_owner,
            published_block,
            published_tx_index,
            published_log_index
        ) VALUES ({}, {}, {}, {}, {}, {})
        ON CONFLICT DO NOTHING",
        placeholders[0], placeholders[1], placeholders[2], placeholders[3], placeholders[4], placeholders[5],
    );

    Statement::from_sql_and_values(
        backend,
        sql,
        vec![
            safe_contract_id.into(),
            owner_address.as_ref().to_vec().into(),
            is_current_owner.into(),
            block.cast_signed().into(),
            tx_index.cast_signed().into(),
            log_index.cast_signed().into(),
        ],
    )
}

#[allow(clippy::too_many_arguments)]
fn safe_activity_insert_statement(
    backend: DatabaseBackend,
    safe_contract_id: i64,
    event_kind: SafeActivityKind,
    chain_tx_hash: Hash,
    safe_tx_hash: Option<Hash>,
    owner_address: Option<Address>,
    threshold: Option<String>,
    payment: Option<String>,
    initiator_address: Option<Address>,
    block: u64,
    tx_index: u64,
    log_index: u64,
) -> Statement {
    let placeholders = (1..=11)
        .map(|position| sql_placeholder(backend, position))
        .collect::<Vec<_>>();

    let sql = format!(
        "INSERT INTO hopr_safe_activity (
            hopr_safe_contract_id,
            event_kind,
            chain_tx_hash,
            safe_tx_hash,
            owner_address,
            threshold,
            payment,
            initiator_address,
            published_block,
            published_tx_index,
            published_log_index
        ) VALUES ({}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {})
        ON CONFLICT DO NOTHING",
        placeholders[0],
        placeholders[1],
        placeholders[2],
        placeholders[3],
        placeholders[4],
        placeholders[5],
        placeholders[6],
        placeholders[7],
        placeholders[8],
        placeholders[9],
        placeholders[10],
    );

    Statement::from_sql_and_values(
        backend,
        sql,
        vec![
            safe_contract_id.into(),
            event_kind.as_str().into(),
            chain_tx_hash.as_ref().to_vec().into(),
            safe_tx_hash.map(|hash| hash.as_ref().to_vec()).into(),
            owner_address.map(|address| address.as_ref().to_vec()).into(),
            threshold.into(),
            payment.into(),
            initiator_address.map(|address| address.as_ref().to_vec()).into(),
            block.cast_signed().into(),
            tx_index.cast_signed().into(),
            log_index.cast_signed().into(),
        ],
    )
}

fn safe_owner_state_id_statement(
    backend: DatabaseBackend,
    safe_contract_id: i64,
    owner_address: Address,
    block: u64,
    tx_index: u64,
    log_index: u64,
) -> Statement {
    let placeholders = (1..=5)
        .map(|position| sql_placeholder(backend, position))
        .collect::<Vec<_>>();

    let sql = format!(
        "SELECT id FROM hopr_safe_owner_state
         WHERE hopr_safe_contract_id = {}
           AND owner_address = {}
           AND published_block = {}
           AND published_tx_index = {}
           AND published_log_index = {}",
        placeholders[0], placeholders[1], placeholders[2], placeholders[3], placeholders[4],
    );

    Statement::from_sql_and_values(
        backend,
        sql,
        vec![
            safe_contract_id.into(),
            owner_address.as_ref().to_vec().into(),
            block.cast_signed().into(),
            tx_index.cast_signed().into(),
            log_index.cast_signed().into(),
        ],
    )
}

fn safe_activity_id_statement(
    backend: DatabaseBackend,
    safe_contract_id: i64,
    block: u64,
    tx_index: u64,
    log_index: u64,
) -> Statement {
    let placeholders = (1..=4)
        .map(|position| sql_placeholder(backend, position))
        .collect::<Vec<_>>();

    let sql = format!(
        "SELECT id FROM hopr_safe_activity
         WHERE hopr_safe_contract_id = {}
           AND published_block = {}
           AND published_tx_index = {}
           AND published_log_index = {}",
        placeholders[0], placeholders[1], placeholders[2], placeholders[3],
    );

    Statement::from_sql_and_values(
        backend,
        sql,
        vec![
            safe_contract_id.into(),
            block.cast_signed().into(),
            tx_index.cast_signed().into(),
            log_index.cast_signed().into(),
        ],
    )
}

fn safe_current_owners_statement(backend: DatabaseBackend, safe_address: Address) -> Statement {
    let placeholder = sql_placeholder(backend, 1);
    let sql = format!(
        "SELECT owner_address
         FROM safe_owner_current
         WHERE safe_address = {}
         ORDER BY owner_address ASC",
        placeholder,
    );
    Statement::from_sql_and_values(backend, sql, vec![safe_address.as_ref().to_vec().into()])
}

fn safe_activity_history_statement(backend: DatabaseBackend, safe_address: Address) -> Statement {
    let placeholder = sql_placeholder(backend, 1);
    let sql = format!(
        "SELECT
            sc.address AS safe_address,
            sa.event_kind,
            sa.chain_tx_hash,
            sa.safe_tx_hash,
            sa.owner_address,
            sa.threshold,
            sa.payment,
            sa.initiator_address,
            sa.published_block,
            sa.published_tx_index,
            sa.published_log_index
         FROM hopr_safe_activity sa
         JOIN hopr_safe_contract sc ON sc.id = sa.hopr_safe_contract_id
         WHERE sc.address = {}
         ORDER BY sa.published_block ASC, sa.published_tx_index ASC, sa.published_log_index ASC",
        placeholder,
    );
    Statement::from_sql_and_values(backend, sql, vec![safe_address.as_ref().to_vec().into()])
}

fn try_get_i64(row: &QueryResult, column: &str) -> Result<i64> {
    row.try_get("", column)
        .map_err(|e| DbSqlError::Construction(format!("Failed to read {column}: {e}")))
}

fn try_get_bytes(row: &QueryResult, column: &str) -> Result<Vec<u8>> {
    row.try_get("", column)
        .map_err(|e| DbSqlError::Construction(format!("Failed to read {column}: {e}")))
}

fn try_get_optional_bytes(row: &QueryResult, column: &str) -> Result<Option<Vec<u8>>> {
    row.try_get("", column)
        .map_err(|e| DbSqlError::Construction(format!("Failed to read {column}: {e}")))
}

fn try_get_string(row: &QueryResult, column: &str) -> Result<String> {
    row.try_get("", column)
        .map_err(|e| DbSqlError::Construction(format!("Failed to read {column}: {e}")))
}

fn try_get_optional_string(row: &QueryResult, column: &str) -> Result<Option<String>> {
    row.try_get("", column)
        .map_err(|e| DbSqlError::Construction(format!("Failed to read {column}: {e}")))
}

#[async_trait]
pub trait BlokliDbSafeHistoryOperations: BlokliDbGeneralModelOperations + BlokliDbSafeContractOperations {
    #[allow(clippy::too_many_arguments)]
    async fn record_safe_activity<'a>(
        &'a self,
        tx: OptTx<'a>,
        safe_address: Address,
        event_kind: SafeActivityKind,
        chain_tx_hash: Hash,
        safe_tx_hash: Option<Hash>,
        owner_address: Option<Address>,
        threshold: Option<String>,
        payment: Option<String>,
        initiator_address: Option<Address>,
        block: u64,
        tx_index: u64,
        log_index: u64,
    ) -> Result<i64>;

    #[allow(clippy::too_many_arguments)]
    async fn upsert_safe_owner_state<'a>(
        &'a self,
        tx: OptTx<'a>,
        safe_address: Address,
        owner_address: Address,
        is_current_owner: bool,
        block: u64,
        tx_index: u64,
        log_index: u64,
    ) -> Result<i64>;

    async fn get_safe_owners<'a>(&'a self, tx: OptTx<'a>, safe_address: Address) -> Result<Vec<Address>>;

    async fn get_safe_activity<'a>(&'a self, tx: OptTx<'a>, safe_address: Address) -> Result<Vec<SafeActivityEntry>>;
}

#[async_trait]
impl BlokliDbSafeHistoryOperations for BlokliDb {
    #[allow(clippy::too_many_arguments)]
    async fn record_safe_activity<'a>(
        &'a self,
        tx: OptTx<'a>,
        safe_address: Address,
        event_kind: SafeActivityKind,
        chain_tx_hash: Hash,
        safe_tx_hash: Option<Hash>,
        owner_address: Option<Address>,
        threshold: Option<String>,
        payment: Option<String>,
        initiator_address: Option<Address>,
        block: u64,
        tx_index: u64,
        log_index: u64,
    ) -> Result<i64> {
        let tx = self.nest_transaction(tx).await?;
        let safe = self
            .get_safe_contract_by_address(Some(&tx), safe_address)
            .await?
            .ok_or_else(|| {
                DbSqlError::EntityNotFound(format!("safe contract not found for {}", safe_address.to_hex()))
            })?;
        let backend = tx.as_ref().get_database_backend();

        tx.as_ref()
            .execute_raw(safe_activity_insert_statement(
                backend,
                safe.id,
                event_kind,
                chain_tx_hash,
                safe_tx_hash,
                owner_address,
                threshold,
                payment,
                initiator_address,
                block,
                tx_index,
                log_index,
            ))
            .await?;

        let row = tx
            .as_ref()
            .query_one_raw(safe_activity_id_statement(backend, safe.id, block, tx_index, log_index))
            .await?
            .ok_or_else(|| {
                DbSqlError::EntityNotFound(format!(
                    "safe activity not found after insert for {} at {}:{}:{}",
                    safe_address.to_hex(),
                    block,
                    tx_index,
                    log_index
                ))
            })?;

        let id = try_get_i64(&row, "id")?;
        tx.commit().await?;
        Ok(id)
    }

    #[allow(clippy::too_many_arguments)]
    async fn upsert_safe_owner_state<'a>(
        &'a self,
        tx: OptTx<'a>,
        safe_address: Address,
        owner_address: Address,
        is_current_owner: bool,
        block: u64,
        tx_index: u64,
        log_index: u64,
    ) -> Result<i64> {
        let tx = self.nest_transaction(tx).await?;
        let safe = self
            .get_safe_contract_by_address(Some(&tx), safe_address)
            .await?
            .ok_or_else(|| {
                DbSqlError::EntityNotFound(format!("safe contract not found for {}", safe_address.to_hex()))
            })?;
        let backend = tx.as_ref().get_database_backend();

        tx.as_ref()
            .execute_raw(safe_owner_state_insert_statement(
                backend,
                safe.id,
                owner_address,
                is_current_owner,
                block,
                tx_index,
                log_index,
            ))
            .await?;

        let row = tx
            .as_ref()
            .query_one_raw(safe_owner_state_id_statement(
                backend,
                safe.id,
                owner_address,
                block,
                tx_index,
                log_index,
            ))
            .await?
            .ok_or_else(|| {
                DbSqlError::EntityNotFound(format!(
                    "safe owner state not found after insert for {} owner {} at {}:{}:{}",
                    safe_address.to_hex(),
                    owner_address.to_hex(),
                    block,
                    tx_index,
                    log_index
                ))
            })?;

        let id = try_get_i64(&row, "id")?;
        tx.commit().await?;
        Ok(id)
    }

    async fn get_safe_owners<'a>(&'a self, tx: OptTx<'a>, safe_address: Address) -> Result<Vec<Address>> {
        if tx.is_some() {
            let nested = self.nest_transaction(tx).await?;
            let backend = nested.as_ref().get_database_backend();
            let rows = nested
                .as_ref()
                .query_all_raw(safe_current_owners_statement(backend, safe_address))
                .await?;

            let owners = rows
                .into_iter()
                .map(|row| {
                    let owner_bytes = try_get_bytes(&row, "owner_address")?;
                    Address::try_from(owner_bytes.as_slice())
                        .map_err(|e| DbSqlError::Construction(format!("invalid safe owner address length: {e}")))
                })
                .collect::<Result<Vec<_>>>()?;

            nested.commit().await?;
            Ok(owners)
        } else {
            let backend = self.conn(TargetDb::Index).get_database_backend();
            let rows = self
                .conn(TargetDb::Index)
                .query_all_raw(safe_current_owners_statement(backend, safe_address))
                .await?;

            rows.into_iter()
                .map(|row| {
                    let owner_bytes = try_get_bytes(&row, "owner_address")?;
                    Address::try_from(owner_bytes.as_slice())
                        .map_err(|e| DbSqlError::Construction(format!("invalid safe owner address length: {e}")))
                })
                .collect::<Result<Vec<_>>>()
        }
    }

    async fn get_safe_activity<'a>(&'a self, tx: OptTx<'a>, safe_address: Address) -> Result<Vec<SafeActivityEntry>> {
        if tx.is_some() {
            let nested = self.nest_transaction(tx).await?;
            let backend = nested.as_ref().get_database_backend();
            let rows = nested
                .as_ref()
                .query_all_raw(safe_activity_history_statement(backend, safe_address))
                .await?;

            let entries = rows
                .into_iter()
                .map(|row| {
                    Ok(SafeActivityEntry {
                        safe_address: try_get_bytes(&row, "safe_address")?,
                        event_kind: try_get_string(&row, "event_kind")?,
                        chain_tx_hash: try_get_bytes(&row, "chain_tx_hash")?,
                        safe_tx_hash: try_get_optional_bytes(&row, "safe_tx_hash")?,
                        owner_address: try_get_optional_bytes(&row, "owner_address")?,
                        threshold: try_get_optional_string(&row, "threshold")?,
                        payment: try_get_optional_string(&row, "payment")?,
                        initiator_address: try_get_optional_bytes(&row, "initiator_address")?,
                        published_block: try_get_i64(&row, "published_block")?,
                        published_tx_index: try_get_i64(&row, "published_tx_index")?,
                        published_log_index: try_get_i64(&row, "published_log_index")?,
                    })
                })
                .collect::<Result<Vec<_>>>()?;

            nested.commit().await?;
            Ok(entries)
        } else {
            let backend = self.conn(TargetDb::Index).get_database_backend();
            let rows = self
                .conn(TargetDb::Index)
                .query_all_raw(safe_activity_history_statement(backend, safe_address))
                .await?;

            rows.into_iter()
                .map(|row| {
                    Ok(SafeActivityEntry {
                        safe_address: try_get_bytes(&row, "safe_address")?,
                        event_kind: try_get_string(&row, "event_kind")?,
                        chain_tx_hash: try_get_bytes(&row, "chain_tx_hash")?,
                        safe_tx_hash: try_get_optional_bytes(&row, "safe_tx_hash")?,
                        owner_address: try_get_optional_bytes(&row, "owner_address")?,
                        threshold: try_get_optional_string(&row, "threshold")?,
                        payment: try_get_optional_string(&row, "payment")?,
                        initiator_address: try_get_optional_bytes(&row, "initiator_address")?,
                        published_block: try_get_i64(&row, "published_block")?,
                        published_tx_index: try_get_i64(&row, "published_tx_index")?,
                        published_log_index: try_get_i64(&row, "published_log_index")?,
                    })
                })
                .collect::<Result<Vec<_>>>()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{db::BlokliDb, safe_contracts::BlokliDbSafeContractOperations};

    fn random_address() -> Address {
        Address::from(hopr_types::crypto_random::random_bytes())
    }

    fn random_hash() -> Hash {
        Hash::from(hopr_types::crypto_random::random_bytes())
    }

    #[tokio::test]
    async fn test_get_safe_owners_tracks_current_membership() -> anyhow::Result<()> {
        let db = BlokliDb::new_in_memory().await?;

        let safe_address = random_address();
        let module_address = random_address();
        let chain_key = random_address();
        let owner_one: Address = "1111111111111111111111111111111111111111".parse()?;
        let owner_two: Address = "2222222222222222222222222222222222222222".parse()?;

        db.create_safe_contract(None, safe_address, module_address, chain_key, 100, 0, 0)
            .await?;

        let first_id = db
            .upsert_safe_owner_state(None, safe_address, owner_one, true, 101, 0, 0)
            .await?;
        let duplicate_id = db
            .upsert_safe_owner_state(None, safe_address, owner_one, true, 101, 0, 0)
            .await?;
        assert_eq!(
            first_id, duplicate_id,
            "duplicate owner state insert should be idempotent"
        );

        db.upsert_safe_owner_state(None, safe_address, owner_two, true, 102, 0, 0)
            .await?;
        db.upsert_safe_owner_state(None, safe_address, owner_one, false, 103, 0, 0)
            .await?;

        let current_owners = db.get_safe_owners(None, safe_address).await?;
        assert_eq!(current_owners, vec![owner_two], "only owner two should still be active");

        db.upsert_safe_owner_state(None, safe_address, owner_one, true, 104, 0, 0)
            .await?;

        let current_owners = db.get_safe_owners(None, safe_address).await?;
        assert_eq!(
            current_owners,
            vec![owner_one, owner_two],
            "re-adding owner one should restore both current owners in sorted order"
        );

        Ok(())
    }

    #[tokio::test]
    async fn test_get_safe_activity_is_ordered_and_idempotent() -> anyhow::Result<()> {
        let db = BlokliDb::new_in_memory().await?;

        let safe_address = random_address();
        let module_address = random_address();
        let chain_key = random_address();
        let owner = random_address();
        let chain_tx_hash = random_hash();
        let safe_tx_hash = random_hash();

        db.create_safe_contract(None, safe_address, module_address, chain_key, 100, 0, 0)
            .await?;

        let first_id = db
            .record_safe_activity(
                None,
                safe_address,
                SafeActivityKind::AddedOwner,
                chain_tx_hash,
                None,
                Some(owner),
                None,
                None,
                None,
                110,
                2,
                0,
            )
            .await?;
        let duplicate_id = db
            .record_safe_activity(
                None,
                safe_address,
                SafeActivityKind::AddedOwner,
                chain_tx_hash,
                None,
                Some(owner),
                None,
                None,
                None,
                110,
                2,
                0,
            )
            .await?;
        assert_eq!(first_id, duplicate_id, "duplicate activity insert should be idempotent");

        db.record_safe_activity(
            None,
            safe_address,
            SafeActivityKind::ExecutionSuccess,
            random_hash(),
            Some(safe_tx_hash),
            None,
            None,
            Some("12".to_string()),
            None,
            111,
            0,
            1,
        )
        .await?;

        let activity = db.get_safe_activity(None, safe_address).await?;
        assert_eq!(
            activity.len(),
            2,
            "duplicate replay should not create duplicate activity rows"
        );
        assert_eq!(activity[0].event_kind, SafeActivityKind::AddedOwner.as_str());
        assert_eq!(activity[0].owner_address, Some(owner.as_ref().to_vec()));
        assert_eq!(activity[0].published_block, 110);
        assert_eq!(activity[1].event_kind, SafeActivityKind::ExecutionSuccess.as_str());
        assert_eq!(activity[1].safe_tx_hash, Some(safe_tx_hash.as_ref().to_vec()));
        assert_eq!(activity[1].payment.as_deref(), Some("12"));
        assert_eq!(activity[1].published_block, 111);

        Ok(())
    }
}
