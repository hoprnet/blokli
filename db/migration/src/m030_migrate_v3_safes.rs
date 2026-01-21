use std::collections::VecDeque;

use sea_orm::{ConnectionTrait, Statement, TransactionTrait};
use sea_orm_migration::prelude::*;
use serde_hex::{SerHex, StrictPfx};

use crate::{MIGRATION_MARKER_BLOCK_ID, SafeDataOrigin};

#[derive(Debug, serde::Deserialize)]
struct SafeCsvEntry {
    #[serde(with = "SerHex::<StrictPfx>")]
    address: [u8; 20],
    #[serde(with = "SerHex::<StrictPfx>")]
    module_address: [u8; 20],
    #[serde(with = "SerHex::<StrictPfx>")]
    chain_key: [u8; 20],
    #[allow(unused)] // Replaced with MIGRATION_MARKER_BLOCK_ID
    deployed_block: u32,
    deployed_tx_index: u32,
    deployed_log_index: u32,
}

#[derive(DeriveMigrationName)]
pub struct Migration(pub(crate) SafeDataOrigin);

fn bytes_to_hex_literal(bytes: &[u8]) -> String {
    // Convert bytes to SQL hex literal format X'...'
    let hex_str: String = bytes.iter().map(|b| format!("{:02x}", b)).collect();
    format!("X'{}'", hex_str)
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let csv_data = match self.0 {
            SafeDataOrigin::NoData => return Ok(()),
            SafeDataOrigin::Rotsee => VecDeque::from(include_bytes!("data/safe-v3-rotsee.csv").to_vec()),
            SafeDataOrigin::Dufour => VecDeque::from(include_bytes!("data/safe-v3-dufour.csv").to_vec()),
        };

        let db = manager.get_connection();
        let backend = db.get_database_backend();

        db.transaction(|tx| {
            Box::pin(async move {
                let reader = csv::ReaderBuilder::default()
                    .has_headers(true)
                    .trim(csv::Trim::All)
                    .from_reader(csv_data);

                for result in reader.into_deserialize() {
                    let entry: SafeCsvEntry =
                        result.map_err(|e| DbErr::Custom(format!("failed to deserialize CSV entry: {e}")))?;

                    let address_hex = bytes_to_hex_literal(&entry.address);

                    // Insert identity, ignore if already exists (ON CONFLICT DO NOTHING)
                    let insert_identity_sql = format!(
                        "INSERT INTO hopr_safe_contract (address) VALUES ({}) ON CONFLICT DO NOTHING",
                        address_hex
                    );
                    tx.execute_unprepared(&insert_identity_sql).await?;

                    // Get the identity ID (either just inserted or existing)
                    let select_id_sql = format!(
                        "SELECT id FROM hopr_safe_contract WHERE address = {}",
                        address_hex
                    );
                    let row = tx
                        .query_one_raw(Statement::from_string(backend, select_id_sql))
                        .await?
                        .ok_or_else(|| DbErr::Custom("Identity should exist after insert".to_string()))?;
                    let identity_id: i64 = row.try_get("", "id")?;

                    // Insert state entry with MIGRATION_MARKER_BLOCK_ID
                    let module_hex = bytes_to_hex_literal(&entry.module_address);
                    let chain_key_hex = bytes_to_hex_literal(&entry.chain_key);

                    let insert_state_sql = format!(
                        "INSERT INTO hopr_safe_contract_state (hopr_safe_contract_id, module_address, chain_key, published_block, published_tx_index, published_log_index) \
                         VALUES ({}, {}, {}, {}, {}, {}) ON CONFLICT DO NOTHING",
                        identity_id,
                        module_hex,
                        chain_key_hex,
                        MIGRATION_MARKER_BLOCK_ID,
                        entry.deployed_tx_index,
                        entry.deployed_log_index
                    );
                    tx.execute_unprepared(&insert_state_sql).await?;
                }
                Ok::<_, DbErr>(())
            })
        })
        .await
        .map_err(|e| DbErr::Custom(format!("failed to commit transaction: {e}")))?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let csv_data = match self.0 {
            SafeDataOrigin::NoData => return Ok(()),
            SafeDataOrigin::Rotsee => VecDeque::from(include_bytes!("data/safe-v3-rotsee.csv").to_vec()),
            SafeDataOrigin::Dufour => VecDeque::from(include_bytes!("data/safe-v3-dufour.csv").to_vec()),
        };

        let db = manager.get_connection();
        let backend = db.get_database_backend();

        db.transaction(|tx| {
            Box::pin(async move {
                let reader = csv::ReaderBuilder::default()
                    .has_headers(true)
                    .trim(csv::Trim::All)
                    .from_reader(csv_data);

                for result in reader.into_deserialize() {
                    let entry: SafeCsvEntry =
                        result.map_err(|e| DbErr::Custom(format!("failed to deserialize CSV entry: {e}")))?;

                    let address_hex = bytes_to_hex_literal(&entry.address);

                    // Get identity ID
                    let select_id_sql = format!(
                        "SELECT id FROM hopr_safe_contract WHERE address = {}",
                        address_hex
                    );

                    if let Some(row) = tx.query_one_raw(Statement::from_string(backend, select_id_sql)).await? {
                        let identity_id: i64 = row.try_get("", "id")?;

                        // Delete state entries for this identity at MIGRATION_MARKER_BLOCK_ID
                        let delete_state_sql = format!(
                            "DELETE FROM hopr_safe_contract_state WHERE hopr_safe_contract_id = {} AND published_block = {}",
                            identity_id, MIGRATION_MARKER_BLOCK_ID
                        );
                        tx.execute_unprepared(&delete_state_sql).await?;

                        // Check if any state entries remain for this identity
                        let count_sql = format!(
                            "SELECT COUNT(*) as cnt FROM hopr_safe_contract_state WHERE hopr_safe_contract_id = {}",
                            identity_id
                        );
                        let count_row = tx
                            .query_one_raw(Statement::from_string(backend, count_sql))
                            .await?
                            .ok_or_else(|| DbErr::Custom("Count query should return a row".to_string()))?;
                        let remaining_states: i64 = count_row.try_get("", "cnt")?;

                        // If no states remain, delete the identity too
                        if remaining_states == 0 {
                            let delete_identity_sql = format!(
                                "DELETE FROM hopr_safe_contract WHERE id = {}",
                                identity_id
                            );
                            tx.execute_unprepared(&delete_identity_sql).await?;
                        }
                    }
                }
                Ok::<_, DbErr>(())
            })
        })
        .await
        .map_err(|e| DbErr::Custom(format!("failed to commit transaction: {e}")))?;

        Ok(())
    }
}
