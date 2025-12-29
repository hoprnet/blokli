//! Temporal state queries for account and channel history
//!
//! This module provides functions for querying blockchain state at any point in history,
//! enabling precise point-in-time queries and complete audit trails for accounts and channels.
//!
//! # Overview
//!
//! The module supports three types of queries:
//! - **Current state queries** - Get the latest state for an object
//! - **Point-in-time queries** - Get state at a specific blockchain position
//! - **Temporal range queries** - Get all state changes within a position range
//!
//! All temporal queries use **position tuples** `(block, tx_index, log_index)` that
//! uniquely identify the blockchain location where each state change occurred.
//!
//! # Position Ordering
//!
//! Positions are ordered lexicographically:
//! ```text
//! (100, 0, 0) < (100, 0, 1) < (100, 1, 0) < (101, 0, 0)
//! ```
//!
//! This means:
//! - Earlier blocks come before later blocks
//! - Within a block, lower transaction indices come first
//! - Within a transaction, lower log indices come first
//!
//! # Blockchain Reorganizations
//!
//! When a blockchain reorg occurs, this module handles **corrective states** that restore
//! objects to their pre-reorg state without deleting historical data. Corrective states:
//! - Are inserted at synthetic positions `(canonical_block, 0, 0)`
//! - Have the `reorg_correction` flag set to `true`
//! - Preserve the complete audit trail by never deleting data
//!
//! Point-in-time queries automatically include corrective states, ensuring queries
//! always reflect the canonical blockchain state at any given position.
//!
//! # Boundary Conditions
//!
//! Temporal range queries use **inclusive boundaries**:
//! - States exactly at the `from` position are included
//! - States exactly at the `to` position are included
//! - Empty ranges (where `from` > `to`) return empty results
//!
//! Point-in-time queries use **at-or-before semantics**:
//! - Returns the most recent state at or before the given position
//! - Returns `None` if no state exists before the position
//! - Positions exactly matching a state change are included
//!
//! # Performance
//!
//! Queries are optimized with database indices on `(published_block, published_tx_index, published_log_index)`:
//! - Current state queries: O(log n) using `ORDER BY` with `LIMIT 1`
//! - Point-in-time queries: O(log n) for position lookup
//! - Range queries: O(k) where k is the number of states in the range
//!
//! Range queries can efficiently handle hundreds of state changes (tested up to 150+).
//!
//! # Examples
//!
//! ## Basic Usage
//!
//! ```rust,ignore
//! use blokli_db::state_queries::*;
//! use blokli_db::events::BlockPosition;
//!
//! // Get current channel state (most recent)
//! let current = get_current_channel_state(&db.conn(TargetDb::Index), channel_id).await?;
//!
//! // Get state at a specific position
//! let position = BlockPosition { block: 1000, tx_index: 5, log_index: 2 };
//! let historical = get_channel_state_at(&db.conn(TargetDb::Index), channel_id, position).await?;
//!
//! // Get all state changes in a range
//! let from = BlockPosition { block: 900, tx_index: 0, log_index: 0 };
//! let to = BlockPosition { block: 1100, tx_index: 0, log_index: 0 };
//! let history = get_channel_state_history(&db.conn(TargetDb::Index), channel_id, from, to).await?;
//! ```
//!
//! ## Handling Reorgs
//!
//! ```rust,ignore
//! // Query at position after a reorg correction
//! let position = BlockPosition { block: 1300, tx_index: 0, log_index: 0 };
//! let state = get_channel_state_at(&db.conn(TargetDb::Index), channel_id, position).await?;
//!
//! // The returned state may be a corrective state if a reorg occurred
//! if let Some(s) = state {
//!     if s.reorg_correction {
//!         // This state was inserted to correct a blockchain reorganization
//!     }
//! }
//! ```
//!
//! ## Querying Multiple Objects
//!
//! ```rust,ignore
//! // Queries are isolated by object ID - account states don't interfere with each other
//! let account1_state = get_account_state_at(&db.conn(TargetDb::Index), account1_id, position).await?;
//! let account2_state = get_account_state_at(&db.conn(TargetDb::Index), account2_id, position).await?;
//! // Results are independent even if positions overlap
//! ```
//!
//! # See Also
//!
//! - [`BlockPosition`](crate::events::BlockPosition) - Position tuple structure
//! - [`handle_reorg`](crate::indexer::handle_reorg) - Reorg handling implementation

use blokli_db_entity::{
    account_state, channel_state,
    prelude::{AccountState, ChannelState},
};
use sea_orm::{ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, QueryOrder, sea_query::Condition};

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
    channel_id: i64,
) -> Result<Option<blokli_db_entity::channel_state::Model>> {
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
    channel_id: i64,
    position: BlockPosition,
) -> Result<Option<blokli_db_entity::channel_state::Model>> {
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
    channel_id: i64,
    from: BlockPosition,
    to: BlockPosition,
) -> Result<Vec<blokli_db_entity::channel_state::Model>> {
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
    account_id: i64,
) -> Result<Option<blokli_db_entity::account_state::Model>> {
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
    account_id: i64,
    position: BlockPosition,
) -> Result<Option<blokli_db_entity::account_state::Model>> {
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
    account_id: i64,
    from: BlockPosition,
    to: BlockPosition,
) -> Result<Vec<blokli_db_entity::account_state::Model>> {
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
    use blokli_db_entity::{account, account_state, channel, channel_state};
    use sea_orm::{ActiveModelTrait, ActiveValue};

    use super::*;
    use crate::{BlokliDbGeneralModelOperations, TargetDb, db::BlokliDb};

    // Helper to create test channel in database
    async fn create_test_channel(db: &DatabaseConnection) -> anyhow::Result<i64> {
        // First create source and destination accounts
        let source_account = account::ActiveModel {
            chain_key: ActiveValue::Set(vec![1; 20]),
            packet_key: ActiveValue::Set("source_packet_key".to_string()),
            published_block: ActiveValue::Set(100),
            published_tx_index: ActiveValue::Set(0),
            published_log_index: ActiveValue::Set(0),
            ..Default::default()
        };
        let source = source_account.insert(db).await?;

        let dest_account = account::ActiveModel {
            chain_key: ActiveValue::Set(vec![2; 20]),
            packet_key: ActiveValue::Set("dest_packet_key".to_string()),
            published_block: ActiveValue::Set(100),
            published_tx_index: ActiveValue::Set(0),
            published_log_index: ActiveValue::Set(0),
            ..Default::default()
        };
        let dest = dest_account.insert(db).await?;

        // Now create the channel
        let channel = channel::ActiveModel {
            concrete_channel_id: ActiveValue::Set(format!("channel_{}", rand::random::<u32>())),
            source: ActiveValue::Set(source.id),
            destination: ActiveValue::Set(dest.id),
            ..Default::default()
        };

        let result = channel.insert(db).await?;
        Ok(result.id)
    }

    // Helper to create test account in database
    async fn create_test_account(db: &DatabaseConnection) -> anyhow::Result<i64> {
        let account = account::ActiveModel {
            chain_key: ActiveValue::Set(vec![
                1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20,
            ]),
            packet_key: ActiveValue::Set(format!("packet_key_{}", rand::random::<u32>())),
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
        channel_id: i64,
        block: i64,
        tx_index: i64,
        log_index: i64,
        balance: Vec<u8>,
        status: i16,
    ) -> anyhow::Result<i64> {
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
        account_id: i64,
        block: i64,
        tx_index: i64,
        log_index: i64,
    ) -> anyhow::Result<i64> {
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
        let channel_id = create_test_channel(&db.conn(TargetDb::Index)).await?;

        let result = get_current_channel_state(&db.conn(TargetDb::Index), channel_id).await?;
        assert!(result.is_none(), "Should return None for channel with no state");

        Ok(())
    }

    #[tokio::test]
    async fn test_get_current_channel_state_single_version() -> anyhow::Result<()> {
        let db = BlokliDb::new_in_memory().await?;
        let channel_id = create_test_channel(&db.conn(TargetDb::Index)).await?;

        let balance = vec![1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
        insert_channel_state(&db.conn(TargetDb::Index), channel_id, 1000, 5, 2, balance.clone(), 1).await?;

        let result = get_current_channel_state(&db.conn(TargetDb::Index), channel_id).await?;
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
        let channel_id = create_test_channel(&db.conn(TargetDb::Index)).await?;

        // Insert states at different positions
        insert_channel_state(&db.conn(TargetDb::Index), channel_id, 1000, 5, 2, vec![1; 12], 1).await?;
        insert_channel_state(&db.conn(TargetDb::Index), channel_id, 1000, 10, 0, vec![2; 12], 1).await?;
        insert_channel_state(&db.conn(TargetDb::Index), channel_id, 1001, 0, 0, vec![3; 12], 1).await?;
        let latest_state_id =
            insert_channel_state(&db.conn(TargetDb::Index), channel_id, 1001, 5, 1, vec![4; 12], 2).await?;

        let result = get_current_channel_state(&db.conn(TargetDb::Index), channel_id).await?;
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
        let channel_id = create_test_channel(&db.conn(TargetDb::Index)).await?;

        let state_id = insert_channel_state(&db.conn(TargetDb::Index), channel_id, 1000, 5, 2, vec![1; 12], 1).await?;

        let position = BlockPosition {
            block: 1000,
            tx_index: 5,
            log_index: 2,
        };

        let result = get_channel_state_at(&db.conn(TargetDb::Index), channel_id, position).await?;
        assert!(result.is_some());
        assert_eq!(result.unwrap().id, state_id);

        Ok(())
    }

    #[tokio::test]
    async fn test_get_channel_state_at_before_first_state() -> anyhow::Result<()> {
        let db = BlokliDb::new_in_memory().await?;
        let channel_id = create_test_channel(&db.conn(TargetDb::Index)).await?;

        insert_channel_state(&db.conn(TargetDb::Index), channel_id, 1000, 5, 2, vec![1; 12], 1).await?;

        let position = BlockPosition {
            block: 999,
            tx_index: 0,
            log_index: 0,
        };

        let result = get_channel_state_at(&db.conn(TargetDb::Index), channel_id, position).await?;
        assert!(result.is_none(), "Should return None for position before first state");

        Ok(())
    }

    #[tokio::test]
    async fn test_get_channel_state_at_between_versions() -> anyhow::Result<()> {
        let db = BlokliDb::new_in_memory().await?;
        let channel_id = create_test_channel(&db.conn(TargetDb::Index)).await?;

        let state_id_1 =
            insert_channel_state(&db.conn(TargetDb::Index), channel_id, 1000, 5, 2, vec![1; 12], 1).await?;
        insert_channel_state(&db.conn(TargetDb::Index), channel_id, 1001, 10, 0, vec![2; 12], 2).await?;

        // Query at a position between the two states
        let position = BlockPosition {
            block: 1001,
            tx_index: 5,
            log_index: 0,
        };

        let result = get_channel_state_at(&db.conn(TargetDb::Index), channel_id, position).await?;
        assert!(result.is_some());
        assert_eq!(result.unwrap().id, state_id_1, "Should return first state");

        Ok(())
    }

    #[tokio::test]
    async fn test_get_channel_state_at_after_all_versions() -> anyhow::Result<()> {
        let db = BlokliDb::new_in_memory().await?;
        let channel_id = create_test_channel(&db.conn(TargetDb::Index)).await?;

        insert_channel_state(&db.conn(TargetDb::Index), channel_id, 1000, 5, 2, vec![1; 12], 1).await?;
        let state_id_2 =
            insert_channel_state(&db.conn(TargetDb::Index), channel_id, 1001, 10, 0, vec![2; 12], 2).await?;

        // Query at a position after all states
        let position = BlockPosition {
            block: 2000,
            tx_index: 0,
            log_index: 0,
        };

        let result = get_channel_state_at(&db.conn(TargetDb::Index), channel_id, position).await?;
        assert!(result.is_some());
        assert_eq!(result.unwrap().id, state_id_2, "Should return latest state");

        Ok(())
    }

    #[tokio::test]
    async fn test_get_channel_state_history_empty() -> anyhow::Result<()> {
        let db = BlokliDb::new_in_memory().await?;
        let channel_id = create_test_channel(&db.conn(TargetDb::Index)).await?;

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

        let result = get_channel_state_history(&db.conn(TargetDb::Index), channel_id, from, to).await?;
        assert_eq!(result.len(), 0, "Should return empty vector");

        Ok(())
    }

    #[tokio::test]
    async fn test_get_channel_state_history_full_range() -> anyhow::Result<()> {
        let db = BlokliDb::new_in_memory().await?;
        let channel_id = create_test_channel(&db.conn(TargetDb::Index)).await?;

        // Insert states at different positions
        let state_id_1 =
            insert_channel_state(&db.conn(TargetDb::Index), channel_id, 1000, 5, 2, vec![1; 12], 1).await?;
        let state_id_2 =
            insert_channel_state(&db.conn(TargetDb::Index), channel_id, 1001, 10, 0, vec![2; 12], 1).await?;
        let state_id_3 =
            insert_channel_state(&db.conn(TargetDb::Index), channel_id, 1002, 0, 0, vec![3; 12], 1).await?;

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

        let result = get_channel_state_history(&db.conn(TargetDb::Index), channel_id, from, to).await?;
        assert_eq!(result.len(), 3, "Should return all three states");
        assert_eq!(result[0].id, state_id_1);
        assert_eq!(result[1].id, state_id_2);
        assert_eq!(result[2].id, state_id_3);

        Ok(())
    }

    #[tokio::test]
    async fn test_get_channel_state_history_partial_overlap() -> anyhow::Result<()> {
        let db = BlokliDb::new_in_memory().await?;
        let channel_id = create_test_channel(&db.conn(TargetDb::Index)).await?;

        // Insert states
        insert_channel_state(&db.conn(TargetDb::Index), channel_id, 1000, 5, 2, vec![1; 12], 1).await?;
        let state_id_2 =
            insert_channel_state(&db.conn(TargetDb::Index), channel_id, 1001, 10, 0, vec![2; 12], 1).await?;
        let state_id_3 =
            insert_channel_state(&db.conn(TargetDb::Index), channel_id, 1002, 0, 0, vec![3; 12], 1).await?;

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

        let result = get_channel_state_history(&db.conn(TargetDb::Index), channel_id, from, to).await?;
        assert_eq!(result.len(), 2, "Should return two states");
        assert_eq!(result[0].id, state_id_2);
        assert_eq!(result[1].id, state_id_3);

        Ok(())
    }

    // ==================== Account State Tests ====================

    #[tokio::test]
    async fn test_get_current_account_state_empty_db() -> anyhow::Result<()> {
        let db = BlokliDb::new_in_memory().await?;
        let account_id = create_test_account(&db.conn(TargetDb::Index)).await?;

        let result = get_current_account_state(&db.conn(TargetDb::Index), account_id).await?;
        assert!(result.is_none(), "Should return None for account with no state");

        Ok(())
    }

    #[tokio::test]
    async fn test_get_current_account_state_multiple_versions() -> anyhow::Result<()> {
        let db = BlokliDb::new_in_memory().await?;
        let account_id = create_test_account(&db.conn(TargetDb::Index)).await?;

        // Insert states at different positions
        insert_account_state(&db.conn(TargetDb::Index), account_id, 1000, 5, 2).await?;
        insert_account_state(&db.conn(TargetDb::Index), account_id, 1000, 10, 0).await?;
        let latest_state_id = insert_account_state(&db.conn(TargetDb::Index), account_id, 1001, 5, 1).await?;

        let result = get_current_account_state(&db.conn(TargetDb::Index), account_id).await?;
        assert!(result.is_some());

        let state = result.unwrap();
        assert_eq!(state.id, latest_state_id, "Should return the latest state");

        Ok(())
    }

    #[tokio::test]
    async fn test_get_account_state_at_exact_position() -> anyhow::Result<()> {
        let db = BlokliDb::new_in_memory().await?;
        let account_id = create_test_account(&db.conn(TargetDb::Index)).await?;

        let state_id = insert_account_state(&db.conn(TargetDb::Index), account_id, 1000, 5, 2).await?;

        let position = BlockPosition {
            block: 1000,
            tx_index: 5,
            log_index: 2,
        };

        let result = get_account_state_at(&db.conn(TargetDb::Index), account_id, position).await?;
        assert!(result.is_some());
        assert_eq!(result.unwrap().id, state_id);

        Ok(())
    }

    #[tokio::test]
    async fn test_get_account_state_at_before_first_state() -> anyhow::Result<()> {
        let db = BlokliDb::new_in_memory().await?;
        let account_id = create_test_account(&db.conn(TargetDb::Index)).await?;

        insert_account_state(&db.conn(TargetDb::Index), account_id, 1000, 5, 2).await?;

        let position = BlockPosition {
            block: 999,
            tx_index: 0,
            log_index: 0,
        };

        let result = get_account_state_at(&db.conn(TargetDb::Index), account_id, position).await?;
        assert!(result.is_none(), "Should return None for position before first state");

        Ok(())
    }

    #[tokio::test]
    async fn test_get_account_state_history_ordering() -> anyhow::Result<()> {
        let db = BlokliDb::new_in_memory().await?;
        let account_id = create_test_account(&db.conn(TargetDb::Index)).await?;

        // Insert states in non-chronological order
        let state_id_3 = insert_account_state(&db.conn(TargetDb::Index), account_id, 1002, 0, 0).await?;
        let state_id_1 = insert_account_state(&db.conn(TargetDb::Index), account_id, 1000, 5, 2).await?;
        let state_id_2 = insert_account_state(&db.conn(TargetDb::Index), account_id, 1001, 10, 0).await?;

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

        let result = get_account_state_history(&db.conn(TargetDb::Index), account_id, from, to).await?;
        assert_eq!(result.len(), 3);

        // Verify chronological ordering
        assert_eq!(result[0].id, state_id_1, "First state should be oldest");
        assert_eq!(result[1].id, state_id_2, "Second state should be middle");
        assert_eq!(result[2].id, state_id_3, "Third state should be newest");

        Ok(())
    }

    // ==================== Edge Case Tests ====================

    #[tokio::test]
    async fn test_get_channel_state_at_same_block_different_positions() -> anyhow::Result<()> {
        // Test correct position ordering within same block
        let db = BlokliDb::new_in_memory().await?;
        let channel_id = create_test_channel(&db.conn(TargetDb::Index)).await?;

        // Insert multiple states at same block with different (tx_index, log_index)
        let _state_id_1 =
            insert_channel_state(&db.conn(TargetDb::Index), channel_id, 1000, 5, 2, vec![1; 12], 1).await?;
        let state_id_2 =
            insert_channel_state(&db.conn(TargetDb::Index), channel_id, 1000, 5, 5, vec![2; 12], 1).await?;
        let state_id_3 =
            insert_channel_state(&db.conn(TargetDb::Index), channel_id, 1000, 10, 0, vec![3; 12], 1).await?;
        let _state_id_4 =
            insert_channel_state(&db.conn(TargetDb::Index), channel_id, 1000, 10, 3, vec![4; 12], 1).await?;

        // Query at position (1000, 5, 6) - should return state_id_2 at (1000, 5, 5)
        let position = BlockPosition {
            block: 1000,
            tx_index: 5,
            log_index: 6,
        };
        let result = get_channel_state_at(&db.conn(TargetDb::Index), channel_id, position).await?;
        assert!(result.is_some());
        assert_eq!(result.unwrap().id, state_id_2, "Should return state at (1000, 5, 5)");

        // Query at position (1000, 8, 0) - should still return state_id_2
        let position = BlockPosition {
            block: 1000,
            tx_index: 8,
            log_index: 0,
        };
        let result = get_channel_state_at(&db.conn(TargetDb::Index), channel_id, position).await?;
        assert!(result.is_some());
        assert_eq!(
            result.unwrap().id,
            state_id_2,
            "Should return state_id_2 (latest before position)"
        );

        // Query at position (1000, 10, 2) - should return state_id_3
        let position = BlockPosition {
            block: 1000,
            tx_index: 10,
            log_index: 2,
        };
        let result = get_channel_state_at(&db.conn(TargetDb::Index), channel_id, position).await?;
        assert!(result.is_some());
        assert_eq!(result.unwrap().id, state_id_3, "Should return state at (1000, 10, 0)");

        Ok(())
    }

    #[tokio::test]
    async fn test_get_channel_state_history_boundary_conditions() -> anyhow::Result<()> {
        // Test inclusive/exclusive boundary logic for history queries
        let db = BlokliDb::new_in_memory().await?;
        let channel_id = create_test_channel(&db.conn(TargetDb::Index)).await?;

        // Insert states at precise positions
        let state_id_1 =
            insert_channel_state(&db.conn(TargetDb::Index), channel_id, 1000, 5, 2, vec![1; 12], 1).await?;
        let state_id_2 =
            insert_channel_state(&db.conn(TargetDb::Index), channel_id, 1000, 10, 0, vec![2; 12], 1).await?;
        let state_id_3 =
            insert_channel_state(&db.conn(TargetDb::Index), channel_id, 1001, 0, 0, vec![3; 12], 1).await?;
        let state_id_4 =
            insert_channel_state(&db.conn(TargetDb::Index), channel_id, 1001, 5, 1, vec![4; 12], 1).await?;

        // Test exact boundary inclusion - from is inclusive
        let from = BlockPosition {
            block: 1000,
            tx_index: 5,
            log_index: 2,
        };
        let to = BlockPosition {
            block: 1001,
            tx_index: 5,
            log_index: 1,
        };
        let result = get_channel_state_history(&db.conn(TargetDb::Index), channel_id, from, to).await?;
        assert_eq!(result.len(), 4, "Should include all four states");
        assert_eq!(result[0].id, state_id_1, "Should include from boundary");
        assert_eq!(result[3].id, state_id_4, "Should include to boundary");

        // Test just before boundary
        let from = BlockPosition {
            block: 1000,
            tx_index: 5,
            log_index: 3,
        };
        let to = BlockPosition {
            block: 1001,
            tx_index: 5,
            log_index: 0,
        };
        let result = get_channel_state_history(&db.conn(TargetDb::Index), channel_id, from, to).await?;
        assert_eq!(result.len(), 2, "Should exclude state_id_1 and state_id_4");
        assert_eq!(result[0].id, state_id_2);
        assert_eq!(result[1].id, state_id_3);

        Ok(())
    }

    #[tokio::test]
    async fn test_get_channel_state_with_reorg_corrections() -> anyhow::Result<()> {
        // Test querying with reorg_correction states present
        let db = BlokliDb::new_in_memory().await?;
        let channel_id = create_test_channel(&db.conn(TargetDb::Index)).await?;

        // Insert normal state before reorg
        let _state_id_1 =
            insert_channel_state(&db.conn(TargetDb::Index), channel_id, 1000, 5, 2, vec![1; 12], 1).await?;

        // Insert states that will be "reverted" by reorg
        insert_channel_state(&db.conn(TargetDb::Index), channel_id, 1100, 10, 0, vec![2; 12], 2).await?;
        insert_channel_state(&db.conn(TargetDb::Index), channel_id, 1200, 0, 0, vec![3; 12], 2).await?;

        // Insert reorg correction state at synthetic position (1300, 0, 0)
        let correction_state = channel_state::ActiveModel {
            channel_id: ActiveValue::Set(channel_id),
            balance: ActiveValue::Set(vec![1; 12]),
            status: ActiveValue::Set(1),
            epoch: ActiveValue::Set(0),
            ticket_index: ActiveValue::Set(0),
            closure_time: ActiveValue::Set(None),
            corrupted_state: ActiveValue::Set(false),
            published_block: ActiveValue::Set(1300),
            published_tx_index: ActiveValue::Set(0),
            published_log_index: ActiveValue::Set(0),
            reorg_correction: ActiveValue::Set(true),
            ..Default::default()
        };
        let correction_id = correction_state.insert(db.conn(TargetDb::Index)).await?.id;

        // Query at position after reorg - should find correction state
        let position = BlockPosition {
            block: 1300,
            tx_index: 0,
            log_index: 0,
        };
        let result = get_channel_state_at(&db.conn(TargetDb::Index), channel_id, position).await?;
        assert!(result.is_some());
        let state = result.unwrap();
        assert_eq!(state.id, correction_id, "Should return reorg correction state");
        assert!(state.reorg_correction, "Should be marked as reorg correction");
        assert_eq!(state.balance, vec![1; 12], "Should have reverted balance");

        // Query history including reorg should include all states
        let from = BlockPosition {
            block: 1000,
            tx_index: 0,
            log_index: 0,
        };
        let to = BlockPosition {
            block: 1300,
            tx_index: 0,
            log_index: 0,
        };
        let history = get_channel_state_history(&db.conn(TargetDb::Index), channel_id, from, to).await?;
        assert_eq!(history.len(), 4, "Should include all states including correction");

        Ok(())
    }

    #[tokio::test]
    async fn test_get_channel_state_history_large_range() -> anyhow::Result<()> {
        // Test performance with many state changes (100+)
        let db = BlokliDb::new_in_memory().await?;
        let channel_id = create_test_channel(&db.conn(TargetDb::Index)).await?;

        // Insert 150 state changes across 150 blocks
        for block in 1000..1150 {
            let balance = vec![(block % 256) as u8; 12];
            insert_channel_state(&db.conn(TargetDb::Index), channel_id, block, 0, 0, balance, 1).await?;
        }

        // Query full range
        let from = BlockPosition {
            block: 1000,
            tx_index: 0,
            log_index: 0,
        };
        let to = BlockPosition {
            block: 1149,
            tx_index: 0,
            log_index: 0,
        };
        let history = get_channel_state_history(&db.conn(TargetDb::Index), channel_id, from, to).await?;
        assert_eq!(history.len(), 150, "Should return all 150 states");

        // Verify ordering is maintained
        for i in 0..149 {
            assert!(
                history[i].published_block <= history[i + 1].published_block,
                "States should be in chronological order"
            );
        }

        // Query at specific position in middle should return correct state
        let position = BlockPosition {
            block: 1075,
            tx_index: 0,
            log_index: 0,
        };
        let result = get_channel_state_at(&db.conn(TargetDb::Index), channel_id, position).await?;
        assert!(result.is_some());
        let state = result.unwrap();
        assert_eq!(state.published_block, 1075);
        assert_eq!(state.balance, vec![(1075 % 256) as u8; 12]);

        Ok(())
    }

    #[tokio::test]
    async fn test_get_channel_state_at_synthetic_reorg_position() -> anyhow::Result<()> {
        // Test querying at (block, 0, 0) synthetic reorg correction positions
        let db = BlokliDb::new_in_memory().await?;
        let channel_id = create_test_channel(&db.conn(TargetDb::Index)).await?;

        // Insert state before synthetic position
        insert_channel_state(&db.conn(TargetDb::Index), channel_id, 999, 10, 5, vec![1; 12], 1).await?;

        // Insert synthetic reorg correction at (1000, 0, 0)
        let correction_state = channel_state::ActiveModel {
            channel_id: ActiveValue::Set(channel_id),
            balance: ActiveValue::Set(vec![2; 12]),
            status: ActiveValue::Set(1),
            epoch: ActiveValue::Set(0),
            ticket_index: ActiveValue::Set(0),
            closure_time: ActiveValue::Set(None),
            corrupted_state: ActiveValue::Set(false),
            published_block: ActiveValue::Set(1000),
            published_tx_index: ActiveValue::Set(0),
            published_log_index: ActiveValue::Set(0),
            reorg_correction: ActiveValue::Set(true),
            ..Default::default()
        };
        let correction_id = correction_state.insert(db.conn(TargetDb::Index)).await?.id;

        // Insert normal state after synthetic position
        insert_channel_state(&db.conn(TargetDb::Index), channel_id, 1000, 5, 3, vec![3; 12], 2).await?;

        // Query exactly at synthetic position (1000, 0, 0)
        let position = BlockPosition {
            block: 1000,
            tx_index: 0,
            log_index: 0,
        };
        let result = get_channel_state_at(&db.conn(TargetDb::Index), channel_id, position).await?;
        assert!(result.is_some());
        assert_eq!(result.unwrap().id, correction_id, "Should find synthetic state");

        // Query at position (1000, 0, 1) - should still return synthetic state
        let position = BlockPosition {
            block: 1000,
            tx_index: 0,
            log_index: 1,
        };
        let result = get_channel_state_at(&db.conn(TargetDb::Index), channel_id, position).await?;
        assert!(result.is_some());
        assert_eq!(result.unwrap().id, correction_id, "Should find synthetic state");

        // Query at position (1000, 3, 0) - should still return synthetic state
        let position = BlockPosition {
            block: 1000,
            tx_index: 3,
            log_index: 0,
        };
        let result = get_channel_state_at(&db.conn(TargetDb::Index), channel_id, position).await?;
        assert!(result.is_some());
        assert_eq!(result.unwrap().id, correction_id);

        Ok(())
    }

    #[tokio::test]
    async fn test_get_account_state_isolation() -> anyhow::Result<()> {
        // Test that multiple accounts with interleaved positions don't interfere
        let db = BlokliDb::new_in_memory().await?;
        let account_id_1 = create_test_account(&db.conn(TargetDb::Index)).await?;
        let account_id_2 = create_test_account(&db.conn(TargetDb::Index)).await?;

        // Insert states for both accounts with interleaved positions
        let _state_1_a = insert_account_state(&db.conn(TargetDb::Index), account_id_1, 1000, 5, 2).await?;
        let state_2_a = insert_account_state(&db.conn(TargetDb::Index), account_id_2, 1000, 7, 0).await?;
        let state_1_b = insert_account_state(&db.conn(TargetDb::Index), account_id_1, 1001, 3, 1).await?;
        let state_2_b = insert_account_state(&db.conn(TargetDb::Index), account_id_2, 1001, 4, 2).await?;
        let state_1_c = insert_account_state(&db.conn(TargetDb::Index), account_id_1, 1002, 0, 0).await?;

        // Query account 1 at position between its states
        let position = BlockPosition {
            block: 1001,
            tx_index: 4,
            log_index: 0,
        };
        let result = get_account_state_at(&db.conn(TargetDb::Index), account_id_1, position).await?;
        assert!(result.is_some());
        assert_eq!(
            result.unwrap().id,
            state_1_b,
            "Should only see account 1's state, not account 2's"
        );

        // Query account 2 history - should not include account 1's states
        let from = BlockPosition {
            block: 1000,
            tx_index: 7,
            log_index: 0,
        };
        let to = BlockPosition {
            block: 1001,
            tx_index: 4,
            log_index: 2,
        };
        let history = get_account_state_history(&db.conn(TargetDb::Index), account_id_2, from, to).await?;
        assert_eq!(history.len(), 2, "Should only include account 2's states");
        assert_eq!(history[0].id, state_2_a);
        assert_eq!(history[1].id, state_2_b);

        // Verify current state queries are isolated
        let current_1 = get_current_account_state(&db.conn(TargetDb::Index), account_id_1).await?;
        let current_2 = get_current_account_state(&db.conn(TargetDb::Index), account_id_2).await?;
        assert!(current_1.is_some());
        assert!(current_2.is_some());
        assert_eq!(current_1.unwrap().id, state_1_c);
        assert_eq!(current_2.unwrap().id, state_2_b);

        Ok(())
    }
}
