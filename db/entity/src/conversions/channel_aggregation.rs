//! Channel aggregation utilities with optimized batch loading

use std::collections::HashMap;

use chrono::{DateTime, Utc};
use sea_orm::{ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, QueryOrder};

use super::balances::hopr_balance_to_string;
use crate::codegen::{channel, channel_state};

/// Aggregated channel data with state information
#[derive(Debug, Clone)]
pub struct AggregatedChannel {
    pub concrete_channel_id: String,
    pub source: i32,
    pub destination: i32,
    pub balance: String,
    pub status: i8,
    pub epoch: i64,
    pub ticket_index: i64,
    pub closure_time: Option<DateTime<Utc>>,
}

/// Fetch channels with their current state using optimized batch loading
///
/// This function eliminates N+1 queries by:
/// 1. Fetching channels matching the filters (1 query)
/// 2. Batch loading latest channel_state for all channels (1 query with ordering)
/// 3. Aggregating the data in memory
///
/// # Arguments
/// * `db` - Database connection
/// * `source_key_id` - Optional filter by source account ID
/// * `destination_key_id` - Optional filter by destination account ID
/// * `concrete_channel_id` - Optional filter by concrete channel ID
/// * `status` - Optional filter by channel status
///
/// # Returns
/// * `Result<Vec<AggregatedChannel>, sea_orm::DbErr>` - List of aggregated channels with state
pub async fn fetch_channels_with_state(
    db: &DatabaseConnection,
    source_key_id: Option<i32>,
    destination_key_id: Option<i32>,
    concrete_channel_id: Option<String>,
    status: Option<i8>,
) -> Result<Vec<AggregatedChannel>, sea_orm::DbErr> {
    // 1. Build query with filters for channels
    let mut query = channel::Entity::find();

    if let Some(src) = source_key_id {
        query = query.filter(channel::Column::Source.eq(src));
    }

    if let Some(dst) = destination_key_id {
        query = query.filter(channel::Column::Destination.eq(dst));
    }

    if let Some(ch_id) = concrete_channel_id {
        query = query.filter(channel::Column::ConcreteChannelId.eq(ch_id));
    }

    let channels = query.all(db).await?;

    if channels.is_empty() {
        return Ok(Vec::new());
    }

    // Collect all channel IDs
    let channel_ids: Vec<i32> = channels.iter().map(|c| c.id).collect();

    // 2. Batch query channel_state for all channels to get latest state
    let channel_states = channel_state::Entity::find()
        .filter(channel_state::Column::ChannelId.is_in(channel_ids))
        .order_by_desc(channel_state::Column::PublishedBlock)
        .order_by_desc(channel_state::Column::PublishedTxIndex)
        .order_by_desc(channel_state::Column::PublishedLogIndex)
        .all(db)
        .await?;

    // Build map of channel_id -> latest state (only keep first state per channel due to ordering)
    let mut state_map: HashMap<i32, channel_state::Model> = HashMap::new();
    for state in channel_states {
        // Only insert if we haven't seen this channel yet (first occurrence is latest due to ordering)
        state_map.entry(state.channel_id).or_insert(state);
    }

    // 3. Aggregate data, filtering by status if requested
    let result = channels
        .into_iter()
        .filter_map(|channel| {
            // Get the latest state for this channel
            let state = state_map.get(&channel.id)?;

            // Apply status filter if provided
            if let Some(status_filter) = status {
                if state.status != status_filter {
                    return None;
                }
            }

            Some(AggregatedChannel {
                concrete_channel_id: channel.concrete_channel_id,
                source: channel.source,
                destination: channel.destination,
                balance: hopr_balance_to_string(&state.balance),
                status: state.status,
                epoch: state.epoch,
                ticket_index: state.ticket_index,
                closure_time: state.closure_time,
            })
        })
        .collect();

    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    // Note: These tests would require a test database setup
    // For now, we just ensure the module compiles
    #[test]
    fn test_module_compiles() {
        assert!(true);
    }
}
