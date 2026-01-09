use std::collections::VecDeque;

use sea_orm::{ConnectionTrait, TransactionTrait};
use sea_orm_migration::prelude::*;
use serde_hex::{SerHex, StrictPfx};

use crate::SafeDataOrigin;

#[derive(Debug, serde::Deserialize)]
struct SafeCsvEntry {
    #[serde(with = "SerHex::<StrictPfx>")]
    address: [u8; 20],
    #[serde(with = "SerHex::<StrictPfx>")]
    module_address: [u8; 20],
    #[serde(with = "SerHex::<StrictPfx>")]
    chain_key: [u8; 20],
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
            SafeDataOrigin::Rotsee => VecDeque::from(include_bytes!("data/safe-v3-rotsee.csv").to_vec()),
            SafeDataOrigin::Dufour => VecDeque::from(include_bytes!("data/safe-v3-dufour.csv").to_vec()),
        };

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

                        let insert_query = Query::insert()
                            .into_table(HoprSafeContract::Table)
                            .columns([
                                HoprSafeContract::Address,
                                HoprSafeContract::ModuleAddress,
                                HoprSafeContract::ChainKey,
                                HoprSafeContract::DeployedBlock,
                                HoprSafeContract::DeployedTxIndex,
                                HoprSafeContract::DeployedLogIndex,
                            ])
                            .values_panic([
                                entry.address.to_vec().into(),
                                entry.module_address.to_vec().into(),
                                entry.chain_key.to_vec().into(),
                                entry.deployed_block.into(),
                                entry.deployed_tx_index.into(),
                                entry.deployed_log_index.into(),
                            ])
                            .on_conflict(OnConflict::new().do_nothing().to_owned())
                            .to_owned();

                        tx.execute(&insert_query).await?;
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

                        let delete_query = Query::delete()
                            .from_table(HoprSafeContract::Table)
                            .cond_where(
                                Expr::col(HoprSafeContract::Address)
                                    .eq(entry.address.to_vec())
                                    .and(Expr::col(HoprSafeContract::ModuleAddress).eq(entry.module_address.to_vec())),
                            )
                            .to_owned();

                        tx.execute(&delete_query).await?;
                    }
                    Ok::<_, DbErr>(())
                })
            })
            .await
            .map_err(|e| DbErr::Custom(format!("failed to commit transaction: {e}")))?;

        Ok(())
    }
}

#[derive(DeriveIden)]
enum HoprSafeContract {
    Table,
    Address,
    ModuleAddress,
    ChainKey,
    DeployedBlock,
    DeployedTxIndex,
    DeployedLogIndex,
}
