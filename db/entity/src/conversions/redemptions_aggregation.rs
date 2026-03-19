//! Aggregation utilities for redeemed ticket statistics

use hopr_types::primitive::{
    prelude::{Address, HoprBalance},
    traits::IntoEndian,
};
use sea_orm::{ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter};

use crate::hopr_safe_redeemed_stats;

/// Aggregated redeemed ticket statistics across all matching rows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AggregatedRedeemedStats {
    /// Sum of all redeemed HOPR token amounts across matching rows.
    pub redeemed_amount: HoprBalance,
    /// Total number of ticket redemptions across matching rows.
    pub redemption_count: u64,
}

/// Fetch aggregated redeemed ticket statistics with optional safe/node filters.
///
/// Queries the `hopr_safe_redeemed_stats` table, applying optional address filters,
/// then sums `redemption_count` using SQL and accumulates `redeemed_amount` (stored
/// as big-endian binary) in application code.
///
/// # Arguments
///
/// * `conn` - Database connection
/// * `safe_address` - Optional safe contract address filter
/// * `node_address` - Optional destination node address filter
pub async fn fetch_aggregated_redeemed_stats<C>(
    conn: &C,
    safe_address: Option<Address>,
    node_address: Option<Address>,
) -> Result<AggregatedRedeemedStats, sea_orm::DbErr>
where
    C: ConnectionTrait,
{
    let mut query = hopr_safe_redeemed_stats::Entity::find();
    if let Some(addr) = safe_address.as_ref() {
        query = query.filter(hopr_safe_redeemed_stats::Column::SafeAddress.eq(addr.as_ref().to_vec()));
    }
    if let Some(addr) = node_address.as_ref() {
        query = query.filter(hopr_safe_redeemed_stats::Column::NodeAddress.eq(addr.as_ref().to_vec()));
    }

    let rows = query.all(conn).await?;

    let mut total_amount = HoprBalance::zero();
    let mut total_count: u64 = 0;

    for row in rows {
        total_amount = total_amount + HoprBalance::from_be_bytes(row.redeemed_amount.as_slice());
        total_count = total_count.saturating_add(u64::try_from(row.redemption_count).unwrap_or(u64::MAX));
    }

    Ok(AggregatedRedeemedStats {
        redeemed_amount: total_amount,
        redemption_count: total_count,
    })
}
