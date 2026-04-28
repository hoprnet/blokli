use async_trait::async_trait;
pub use blokli_db_entity::conversions::redemptions_aggregation::AggregatedRedeemedStats;
use blokli_db_entity::{
    conversions::redemptions_aggregation::fetch_aggregated_redeemed_stats, hopr_safe_redeemed_stats,
    prelude::HoprSafeRedeemedStats,
};
use hopr_types::primitive::{
    prelude::{Address, HoprBalance},
    traits::IntoEndian,
};
use sea_orm::{ActiveModelTrait, ColumnTrait, EntityTrait, QueryFilter, Set};

use crate::{
    BlokliDbGeneralModelOperations, OptTx, TargetDb,
    db::BlokliDb,
    errors::{DbSqlError, Result},
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SafeRedeemedStatsEntry {
    pub safe_address: Address,
    pub node_address: Address,
    pub redeemed_amount: HoprBalance,
    pub redemption_count: u64,
    pub rejected_amount: HoprBalance,
    pub rejection_count: u64,
    pub last_redeemed_block: u64,
    pub last_redeemed_tx_index: u64,
    pub last_redeemed_log_index: u64,
}

fn model_to_entry(model: hopr_safe_redeemed_stats::Model) -> Result<SafeRedeemedStatsEntry> {
    let safe_address: Address = Address::try_from(model.safe_address.as_slice())?;
    let node_address: Address = Address::try_from(model.node_address.as_slice())?;
    let redeemed_amount: HoprBalance = HoprBalance::from_be_bytes(model.redeemed_amount.as_slice());
    let rejected_amount: HoprBalance = HoprBalance::from_be_bytes(model.rejected_amount.as_slice());

    Ok(SafeRedeemedStatsEntry {
        safe_address,
        node_address,
        redeemed_amount,
        redemption_count: u64::try_from(model.redemption_count).map_err(|_| DbSqlError::DecodingError)?,
        rejected_amount,
        rejection_count: u64::try_from(model.rejection_count).map_err(|_| DbSqlError::DecodingError)?,
        last_redeemed_block: u64::try_from(model.last_redeemed_block).map_err(|_| DbSqlError::DecodingError)?,
        last_redeemed_tx_index: u64::try_from(model.last_redeemed_tx_index).map_err(|_| DbSqlError::DecodingError)?,
        last_redeemed_log_index: u64::try_from(model.last_redeemed_log_index).map_err(|_| DbSqlError::DecodingError)?,
    })
}

#[derive(Clone, Copy)]
enum TicketStatKind {
    Redeemed,
    Rejected,
}

#[async_trait]
pub trait BlokliDbSafeRedeemedStatsOperations {
    #[allow(clippy::too_many_arguments)]
    async fn record_safe_ticket_redeemed<'a>(
        &'a self,
        tx: OptTx<'a>,
        safe_address: Address,
        destination_node_address: Address,
        redeemed_amount: HoprBalance,
        block: u32,
        tx_index: u32,
        log_index: u32,
    ) -> Result<SafeRedeemedStatsEntry>;

    #[allow(clippy::too_many_arguments)]
    async fn record_safe_ticket_rejected<'a>(
        &'a self,
        tx: OptTx<'a>,
        safe_address: Address,
        destination_node_address: Address,
        rejected_amount: HoprBalance,
        block: u32,
        tx_index: u32,
        log_index: u32,
    ) -> Result<SafeRedeemedStatsEntry>;

    /// Returns aggregated redeemed ticket statistics matching the provided optional filters.
    ///
    /// When both filters are `None`, aggregates across all rows.
    /// When only `safe_address` is provided, aggregates all rows for that safe.
    /// When only `node_address` is provided, aggregates all rows for that node.
    /// When both are provided, returns the single matching safe/node pair row.
    async fn get_aggregated_redeemed_stats(
        &self,
        safe_address: Option<Address>,
        node_address: Option<Address>,
    ) -> Result<AggregatedRedeemedStats>;
}

#[async_trait]
impl BlokliDbSafeRedeemedStatsOperations for BlokliDb {
    async fn record_safe_ticket_redeemed<'a>(
        &'a self,
        tx: OptTx<'a>,
        safe_address: Address,
        node_address: Address,
        redeemed_amount: HoprBalance,
        block: u32,
        tx_index: u32,
        log_index: u32,
    ) -> Result<SafeRedeemedStatsEntry> {
        record_safe_ticket_stats(
            self,
            tx,
            safe_address,
            node_address,
            redeemed_amount,
            block,
            tx_index,
            log_index,
            TicketStatKind::Redeemed,
        )
        .await
    }

    async fn record_safe_ticket_rejected<'a>(
        &'a self,
        tx: OptTx<'a>,
        safe_address: Address,
        node_address: Address,
        rejected_amount: HoprBalance,
        block: u32,
        tx_index: u32,
        log_index: u32,
    ) -> Result<SafeRedeemedStatsEntry> {
        record_safe_ticket_stats(
            self,
            tx,
            safe_address,
            node_address,
            rejected_amount,
            block,
            tx_index,
            log_index,
            TicketStatKind::Rejected,
        )
        .await
    }

    async fn get_aggregated_redeemed_stats(
        &self,
        safe_address: Option<Address>,
        node_address: Option<Address>,
    ) -> Result<AggregatedRedeemedStats> {
        Ok(fetch_aggregated_redeemed_stats(self.conn(TargetDb::Index), safe_address, node_address).await?)
    }
}

#[allow(clippy::too_many_arguments)]
async fn record_safe_ticket_stats(
    db: &BlokliDb,
    tx: OptTx<'_>,
    safe_address: Address,
    node_address: Address,
    amount: HoprBalance,
    block: u32,
    tx_index: u32,
    log_index: u32,
    stat_kind: TicketStatKind,
) -> Result<SafeRedeemedStatsEntry> {
    db.nest_transaction(tx)
        .await?
        .perform(|tx| {
            Box::pin(async move {
                let safe_address_bytes = safe_address.as_ref().to_vec();
                let node_address_bytes = node_address.as_ref().to_vec();

                let existing = HoprSafeRedeemedStats::find()
                    .filter(hopr_safe_redeemed_stats::Column::SafeAddress.eq(safe_address_bytes.clone()))
                    .filter(hopr_safe_redeemed_stats::Column::NodeAddress.eq(node_address_bytes.clone()))
                    .one(tx.as_ref())
                    .await?;

                let stored = match existing {
                    Some(model) => {
                        let current_redeemed_amount = HoprBalance::from_be_bytes(model.redeemed_amount.as_slice());
                        let current_rejected_amount = HoprBalance::from_be_bytes(model.rejected_amount.as_slice());
                        let next_redeemed_amount = match stat_kind {
                            TicketStatKind::Redeemed => current_redeemed_amount + amount,
                            TicketStatKind::Rejected => current_redeemed_amount,
                        };
                        let next_rejected_amount = match stat_kind {
                            TicketStatKind::Redeemed => current_rejected_amount,
                            TicketStatKind::Rejected => current_rejected_amount + amount,
                        };
                        let next_redemption_count = match stat_kind {
                            TicketStatKind::Redeemed => model.redemption_count.checked_add(1).ok_or_else(|| {
                                DbSqlError::LogicalError("safe redemption count overflow".to_string())
                            })?,
                            TicketStatKind::Rejected => model.redemption_count,
                        };
                        let next_rejection_count = match stat_kind {
                            TicketStatKind::Redeemed => model.rejection_count,
                            TicketStatKind::Rejected => model
                                .rejection_count
                                .checked_add(1)
                                .ok_or_else(|| DbSqlError::LogicalError("safe rejection count overflow".to_string()))?,
                        };

                        let mut active: hopr_safe_redeemed_stats::ActiveModel = model.into();
                        active.redeemed_amount = Set(next_redeemed_amount.to_be_bytes().to_vec());
                        active.redemption_count = Set(next_redemption_count);
                        active.rejected_amount = Set(next_rejected_amount.to_be_bytes().to_vec());
                        active.rejection_count = Set(next_rejection_count);
                        active.last_redeemed_block = Set(i64::from(block));
                        active.last_redeemed_tx_index = Set(i64::from(tx_index));
                        active.last_redeemed_log_index = Set(i64::from(log_index));
                        active.update(tx.as_ref()).await?
                    }
                    None => {
                        let (redeemed_amount, redemption_count, rejected_amount, rejection_count) = match stat_kind {
                            TicketStatKind::Redeemed => (amount, 1, HoprBalance::zero(), 0),
                            TicketStatKind::Rejected => (HoprBalance::zero(), 0, amount, 1),
                        };

                        let active = hopr_safe_redeemed_stats::ActiveModel {
                            safe_address: Set(safe_address_bytes),
                            node_address: Set(node_address_bytes),
                            redeemed_amount: Set(redeemed_amount.to_be_bytes().to_vec()),
                            redemption_count: Set(redemption_count),
                            rejected_amount: Set(rejected_amount.to_be_bytes().to_vec()),
                            rejection_count: Set(rejection_count),
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

#[cfg(test)]
mod tests {
    use rand::Rng;
    use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};

    use super::*;
    use crate::TargetDb;

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
        let node_address = random_address();

        let first = db
            .record_safe_ticket_redeemed(None, safe_address, node_address, HoprBalance::from(10_u64), 100, 1, 1)
            .await?;
        assert_eq!(first.redeemed_amount, HoprBalance::from(10_u64));
        assert_eq!(first.redemption_count, 1);
        assert_eq!(first.rejected_amount, HoprBalance::zero());
        assert_eq!(first.rejection_count, 0);
        assert_eq!(first.node_address, node_address);

        let second = db
            .record_safe_ticket_redeemed(None, safe_address, node_address, HoprBalance::from(5_u64), 110, 2, 3)
            .await?;
        assert_eq!(second.redeemed_amount, HoprBalance::from(15_u64));
        assert_eq!(second.redemption_count, 2);
        assert_eq!(second.rejected_amount, HoprBalance::zero());
        assert_eq!(second.rejection_count, 0);
        assert_eq!(second.node_address, node_address);
        assert_eq!(second.last_redeemed_block, 110);
        assert_eq!(second.last_redeemed_tx_index, 2);
        assert_eq!(second.last_redeemed_log_index, 3);

        let loaded = HoprSafeRedeemedStats::find()
            .filter(hopr_safe_redeemed_stats::Column::SafeAddress.eq(safe_address.as_ref().to_vec()))
            .filter(hopr_safe_redeemed_stats::Column::NodeAddress.eq(node_address.as_ref().to_vec()))
            .one(db.conn(TargetDb::Index))
            .await?
            .map(model_to_entry)
            .transpose()?
            .expect("stats should exist");
        assert_eq!(loaded, second);

        Ok(())
    }

    #[tokio::test]
    async fn test_get_safe_redeemed_stats_missing() -> anyhow::Result<()> {
        let db = BlokliDb::new_in_memory().await?;
        let safe_address = random_address();
        let node_address = random_address();

        let loaded = HoprSafeRedeemedStats::find()
            .filter(hopr_safe_redeemed_stats::Column::SafeAddress.eq(safe_address.as_ref().to_vec()))
            .filter(hopr_safe_redeemed_stats::Column::NodeAddress.eq(node_address.as_ref().to_vec()))
            .one(db.conn(TargetDb::Index))
            .await?
            .map(model_to_entry)
            .transpose()?;
        assert!(loaded.is_none());

        Ok(())
    }

    #[tokio::test]
    async fn test_record_is_partitioned_by_safe_and_node() -> anyhow::Result<()> {
        let db = BlokliDb::new_in_memory().await?;
        let safe_address = random_address();
        let first_node = random_address();
        let second_node = random_address();

        let first = db
            .record_safe_ticket_redeemed(None, safe_address, first_node, HoprBalance::from(10_u64), 100, 1, 1)
            .await?;
        assert_eq!(first.redeemed_amount, HoprBalance::from(10_u64));
        assert_eq!(first.redemption_count, 1);
        assert_eq!(first.rejected_amount, HoprBalance::zero());
        assert_eq!(first.rejection_count, 0);

        let second = db
            .record_safe_ticket_redeemed(None, safe_address, second_node, HoprBalance::from(3_u64), 101, 1, 2)
            .await?;
        assert_eq!(second.redeemed_amount, HoprBalance::from(3_u64));
        assert_eq!(second.redemption_count, 1);
        assert_eq!(second.rejected_amount, HoprBalance::zero());
        assert_eq!(second.rejection_count, 0);

        let first_updated = db
            .record_safe_ticket_redeemed(None, safe_address, first_node, HoprBalance::from(5_u64), 102, 2, 0)
            .await?;
        assert_eq!(first_updated.redeemed_amount, HoprBalance::from(15_u64));
        assert_eq!(first_updated.redemption_count, 2);
        assert_eq!(first_updated.rejected_amount, HoprBalance::zero());
        assert_eq!(first_updated.rejection_count, 0);

        let loaded_first = HoprSafeRedeemedStats::find()
            .filter(hopr_safe_redeemed_stats::Column::SafeAddress.eq(safe_address.as_ref().to_vec()))
            .filter(hopr_safe_redeemed_stats::Column::NodeAddress.eq(first_node.as_ref().to_vec()))
            .one(db.conn(TargetDb::Index))
            .await?
            .map(model_to_entry)
            .transpose()?
            .expect("first node stats should exist");
        let loaded_second = HoprSafeRedeemedStats::find()
            .filter(hopr_safe_redeemed_stats::Column::SafeAddress.eq(safe_address.as_ref().to_vec()))
            .filter(hopr_safe_redeemed_stats::Column::NodeAddress.eq(second_node.as_ref().to_vec()))
            .one(db.conn(TargetDb::Index))
            .await?
            .map(model_to_entry)
            .transpose()?
            .expect("second node stats should exist");

        assert_eq!(loaded_first.redeemed_amount, HoprBalance::from(15_u64));
        assert_eq!(loaded_first.redemption_count, 2);
        assert_eq!(loaded_first.rejected_amount, HoprBalance::zero());
        assert_eq!(loaded_first.rejection_count, 0);
        assert_eq!(loaded_second.redeemed_amount, HoprBalance::from(3_u64));
        assert_eq!(loaded_second.redemption_count, 1);
        assert_eq!(loaded_second.rejected_amount, HoprBalance::zero());
        assert_eq!(loaded_second.rejection_count, 0);

        Ok(())
    }

    #[tokio::test]
    async fn test_record_rejected_stats_and_aggregate() -> anyhow::Result<()> {
        let db = BlokliDb::new_in_memory().await?;
        let safe_address = random_address();
        let node_address = random_address();

        let first = db
            .record_safe_ticket_rejected(None, safe_address, node_address, HoprBalance::from(7_u64), 200, 1, 1)
            .await?;
        assert_eq!(first.redeemed_amount, HoprBalance::zero());
        assert_eq!(first.redemption_count, 0);
        assert_eq!(first.rejected_amount, HoprBalance::from(7_u64));
        assert_eq!(first.rejection_count, 1);

        let second = db
            .record_safe_ticket_redeemed(None, safe_address, node_address, HoprBalance::from(3_u64), 201, 1, 2)
            .await?;
        assert_eq!(second.redeemed_amount, HoprBalance::from(3_u64));
        assert_eq!(second.redemption_count, 1);
        assert_eq!(second.rejected_amount, HoprBalance::from(7_u64));
        assert_eq!(second.rejection_count, 1);

        let third = db
            .record_safe_ticket_rejected(None, safe_address, node_address, HoprBalance::from(5_u64), 202, 1, 3)
            .await?;
        assert_eq!(third.redeemed_amount, HoprBalance::from(3_u64));
        assert_eq!(third.redemption_count, 1);
        assert_eq!(third.rejected_amount, HoprBalance::from(12_u64));
        assert_eq!(third.rejection_count, 2);

        let aggregated = db
            .get_aggregated_redeemed_stats(Some(safe_address), Some(node_address))
            .await?;
        assert_eq!(aggregated.redeemed_amount, HoprBalance::from(3_u64));
        assert_eq!(aggregated.redemption_count, 1);
        assert_eq!(aggregated.rejected_amount, HoprBalance::from(12_u64));
        assert_eq!(aggregated.rejection_count, 2);

        Ok(())
    }
}
