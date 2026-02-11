//! Channel aggregation utilities using the `channel_current` database view

use chrono::{DateTime, Utc};
use hopr_primitive_types::prelude::{HoprBalance, IntoEndian};
use sea_orm::{ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter};

use crate::views::channel_current;

/// Aggregated channel data with state information
#[derive(Debug, Clone)]
pub struct AggregatedChannel {
    pub concrete_channel_id: String,
    pub source: i64,
    pub destination: i64,
    pub balance: String,
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
/// # Arguments
/// * `conn` - Database connection
/// * `source_key_id` - Optional filter by source account ID
/// * `destination_key_id` - Optional filter by destination account ID
/// * `concrete_channel_id` - Optional filter by concrete channel ID
/// * `status` - Optional filter by channel status
///
/// # Returns
/// * `Result<Vec<AggregatedChannel>, sea_orm::DbErr>` - List of aggregated channels with state
pub async fn fetch_channels_with_state<C>(
    conn: &C,
    source_key_id: Option<i64>,
    destination_key_id: Option<i64>,
    concrete_channel_id: Option<String>,
    status: Option<i16>,
) -> Result<Vec<AggregatedChannel>, sea_orm::DbErr>
where
    C: ConnectionTrait,
{
    let mut query = channel_current::Entity::find();

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

            let balance_bytes_32: [u8; 32] = {
                let mut bytes = [0u8; 32];
                bytes[20..32].copy_from_slice(&balance_slice);
                bytes
            };

            let hopr_balance = HoprBalance::from_be_bytes(balance_bytes_32);

            Ok(AggregatedChannel {
                concrete_channel_id: row.concrete_channel_id,
                source: row.source,
                destination: row.destination,
                balance: hopr_balance.to_string(),
                status: row.status,
                epoch: row.epoch,
                ticket_index: row.ticket_index,
                closure_time: row.closure_time.map(|time| time.with_timezone(&Utc)),
            })
        })
        .collect::<Result<Vec<_>, sea_orm::DbErr>>()?;

    Ok(result)
}
