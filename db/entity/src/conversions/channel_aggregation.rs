//! Channel aggregation utilities using the `channel_current` database view

use chrono::{DateTime, Utc};
use futures::StreamExt;
use hopr_types::primitive::prelude::{Address, HoprBalance, IntoEndian};
use sea_orm::{ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter, QuerySelect, StreamTrait};

use crate::{
    hopr_balance, hopr_safe_contract,
    views::{account_current, channel_current},
};

/// Aggregated channel data with state information
#[derive(Debug, Clone)]
pub struct AggregatedChannel {
    pub concrete_channel_id: String,
    pub source: i64,
    pub destination: i64,
    pub balance: HoprBalance,
    pub status: i16,
    pub epoch: i64,
    pub ticket_index: i64,
    pub closure_time: Option<DateTime<Utc>>,
}

/// Fetch channels with their current state using the `channel_current` view
///
/// The `channel_current` view already returns one row per channel with the latest state,
/// so no deduplication or ordering is needed in application code.
///
/// When `safe_address` is provided, only channels where the source account is currently
/// associated with that safe are included.
///
/// # Arguments
/// * `conn` - Database connection
/// * `source_key_id` - Optional filter by source account ID
/// * `destination_key_id` - Optional filter by destination account ID
/// * `concrete_channel_id` - Optional filter by concrete channel ID
/// * `status` - Optional filter by channel status
/// * `safe_address` - Optional safe contract address; restricts to channels where the source belongs to this safe
///
/// # Returns
/// * `Result<Vec<AggregatedChannel>, sea_orm::DbErr>` - List of aggregated channels with state
pub async fn fetch_channels_with_state<C>(
    conn: &C,
    source_key_id: Option<i64>,
    destination_key_id: Option<i64>,
    concrete_channel_id: Option<String>,
    status: Option<i16>,
    safe_address: Option<Address>,
) -> Result<Vec<AggregatedChannel>, sea_orm::DbErr>
where
    C: ConnectionTrait,
{
    let mut query = channel_current::Entity::find();

    // When a safe address is given, first resolve which account keyids belong to it
    if let Some(ref safe_addr) = safe_address {
        let safe_addr_bytes = safe_addr.as_ref().to_vec();
        let account_ids: Vec<i64> = account_current::Entity::find()
            .filter(account_current::Column::SafeAddress.eq(safe_addr_bytes))
            .all(conn)
            .await?
            .into_iter()
            .map(|row| row.account_id)
            .collect();

        if account_ids.is_empty() {
            return Ok(Vec::new());
        }
        query = query.filter(channel_current::Column::Source.is_in(account_ids));
    }

    if let Some(src) = source_key_id {
        query = query.filter(channel_current::Column::Source.eq(src));
    }

    if let Some(dst) = destination_key_id {
        query = query.filter(channel_current::Column::Destination.eq(dst));
    }

    if let Some(ch_id) = concrete_channel_id {
        query = query.filter(
            channel_current::Column::ConcreteChannelId.eq(ch_id.strip_prefix("0x").unwrap_or(&ch_id).to_string()),
        );
    }

    if let Some(status_filter) = status {
        query = query.filter(channel_current::Column::Status.eq(status_filter));
    }

    let rows = query.all(conn).await?;

    let result = rows
        .into_iter()
        .map(|row| {
            let balance_slice: [u8; 12] = row.balance.as_slice().try_into().map_err(|_| {
                sea_orm::DbErr::Custom(format!(
                    "Invalid balance length for channel {}: expected 12, got {}",
                    row.concrete_channel_id,
                    row.balance.len()
                ))
            })?;

            Ok(AggregatedChannel {
                concrete_channel_id: row.concrete_channel_id,
                source: row.source,
                destination: row.destination,
                balance: HoprBalance::from_be_bytes(balance_slice),
                status: row.status,
                epoch: row.epoch,
                ticket_index: row.ticket_index,
                closure_time: row.closure_time.map(|time| time.with_timezone(&Utc)),
            })
        })
        .collect::<Result<Vec<_>, sea_orm::DbErr>>()?;

    Ok(result)
}

/// Aggregated channel statistics: count and total wxHOPR balance.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AggregatedChannelStats {
    /// Number of channels matching the filters.
    pub count: u64,
    /// Sum of wxHOPR balances across all matching channels.
    pub balance: HoprBalance,
}

/// Fetch channel count and total wxHOPR balance for channels matching optional filters.
///
/// Only the `balance` column is fetched from the database, reducing data transfer
/// compared to [`fetch_channels_with_state`] which retrieves all columns.
/// The count is derived from the number of returned rows.
///
/// When `safe_address` is provided, only channels where the source account is currently
/// associated with that safe are included.
///
/// # Arguments
/// * `conn` - Database connection
/// * `source_key_id` - Optional filter by source account ID
/// * `destination_key_id` - Optional filter by destination account ID
/// * `concrete_channel_id` - Optional filter by concrete channel ID
/// * `status` - Optional filter by channel status
/// * `safe_address` - Optional safe contract address; restricts to channels where the source belongs to this safe
pub async fn fetch_channel_stats<C>(
    conn: &C,
    source_key_id: Option<i64>,
    destination_key_id: Option<i64>,
    concrete_channel_id: Option<String>,
    status: Option<i16>,
    safe_address: Option<Address>,
) -> Result<AggregatedChannelStats, sea_orm::DbErr>
where
    C: ConnectionTrait + StreamTrait,
{
    let mut query = channel_current::Entity::find()
        .select_only()
        .column(channel_current::Column::Balance);

    if let Some(ref safe_addr) = safe_address {
        let safe_addr_bytes = safe_addr.as_ref().to_vec();
        let account_ids: Vec<i64> = account_current::Entity::find()
            .filter(account_current::Column::SafeAddress.eq(safe_addr_bytes))
            .all(conn)
            .await?
            .into_iter()
            .map(|row| row.account_id)
            .collect();

        query = query.filter(channel_current::Column::Source.is_in(account_ids));
    }

    if let Some(src) = source_key_id {
        query = query.filter(channel_current::Column::Source.eq(src));
    }

    if let Some(dst) = destination_key_id {
        query = query.filter(channel_current::Column::Destination.eq(dst));
    }

    if let Some(ch_id) = concrete_channel_id {
        query = query.filter(
            channel_current::Column::ConcreteChannelId.eq(ch_id.strip_prefix("0x").unwrap_or(&ch_id).to_string()),
        );
    }

    if let Some(status_filter) = status {
        query = query.filter(channel_current::Column::Status.eq(status_filter));
    }

    let mut stream = query.into_tuple::<Vec<u8>>().stream(conn).await?;

    let mut count = 0u64;
    let mut total_balance = HoprBalance::zero();
    while let Some(result) = stream.next().await {
        let balance_bytes = result?;
        let slice: [u8; 12] = balance_bytes.as_slice().try_into().map_err(|_| {
            sea_orm::DbErr::Custom(format!(
                "Invalid balance length in channel_current: expected 12, got {}",
                balance_bytes.len()
            ))
        })?;
        total_balance = total_balance + HoprBalance::from_be_bytes(slice);
        count += 1;
    }

    Ok(AggregatedChannelStats {
        count,
        balance: total_balance,
    })
}

/// Aggregated wxHOPR balance held across all indexed safe contracts.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AggregatedSafeHoprBalance {
    /// Sum of wxHOPR balances for all safe contract addresses.
    pub balance: HoprBalance,
    /// Number of safe contracts counted.
    pub count: u32,
}

/// Fetch the total wxHOPR balance held across indexed safe contracts.
///
/// When `owner_address` is provided, only safes whose registered accounts have
/// that chain key are included. Otherwise all indexed safe contracts are summed.
/// Safes with no indexed balance entry contribute zero to the sum.
///
/// # Arguments
///
/// * `conn` - Database connection
/// * `owner_address` - Optional chain key (owner address); restricts to safes associated with this key
pub async fn fetch_safes_balance<C>(
    conn: &C,
    owner_address: Option<Address>,
) -> Result<AggregatedSafeHoprBalance, sea_orm::DbErr>
where
    C: ConnectionTrait + StreamTrait,
{
    let safe_addresses: Vec<Vec<u8>> = if let Some(owner) = owner_address {
        let owner_bytes = owner.as_ref().to_vec();
        account_current::Entity::find()
            .filter(account_current::Column::ChainKey.eq(owner_bytes))
            .all(conn)
            .await?
            .into_iter()
            .filter_map(|row| row.safe_address)
            .collect()
    } else {
        hopr_safe_contract::Entity::find()
            .all(conn)
            .await?
            .into_iter()
            .map(|row| row.address)
            .collect()
    };

    let safe_count =
        u32::try_from(safe_addresses.len()).map_err(|e| sea_orm::DbErr::Custom(format!("Safe count overflow: {e}")))?;

    if safe_addresses.is_empty() {
        return Ok(AggregatedSafeHoprBalance {
            balance: HoprBalance::zero(),
            count: 0,
        });
    }

    let mut stream = hopr_balance::Entity::find()
        .filter(hopr_balance::Column::Address.is_in(safe_addresses))
        .stream(conn)
        .await?;

    let mut total_balance = HoprBalance::zero();
    while let Some(result) = stream.next().await {
        let row = result?;
        let balance_slice: [u8; 12] = row.balance.as_slice().try_into().map_err(|_| {
            sea_orm::DbErr::Custom(format!(
                "Invalid balance length in hopr_balance: expected 12, got {}",
                row.balance.len()
            ))
        })?;
        total_balance = total_balance + HoprBalance::from_be_bytes(balance_slice);
    }

    Ok(AggregatedSafeHoprBalance {
        balance: total_balance,
        count: safe_count,
    })
}
