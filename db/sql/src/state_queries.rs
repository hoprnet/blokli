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
    use blokli_db_entity::{account_state, channel_state, prelude::*};
    use chrono::Utc;
    use sea_orm::{ActiveModelTrait, ActiveValue, EntityTrait};

    use super::*;
    use crate::db::BlokliDb;

    // Helper to create test channel in database
    async fn create_test_channel(db: &DatabaseConnection) -> anyhow::Result<i32> {
        use blokli_db_entity::channel;

        let channel = channel::ActiveModel {
            source: ActiveValue::Set(vec![
                1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20,
            ]),
            destination: ActiveValue::Set(vec![
                21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31, 32, 33, 34, 35, 36, 37, 38, 39, 40,
            ]),
            published_block: ActiveValue::Set(100),
            published_tx_index: ActiveValue::Set(0),
            published_log_index: ActiveValue::Set(0),
            ..Default::default()
        };

        let result = channel.insert(db).await?;
        Ok(result.id)
    }

    // Helper to create test account in database
    async fn create_test_account(db: &DatabaseConnection) -> anyhow::Result<i32> {
        use blokli_db_entity::account;

        let account = account::ActiveModel {
            chain_key: ActiveValue::Set(vec![
                1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20,
            ]),
            packet_key: ActiveValue::Set(vec![
                21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31, 32, 33, 34, 35, 36, 37, 38, 39, 40, 41, 42, 43, 44, 45, 46,
                47, 48, 49, 50, 51, 52,
            ]),
            published_block: ActiveValue::Set(100),
            published_tx_index: ActiveValue::Set(0),
            published_log_index: ActiveValue::Set(0),
            ..Default::default()
        };

        let result = account.insert(db).await?;
        Ok(result.id)
    }

    // Helper to insert channel state
    async fn insert_channel_state(
        db: &DatabaseConnection,
        channel_id: i32,
        block: i64,
        tx_index: i64,
        log_index: i64,
        balance: Vec<u8>,
        status: i8,
    ) -> anyhow::Result<i32> {
        let state = channel_state::ActiveModel {
            channel_id: ActiveValue::Set(channel_id),
            balance: ActiveValue::Set(balance),
            status: ActiveValue::Set(status),
            epoch: ActiveValue::Set(0),
            ticket_index: ActiveValue::Set(0),
            closure_time: ActiveValue::Set(None),
            corrupted_state: ActiveValue::Set(false),
            published_block: ActiveValue::Set(block),
            published_tx_index: ActiveValue::Set(tx_index),
            published_log_index: ActiveValue::Set(log_index),
            reorg_correction: ActiveValue::Set(false),
            ..Default::default()
        };

        let result = state.insert(db).await?;
        Ok(result.id)
    }

    // Helper to insert account state
    async fn insert_account_state(
        db: &DatabaseConnection,
        account_id: i32,
        block: i64,
        tx_index: i64,
        log_index: i64,
    ) -> anyhow::Result<i32> {
        let state = account_state::ActiveModel {
            account_id: ActiveValue::Set(account_id),
            published_block: ActiveValue::Set(block),
            published_tx_index: ActiveValue::Set(tx_index),
            published_log_index: ActiveValue::Set(log_index),
            ..Default::default()
        };

        let result = state.insert(db).await?;
        Ok(result.id)
    }

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

    #[test]
    fn test_block_position_ordering() {
        let pos1 = BlockPosition {
            block: 100,
            tx_index: 0,
            log_index: 0,
        };
        let pos2 = BlockPosition {
            block: 100,
            tx_index: 0,
            log_index: 1,
        };
        let pos3 = BlockPosition {
            block: 100,
            tx_index: 1,
            log_index: 0,
        };
        let pos4 = BlockPosition {
            block: 101,
            tx_index: 0,
            log_index: 0,
        };

        assert!(pos1 < pos2);
        assert!(pos2 < pos3);
        assert!(pos3 < pos4);
    }

    // ==================== Channel State Tests ====================

    #[tokio::test]
    async fn test_get_current_channel_state_empty_db() -> anyhow::Result<()> {
        let db = BlokliDb::new_in_memory().await?;
        let channel_id = create_test_channel(&db).await?;

        let result = get_current_channel_state(&db, channel_id).await?;
        assert!(result.is_none(), "Should return None for channel with no state");

        Ok(())
    }

    #[tokio::test]
    async fn test_get_current_channel_state_single_version() -> anyhow::Result<()> {
        let db = BlokliDb::new_in_memory().await?;
        let channel_id = create_test_channel(&db).await?;

        let balance = vec![1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
        insert_channel_state(&db, channel_id, 1000, 5, 2, balance.clone(), 1).await?;

        let result = get_current_channel_state(&db, channel_id).await?;
        assert!(result.is_some());

        let state = result.unwrap();
        assert_eq!(state.channel_id, channel_id);
        assert_eq!(state.balance, balance);
        assert_eq!(state.status, 1);

        Ok(())
    }

    #[tokio::test]
    async fn test_get_current_channel_state_multiple_versions() -> anyhow::Result<()> {
        let db = BlokliDb::new_in_memory().await?;
        let channel_id = create_test_channel(&db).await?;

        // Insert states at different positions
        insert_channel_state(&db, channel_id, 1000, 5, 2, vec![1; 12], 1).await?;
        insert_channel_state(&db, channel_id, 1000, 10, 0, vec![2; 12], 1).await?;
        insert_channel_state(&db, channel_id, 1001, 0, 0, vec![3; 12], 1).await?;
        let latest_state_id = insert_channel_state(&db, channel_id, 1001, 5, 1, vec![4; 12], 2).await?;

        let result = get_current_channel_state(&db, channel_id).await?;
        assert!(result.is_some());

        let state = result.unwrap();
        assert_eq!(state.id, latest_state_id, "Should return the latest state");
        assert_eq!(state.balance, vec![4; 12]);
        assert_eq!(state.status, 2);

        Ok(())
    }

    #[tokio::test]
    async fn test_get_channel_state_at_exact_position() -> anyhow::Result<()> {
        let db = BlokliDb::new_in_memory().await?;
        let channel_id = create_test_channel(&db).await?;

        let state_id = insert_channel_state(&db, channel_id, 1000, 5, 2, vec![1; 12], 1).await?;

        let position = BlockPosition {
            block: 1000,
            tx_index: 5,
            log_index: 2,
        };

        let result = get_channel_state_at(&db, channel_id, position).await?;
        assert!(result.is_some());
        assert_eq!(result.unwrap().id, state_id);

        Ok(())
    }

    #[tokio::test]
    async fn test_get_channel_state_at_before_first_state() -> anyhow::Result<()> {
        let db = BlokliDb::new_in_memory().await?;
        let channel_id = create_test_channel(&db).await?;

        insert_channel_state(&db, channel_id, 1000, 5, 2, vec![1; 12], 1).await?;

        let position = BlockPosition {
            block: 999,
            tx_index: 0,
            log_index: 0,
        };

        let result = get_channel_state_at(&db, channel_id, position).await?;
        assert!(result.is_none(), "Should return None for position before first state");

        Ok(())
    }

    #[tokio::test]
    async fn test_get_channel_state_at_between_versions() -> anyhow::Result<()> {
        let db = BlokliDb::new_in_memory().await?;
        let channel_id = create_test_channel(&db).await?;

        let state_id_1 = insert_channel_state(&db, channel_id, 1000, 5, 2, vec![1; 12], 1).await?;
        insert_channel_state(&db, channel_id, 1001, 10, 0, vec![2; 12], 2).await?;

        // Query at a position between the two states
        let position = BlockPosition {
            block: 1001,
            tx_index: 5,
            log_index: 0,
        };

        let result = get_channel_state_at(&db, channel_id, position).await?;
        assert!(result.is_some());
        assert_eq!(result.unwrap().id, state_id_1, "Should return first state");

        Ok(())
    }

    #[tokio::test]
    async fn test_get_channel_state_at_after_all_versions() -> anyhow::Result<()> {
        let db = BlokliDb::new_in_memory().await?;
        let channel_id = create_test_channel(&db).await?;

        insert_channel_state(&db, channel_id, 1000, 5, 2, vec![1; 12], 1).await?;
        let state_id_2 = insert_channel_state(&db, channel_id, 1001, 10, 0, vec![2; 12], 2).await?;

        // Query at a position after all states
        let position = BlockPosition {
            block: 2000,
            tx_index: 0,
            log_index: 0,
        };

        let result = get_channel_state_at(&db, channel_id, position).await?;
        assert!(result.is_some());
        assert_eq!(result.unwrap().id, state_id_2, "Should return latest state");

        Ok(())
    }

    #[tokio::test]
    async fn test_get_channel_state_history_empty() -> anyhow::Result<()> {
        let db = BlokliDb::new_in_memory().await?;
        let channel_id = create_test_channel(&db).await?;

        let from = BlockPosition {
            block: 1000,
            tx_index: 0,
            log_index: 0,
        };
        let to = BlockPosition {
            block: 2000,
            tx_index: 0,
            log_index: 0,
        };

        let result = get_channel_state_history(&db, channel_id, from, to).await?;
        assert_eq!(result.len(), 0, "Should return empty vector");

        Ok(())
    }

    #[tokio::test]
    async fn test_get_channel_state_history_full_range() -> anyhow::Result<()> {
        let db = BlokliDb::new_in_memory().await?;
        let channel_id = create_test_channel(&db).await?;

        // Insert states at different positions
        let state_id_1 = insert_channel_state(&db, channel_id, 1000, 5, 2, vec![1; 12], 1).await?;
        let state_id_2 = insert_channel_state(&db, channel_id, 1001, 10, 0, vec![2; 12], 1).await?;
        let state_id_3 = insert_channel_state(&db, channel_id, 1002, 0, 0, vec![3; 12], 1).await?;

        let from = BlockPosition {
            block: 1000,
            tx_index: 5,
            log_index: 2,
        };
        let to = BlockPosition {
            block: 1002,
            tx_index: 0,
            log_index: 0,
        };

        let result = get_channel_state_history(&db, channel_id, from, to).await?;
        assert_eq!(result.len(), 3, "Should return all three states");
        assert_eq!(result[0].id, state_id_1);
        assert_eq!(result[1].id, state_id_2);
        assert_eq!(result[2].id, state_id_3);

        Ok(())
    }

    #[tokio::test]
    async fn test_get_channel_state_history_partial_overlap() -> anyhow::Result<()> {
        let db = BlokliDb::new_in_memory().await?;
        let channel_id = create_test_channel(&db).await?;

        // Insert states
        insert_channel_state(&db, channel_id, 1000, 5, 2, vec![1; 12], 1).await?;
        let state_id_2 = insert_channel_state(&db, channel_id, 1001, 10, 0, vec![2; 12], 1).await?;
        let state_id_3 = insert_channel_state(&db, channel_id, 1002, 0, 0, vec![3; 12], 1).await?;

        // Query range that excludes first state
        let from = BlockPosition {
            block: 1001,
            tx_index: 0,
            log_index: 0,
        };
        let to = BlockPosition {
            block: 1002,
            tx_index: 5,
            log_index: 0,
        };

        let result = get_channel_state_history(&db, channel_id, from, to).await?;
        assert_eq!(result.len(), 2, "Should return two states");
        assert_eq!(result[0].id, state_id_2);
        assert_eq!(result[1].id, state_id_3);

        Ok(())
    }

    // ==================== Account State Tests ====================

    #[tokio::test]
    async fn test_get_current_account_state_empty_db() -> anyhow::Result<()> {
        let db = BlokliDb::new_in_memory().await?;
        let account_id = create_test_account(&db).await?;

        let result = get_current_account_state(&db, account_id).await?;
        assert!(result.is_none(), "Should return None for account with no state");

        Ok(())
    }

    #[tokio::test]
    async fn test_get_current_account_state_multiple_versions() -> anyhow::Result<()> {
        let db = BlokliDb::new_in_memory().await?;
        let account_id = create_test_account(&db).await?;

        // Insert states at different positions
        insert_account_state(&db, account_id, 1000, 5, 2).await?;
        insert_account_state(&db, account_id, 1000, 10, 0).await?;
        let latest_state_id = insert_account_state(&db, account_id, 1001, 5, 1).await?;

        let result = get_current_account_state(&db, account_id).await?;
        assert!(result.is_some());

        let state = result.unwrap();
        assert_eq!(state.id, latest_state_id, "Should return the latest state");

        Ok(())
    }

    #[tokio::test]
    async fn test_get_account_state_at_exact_position() -> anyhow::Result<()> {
        let db = BlokliDb::new_in_memory().await?;
        let account_id = create_test_account(&db).await?;

        let state_id = insert_account_state(&db, account_id, 1000, 5, 2).await?;

        let position = BlockPosition {
            block: 1000,
            tx_index: 5,
            log_index: 2,
        };

        let result = get_account_state_at(&db, account_id, position).await?;
        assert!(result.is_some());
        assert_eq!(result.unwrap().id, state_id);

        Ok(())
    }

    #[tokio::test]
    async fn test_get_account_state_at_before_first_state() -> anyhow::Result<()> {
        let db = BlokliDb::new_in_memory().await?;
        let account_id = create_test_account(&db).await?;

        insert_account_state(&db, account_id, 1000, 5, 2).await?;

        let position = BlockPosition {
            block: 999,
            tx_index: 0,
            log_index: 0,
        };

        let result = get_account_state_at(&db, account_id, position).await?;
        assert!(result.is_none(), "Should return None for position before first state");

        Ok(())
    }

    #[tokio::test]
    async fn test_get_account_state_history_ordering() -> anyhow::Result<()> {
        let db = BlokliDb::new_in_memory().await?;
        let account_id = create_test_account(&db).await?;

        // Insert states in non-chronological order
        let state_id_3 = insert_account_state(&db, account_id, 1002, 0, 0).await?;
        let state_id_1 = insert_account_state(&db, account_id, 1000, 5, 2).await?;
        let state_id_2 = insert_account_state(&db, account_id, 1001, 10, 0).await?;

        let from = BlockPosition {
            block: 1000,
            tx_index: 5,
            log_index: 2,
        };
        let to = BlockPosition {
            block: 1002,
            tx_index: 0,
            log_index: 0,
        };

        let result = get_account_state_history(&db, account_id, from, to).await?;
        assert_eq!(result.len(), 3);

        // Verify chronological ordering
        assert_eq!(result[0].id, state_id_1, "First state should be oldest");
        assert_eq!(result[1].id, state_id_2, "Second state should be middle");
        assert_eq!(result[2].id, state_id_3, "Third state should be newest");

        Ok(())
    }
}
