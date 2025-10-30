//! Helper functions for querying account and channel state
//!
//! This module provides convenient functions for:
//! - Current state queries (using database views)
//! - Point-in-time state queries
//! - Temporal range queries for state history
//!
//! # Current State Queries
//!
//! Current state is retrieved from the `channel_current` and `account_current` views,
//! which use window functions to efficiently get the latest state for each object.
//!
//! # Temporal Queries
//!
//! Temporal queries allow querying state at any point in blockchain history using
//! (block, tx_index, log_index) position tuples.
//!
//! # Example
//!
//! ```rust,ignore
//! // Get current channel state
//! let current = get_current_channel_state(&db, channel_id).await?;
//!
//! // Get channel state at block 1000
//! let historical = get_channel_state_at(&db, channel_id, 1000, 0, 0).await?;
//!
//! // Get all state changes between two blocks
//! let history = get_channel_state_history(&db, channel_id, 900, 1100).await?;
//! ```

use sea_orm::{ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, QueryOrder};

use crate::{errors::Result, events::BlockPosition};

/// Get the current (latest) channel state for a channel
///
/// This uses the `channel_current` view which efficiently retrieves the latest state.
///
/// # Arguments
///
/// * `db` - Database connection
/// * `channel_id` - Channel ID to query
///
/// # Returns
///
/// The current channel state record, or None if channel doesn't exist or has no state
pub async fn get_current_channel_state(
    db: &DatabaseConnection,
    channel_id: i32,
) -> Result<Option<blokli_db_entity::channel_state::Model>> {
    use blokli_db_entity::{channel_state, prelude::ChannelState};

    // TODO(Phase 2-3): Query from channel_current view instead of direct table query
    // For now, manually get the latest state
    let state = ChannelState::find()
        .filter(channel_state::Column::ChannelId.eq(channel_id))
        .order_by_desc(channel_state::Column::PublishedBlock)
        .order_by_desc(channel_state::Column::PublishedTxIndex)
        .order_by_desc(channel_state::Column::PublishedLogIndex)
        .one(db)
        .await?;

    Ok(state)
}

/// Get channel state at a specific point in time
///
/// Returns the most recent channel state at or before the given position.
///
/// # Arguments
///
/// * `db` - Database connection
/// * `channel_id` - Channel ID to query
/// * `position` - Blockchain position to query at
///
/// # Returns
///
/// The channel state at the given position, or None if no state exists before that point
pub async fn get_channel_state_at(
    db: &DatabaseConnection,
    channel_id: i32,
    position: BlockPosition,
) -> Result<Option<blokli_db_entity::channel_state::Model>> {
    use blokli_db_entity::{channel_state, prelude::ChannelState};
    use sea_orm::sea_query::Condition;

    // Query for state at or before the given position
    // Position comparison: (block < target_block) OR
    //                      (block = target_block AND tx_index < target_tx) OR
    //                      (block = target_block AND tx_index = target_tx AND log_index <= target_log)
    let condition = Condition::any()
        .add(channel_state::Column::PublishedBlock.lt(position.block))
        .add(
            Condition::all()
                .add(channel_state::Column::PublishedBlock.eq(position.block))
                .add(channel_state::Column::PublishedTxIndex.lt(position.tx_index)),
        )
        .add(
            Condition::all()
                .add(channel_state::Column::PublishedBlock.eq(position.block))
                .add(channel_state::Column::PublishedTxIndex.eq(position.tx_index))
                .add(channel_state::Column::PublishedLogIndex.lte(position.log_index)),
        );

    let state = ChannelState::find()
        .filter(channel_state::Column::ChannelId.eq(channel_id))
        .filter(condition)
        .order_by_desc(channel_state::Column::PublishedBlock)
        .order_by_desc(channel_state::Column::PublishedTxIndex)
        .order_by_desc(channel_state::Column::PublishedLogIndex)
        .one(db)
        .await?;

    Ok(state)
}

/// Get all channel state changes in a block range
///
/// Returns all state changes for a channel between two positions (inclusive).
///
/// # Arguments
///
/// * `db` - Database connection
/// * `channel_id` - Channel ID to query
/// * `from` - Start position (inclusive)
/// * `to` - End position (inclusive)
///
/// # Returns
///
/// Vector of channel state changes, ordered by position (oldest first)
pub async fn get_channel_state_history(
    db: &DatabaseConnection,
    channel_id: i32,
    from: BlockPosition,
    to: BlockPosition,
) -> Result<Vec<blokli_db_entity::channel_state::Model>> {
    use blokli_db_entity::{channel_state, prelude::ChannelState};
    use sea_orm::sea_query::Condition;

    // Build condition for range: state >= from AND state <= to
    let from_condition = Condition::any()
        .add(channel_state::Column::PublishedBlock.gt(from.block))
        .add(
            Condition::all()
                .add(channel_state::Column::PublishedBlock.eq(from.block))
                .add(channel_state::Column::PublishedTxIndex.gt(from.tx_index)),
        )
        .add(
            Condition::all()
                .add(channel_state::Column::PublishedBlock.eq(from.block))
                .add(channel_state::Column::PublishedTxIndex.eq(from.tx_index))
                .add(channel_state::Column::PublishedLogIndex.gte(from.log_index)),
        );

    let to_condition = Condition::any()
        .add(channel_state::Column::PublishedBlock.lt(to.block))
        .add(
            Condition::all()
                .add(channel_state::Column::PublishedBlock.eq(to.block))
                .add(channel_state::Column::PublishedTxIndex.lt(to.tx_index)),
        )
        .add(
            Condition::all()
                .add(channel_state::Column::PublishedBlock.eq(to.block))
                .add(channel_state::Column::PublishedTxIndex.eq(to.tx_index))
                .add(channel_state::Column::PublishedLogIndex.lte(to.log_index)),
        );

    let states = ChannelState::find()
        .filter(channel_state::Column::ChannelId.eq(channel_id))
        .filter(from_condition)
        .filter(to_condition)
        .order_by_asc(channel_state::Column::PublishedBlock)
        .order_by_asc(channel_state::Column::PublishedTxIndex)
        .order_by_asc(channel_state::Column::PublishedLogIndex)
        .all(db)
        .await?;

    Ok(states)
}

/// Get the current (latest) account state for an account
///
/// This uses the `account_current` view which efficiently retrieves the latest state.
///
/// # Arguments
///
/// * `db` - Database connection
/// * `account_id` - Account ID to query
///
/// # Returns
///
/// The current account state record, or None if account doesn't exist or has no state
pub async fn get_current_account_state(
    db: &DatabaseConnection,
    account_id: i32,
) -> Result<Option<blokli_db_entity::account_state::Model>> {
    use blokli_db_entity::{account_state, prelude::AccountState};

    // TODO(Phase 2-3): Query from account_current view instead of direct table query
    // For now, manually get the latest state
    let state = AccountState::find()
        .filter(account_state::Column::AccountId.eq(account_id))
        .order_by_desc(account_state::Column::PublishedBlock)
        .order_by_desc(account_state::Column::PublishedTxIndex)
        .order_by_desc(account_state::Column::PublishedLogIndex)
        .one(db)
        .await?;

    Ok(state)
}

/// Get account state at a specific point in time
///
/// Returns the most recent account state at or before the given position.
///
/// # Arguments
///
/// * `db` - Database connection
/// * `account_id` - Account ID to query
/// * `position` - Blockchain position to query at
///
/// # Returns
///
/// The account state at the given position, or None if no state exists before that point
pub async fn get_account_state_at(
    db: &DatabaseConnection,
    account_id: i32,
    position: BlockPosition,
) -> Result<Option<blokli_db_entity::account_state::Model>> {
    use blokli_db_entity::{account_state, prelude::AccountState};
    use sea_orm::sea_query::Condition;

    let condition = Condition::any()
        .add(account_state::Column::PublishedBlock.lt(position.block))
        .add(
            Condition::all()
                .add(account_state::Column::PublishedBlock.eq(position.block))
                .add(account_state::Column::PublishedTxIndex.lt(position.tx_index)),
        )
        .add(
            Condition::all()
                .add(account_state::Column::PublishedBlock.eq(position.block))
                .add(account_state::Column::PublishedTxIndex.eq(position.tx_index))
                .add(account_state::Column::PublishedLogIndex.lte(position.log_index)),
        );

    let state = AccountState::find()
        .filter(account_state::Column::AccountId.eq(account_id))
        .filter(condition)
        .order_by_desc(account_state::Column::PublishedBlock)
        .order_by_desc(account_state::Column::PublishedTxIndex)
        .order_by_desc(account_state::Column::PublishedLogIndex)
        .one(db)
        .await?;

    Ok(state)
}

/// Get all account state changes in a block range
///
/// Returns all state changes for an account between two positions (inclusive).
///
/// # Arguments
///
/// * `db` - Database connection
/// * `account_id` - Account ID to query
/// * `from` - Start position (inclusive)
/// * `to` - End position (inclusive)
///
/// # Returns
///
/// Vector of account state changes, ordered by position (oldest first)
pub async fn get_account_state_history(
    db: &DatabaseConnection,
    account_id: i32,
    from: BlockPosition,
    to: BlockPosition,
) -> Result<Vec<blokli_db_entity::account_state::Model>> {
    use blokli_db_entity::{account_state, prelude::AccountState};
    use sea_orm::sea_query::Condition;

    let from_condition = Condition::any()
        .add(account_state::Column::PublishedBlock.gt(from.block))
        .add(
            Condition::all()
                .add(account_state::Column::PublishedBlock.eq(from.block))
                .add(account_state::Column::PublishedTxIndex.gt(from.tx_index)),
        )
        .add(
            Condition::all()
                .add(account_state::Column::PublishedBlock.eq(from.block))
                .add(account_state::Column::PublishedTxIndex.eq(from.tx_index))
                .add(account_state::Column::PublishedLogIndex.gte(from.log_index)),
        );

    let to_condition = Condition::any()
        .add(account_state::Column::PublishedBlock.lt(to.block))
        .add(
            Condition::all()
                .add(account_state::Column::PublishedBlock.eq(to.block))
                .add(account_state::Column::PublishedTxIndex.lt(to.tx_index)),
        )
        .add(
            Condition::all()
                .add(account_state::Column::PublishedBlock.eq(to.block))
                .add(account_state::Column::PublishedTxIndex.eq(to.tx_index))
                .add(account_state::Column::PublishedLogIndex.lte(to.log_index)),
        );

    let states = AccountState::find()
        .filter(account_state::Column::AccountId.eq(account_id))
        .filter(from_condition)
        .filter(to_condition)
        .order_by_asc(account_state::Column::PublishedBlock)
        .order_by_asc(account_state::Column::PublishedTxIndex)
        .order_by_asc(account_state::Column::PublishedLogIndex)
        .all(db)
        .await?;

    Ok(states)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_block_position_usage() {
        let pos = BlockPosition {
            block: 1000,
            tx_index: 5,
            log_index: 2,
        };
        assert_eq!(pos.block, 1000);
        assert_eq!(pos.tx_index, 5);
        assert_eq!(pos.log_index, 2);
    }
}
