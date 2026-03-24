use std::collections::VecDeque;

use sea_orm::{ConnectionTrait, TransactionTrait};
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

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let csv_data = match self.0 {
            SafeDataOrigin::NoData => return Ok(()),
            SafeDataOrigin::Jura => VecDeque::from(include_bytes!("data/safe-v3-jura.csv").to_vec()),
        };

        let backend = manager.get_database_backend();

        manager
            .get_connection()
            .transaction(|tx| {
                Box::pin(async move {
                    let reader = csv::ReaderBuilder::default()
                        .has_headers(true)
                        .trim(csv::Trim::All)
                        .from_reader(csv_data);

                    for result in reader.into_deserialize() {
                        let entry: SafeCsvEntry =
                            result.map_err(|e| DbErr::Custom(format!("failed to deserialize CSV entry: {e}")))?;

                        let addr_hex: String = entry.address.iter().map(|b| format!("{b:02x}")).collect();
                        let mod_addr_hex: String = entry.module_address.iter().map(|b| format!("{b:02x}")).collect();
                        let chain_key_hex: String = entry.chain_key.iter().map(|b| format!("{b:02x}")).collect();
                        let block = MIGRATION_MARKER_BLOCK_ID;
                        let tx_idx = entry.deployed_tx_index;
                        let log_idx = entry.deployed_log_index;

                        // Insert the safe address into hopr_safe_contract (ignore conflicts)
                        let insert_safe = match backend {
                            sea_orm::DatabaseBackend::Postgres => format!(
                                "INSERT INTO hopr_safe_contract (address) VALUES (decode('{addr_hex}', 'hex')) ON \
                                 CONFLICT DO NOTHING"
                            ),
                            _ => format!("INSERT OR IGNORE INTO hopr_safe_contract (address) VALUES (X'{addr_hex}')"),
                        };
                        tx.execute_unprepared(&insert_safe).await?;

                        // Insert the state into hopr_safe_contract_state via SELECT to get the id
                        let insert_state = match backend {
                            sea_orm::DatabaseBackend::Postgres => format!(
                                "INSERT INTO hopr_safe_contract_state (hopr_safe_contract_id, module_address, \
                                 chain_key, published_block, published_tx_index, published_log_index) SELECT id, \
                                 decode('{mod_addr_hex}', 'hex'), decode('{chain_key_hex}', 'hex'), {block}, \
                                 {tx_idx}, {log_idx} FROM hopr_safe_contract WHERE address = decode('{addr_hex}', \
                                 'hex') ON CONFLICT DO NOTHING"
                            ),
                            _ => format!(
                                "INSERT OR IGNORE INTO hopr_safe_contract_state (hopr_safe_contract_id, \
                                 module_address, chain_key, published_block, published_tx_index, published_log_index) \
                                 SELECT id, X'{mod_addr_hex}', X'{chain_key_hex}', {block}, {tx_idx}, {log_idx} FROM \
                                 hopr_safe_contract WHERE address = X'{addr_hex}'"
                            ),
                        };
                        tx.execute_unprepared(&insert_state).await?;
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
            SafeDataOrigin::Jura => VecDeque::from(include_bytes!("data/safe-v3-jura.csv").to_vec()),
        };

        let backend = manager.get_database_backend();

        manager
            .get_connection()
            .transaction(|tx| {
                Box::pin(async move {
                    let reader = csv::ReaderBuilder::default()
                        .has_headers(true)
                        .trim(csv::Trim::All)
                        .from_reader(csv_data);

                    for result in reader.into_deserialize() {
                        let entry: SafeCsvEntry =
                            result.map_err(|e| DbErr::Custom(format!("failed to deserialize CSV entry: {e}")))?;

                        let addr_hex: String = entry.address.iter().map(|b| format!("{b:02x}")).collect();

                        // Deleting from hopr_safe_contract cascades to hopr_safe_contract_state
                        let delete_safe = match backend {
                            sea_orm::DatabaseBackend::Postgres => {
                                format!("DELETE FROM hopr_safe_contract WHERE address = decode('{addr_hex}', 'hex')")
                            }
                            _ => format!("DELETE FROM hopr_safe_contract WHERE address = X'{addr_hex}'"),
                        };
                        tx.execute_unprepared(&delete_safe).await?;
                    }

                    Ok::<_, DbErr>(())
                })
            })
            .await
            .map_err(|e| DbErr::Custom(format!("failed to commit transaction: {e}")))?;

        Ok(())
    }
}
