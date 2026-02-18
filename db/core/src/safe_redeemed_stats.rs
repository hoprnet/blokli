use async_trait::async_trait;
use blokli_db_entity::{hopr_safe_redeemed_stats, prelude::HoprSafeRedeemedStats};
use hopr_primitive_types::{
    prelude::{Address, HoprBalance},
    traits::IntoEndian,
};
use sea_orm::{ActiveModelTrait, ColumnTrait, EntityTrait, QueryFilter, Set};

use crate::{
    BlokliDbGeneralModelOperations, OptTx,
    db::BlokliDb,
    errors::{DbSqlError, Result},
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SafeRedeemedStatsEntry {
    pub safe_address: Address,
    pub redeemed_amount: HoprBalance,
    pub redemption_count: u64,
    pub last_redeemed_block: u64,
    pub last_redeemed_tx_index: u64,
    pub last_redeemed_log_index: u64,
}

fn model_to_entry(model: hopr_safe_redeemed_stats::Model) -> Result<SafeRedeemedStatsEntry> {
    let safe_address: Address = Address::try_from(model.safe_address.as_slice())?;
    let redeemed_amount: HoprBalance = HoprBalance::from_be_bytes(model.redeemed_amount.as_slice());

    Ok(SafeRedeemedStatsEntry {
        safe_address,
        redeemed_amount,
        redemption_count: u64::try_from(model.redemption_count).map_err(|_| DbSqlError::DecodingError)?,
        last_redeemed_block: u64::try_from(model.last_redeemed_block).map_err(|_| DbSqlError::DecodingError)?,
        last_redeemed_tx_index: u64::try_from(model.last_redeemed_tx_index).map_err(|_| DbSqlError::DecodingError)?,
        last_redeemed_log_index: u64::try_from(model.last_redeemed_log_index).map_err(|_| DbSqlError::DecodingError)?,
    })
}

#[async_trait]
pub trait BlokliDbSafeRedeemedStatsOperations {
    #[allow(clippy::too_many_arguments)]
    async fn record_safe_ticket_redeemed<'a>(
        &'a self,
        tx: OptTx<'a>,
        safe_address: Address,
        redeemed_amount: HoprBalance,
        block: u32,
        tx_index: u32,
        log_index: u32,
    ) -> Result<SafeRedeemedStatsEntry>;

    async fn get_safe_redeemed_stats<'a>(
        &'a self,
        tx: OptTx<'a>,
        safe_address: Address,
    ) -> Result<Option<SafeRedeemedStatsEntry>>;
}

#[async_trait]
impl BlokliDbSafeRedeemedStatsOperations for BlokliDb {
    async fn record_safe_ticket_redeemed<'a>(
        &'a self,
        tx: OptTx<'a>,
        safe_address: Address,
        redeemed_amount: HoprBalance,
        block: u32,
        tx_index: u32,
        log_index: u32,
    ) -> Result<SafeRedeemedStatsEntry> {
        self.nest_transaction(tx)
            .await?
            .perform(|tx| {
                Box::pin(async move {
                    let safe_address_bytes = safe_address.as_ref().to_vec();

                    let existing = HoprSafeRedeemedStats::find()
                        .filter(hopr_safe_redeemed_stats::Column::SafeAddress.eq(safe_address_bytes.clone()))
                        .one(tx.as_ref())
                        .await?;

                    let stored = match existing {
                        Some(model) => {
                            let current_amount = HoprBalance::from_be_bytes(model.redeemed_amount.as_slice());
                            let next_amount = current_amount + redeemed_amount;
                            let next_count = model.redemption_count.checked_add(1).ok_or_else(|| {
                                DbSqlError::LogicalError("safe redemption count overflow".to_string())
                            })?;

                            let mut active: hopr_safe_redeemed_stats::ActiveModel = model.into();
                            active.redeemed_amount = Set(next_amount.to_be_bytes().to_vec());
                            active.redemption_count = Set(next_count);
                            active.last_redeemed_block = Set(i64::from(block));
                            active.last_redeemed_tx_index = Set(i64::from(tx_index));
                            active.last_redeemed_log_index = Set(i64::from(log_index));
                            active.update(tx.as_ref()).await?
                        }
                        None => {
                            let active = hopr_safe_redeemed_stats::ActiveModel {
                                safe_address: Set(safe_address_bytes),
                                redeemed_amount: Set(redeemed_amount.to_be_bytes().to_vec()),
                                redemption_count: Set(1),
                                last_redeemed_block: Set(i64::from(block)),
                                last_redeemed_tx_index: Set(i64::from(tx_index)),
                                last_redeemed_log_index: Set(i64::from(log_index)),
                                ..Default::default()
                            };
                            active.insert(tx.as_ref()).await?
                        }
                    };

                    model_to_entry(stored)
                })
            })
            .await
    }

    async fn get_safe_redeemed_stats<'a>(
        &'a self,
        tx: OptTx<'a>,
        safe_address: Address,
    ) -> Result<Option<SafeRedeemedStatsEntry>> {
        self.nest_transaction(tx)
            .await?
            .perform(|tx| {
                Box::pin(async move {
                    let model = HoprSafeRedeemedStats::find()
                        .filter(hopr_safe_redeemed_stats::Column::SafeAddress.eq(safe_address.as_ref().to_vec()))
                        .one(tx.as_ref())
                        .await?;

                    model.map(model_to_entry).transpose()
                })
            })
            .await
    }
}

#[cfg(test)]
mod tests {
    use rand::RngCore;

    use super::*;

    fn random_address() -> Address {
        let mut rng = rand::rng();
        let mut bytes = [0u8; 20];
        rng.fill_bytes(&mut bytes);
        Address::from(bytes)
    }

    #[tokio::test]
    async fn test_record_and_get_safe_redeemed_stats() -> anyhow::Result<()> {
        let db = BlokliDb::new_in_memory().await?;
        let safe_address = random_address();

        let first = db
            .record_safe_ticket_redeemed(None, safe_address, HoprBalance::from(10_u64), 100, 1, 1)
            .await?;
        assert_eq!(first.redeemed_amount, HoprBalance::from(10_u64));
        assert_eq!(first.redemption_count, 1);

        let second = db
            .record_safe_ticket_redeemed(None, safe_address, HoprBalance::from(5_u64), 110, 2, 3)
            .await?;
        assert_eq!(second.redeemed_amount, HoprBalance::from(15_u64));
        assert_eq!(second.redemption_count, 2);
        assert_eq!(second.last_redeemed_block, 110);
        assert_eq!(second.last_redeemed_tx_index, 2);
        assert_eq!(second.last_redeemed_log_index, 3);

        let loaded = db
            .get_safe_redeemed_stats(None, safe_address)
            .await?
            .expect("stats should exist");
        assert_eq!(loaded, second);

        Ok(())
    }

    #[tokio::test]
    async fn test_get_safe_redeemed_stats_missing() -> anyhow::Result<()> {
        let db = BlokliDb::new_in_memory().await?;
        let safe_address = random_address();

        let loaded = db.get_safe_redeemed_stats(None, safe_address).await?;
        assert!(loaded.is_none());

        Ok(())
    }
}
