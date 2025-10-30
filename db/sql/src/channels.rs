// Allow casts for Solidity uint24/uint48 that fit safely in i64
#![allow(clippy::cast_possible_wrap)]

use std::time::SystemTime;

use async_trait::async_trait;
use blokli_db_entity::channel;
use chrono::{DateTime, Utc};
use futures::stream::BoxStream;
use hopr_crypto_types::types::Hash;
use hopr_internal_types::channels::{ChannelDirection, ChannelEntry, ChannelStatus};
use hopr_primitive_types::{
    prelude::{Address, HoprBalance, U256},
    traits::{IntoEndian, ToHex},
};
use sea_orm::{ActiveModelTrait, ActiveValue::Set, ColumnTrait, EntityTrait, QueryFilter};
use tracing::instrument;

use crate::{
    BlokliDbGeneralModelOperations, OptTx,
    db::BlokliDb,
    errors::{DbSqlError, Result},
    events::{ChannelStateChange, StateChange},
};

/// API for editing [ChannelEntry] in the DB.
pub struct ChannelEditor {
    orig: ChannelEntry,
    #[allow(dead_code)] // TODO(Phase 3): Will be used when channel updates are refactored for state tables
    model: channel::ActiveModel,
    delete: bool,
}

impl ChannelEditor {
    /// Original channel entry **before** the edits.
    pub fn entry(&self) -> &ChannelEntry {
        &self.orig
    }

    /// Change the HOPR balance of the channel.
    /// TODO(Phase 2-3): Update to work with channel_state table
    pub fn change_balance(self, _balance: HoprBalance) -> Self {
        panic!("Channel balance updates must now go through channel_state table - not yet implemented");
    }

    /// Change the channel status.
    /// TODO(Phase 2-3): Update to work with channel_state table
    pub fn change_status(self, _status: ChannelStatus) -> Self {
        panic!("Channel status updates must now go through channel_state table - not yet implemented");
    }

    /// Change the ticket index.
    /// TODO(Phase 2-3): Update to work with channel_state table
    pub fn change_ticket_index(self, _index: impl Into<U256>) -> Self {
        panic!("Channel ticket_index updates must now go through channel_state table - not yet implemented");
    }

    /// Change the channel epoch.
    /// TODO(Phase 2-3): Update to work with channel_state table
    pub fn change_epoch(self, _epoch: impl Into<U256>) -> Self {
        panic!("Channel epoch updates must now go through channel_state table - not yet implemented");
    }

    /// If set, the channel will be deleted, no other edits will be done.
    pub fn delete(mut self) -> Self {
        self.delete = true;
        self
    }
}

/// Helper function to insert a channel state record and emit event
///
/// This creates a new version record in channel_state table and broadcasts the change event.
///
/// # Arguments
///
/// * `db` - Database connection
/// * `tx` - Transaction reference
/// * `channel_id` - Channel ID
/// * `channel_entry` - Channel entry containing state information
/// * `block` - Block number where change occurred
/// * `tx_index` - Transaction index within block
/// * `log_index` - Log index within transaction
///
/// # Returns
///
/// The inserted channel_state record
async fn insert_channel_state_and_emit(
    db: &BlokliDb,
    tx: &sea_orm::DatabaseTransaction,
    channel_id: i32,
    channel_entry: &ChannelEntry,
    block: i64,
    tx_index: i64,
    log_index: i64,
) -> Result<blokli_db_entity::channel_state::Model> {
    use blokli_db_entity::channel_state;

    // Helper function to convert SystemTime to DateTime<Utc>
    fn system_time_to_datetime(time: &SystemTime) -> DateTime<Utc> {
        DateTime::from(*time)
    }

    // Create channel_state record with state fields from ChannelEntry
    let state_model = channel_state::ActiveModel {
        channel_id: Set(channel_id),
        balance: Set(channel_entry.balance.amount().to_be_bytes().to_vec()),
        status: Set(i8::from(channel_entry.status)),
        epoch: Set(channel_entry.channel_epoch.as_u64() as i64),
        ticket_index: Set(channel_entry.ticket_index.as_u64() as i64),
        closure_time: Set(match &channel_entry.status {
            ChannelStatus::PendingToClose(time) => Some(system_time_to_datetime(time)),
            _ => None,
        }),
        corrupted_state: Set(false), // TODO: Handle corrupted state
        published_block: Set(block),
        published_tx_index: Set(tx_index),
        published_log_index: Set(log_index),
        ..Default::default()
    };

    let inserted = state_model.insert(tx).await?;

    // Emit state change event (fire and forget - don't block on event delivery)
    let event = StateChange::ChannelState(ChannelStateChange {
        channel_id,
        state_id: inserted.id,
        published_block: block,
        published_tx_index: tx_index,
        published_log_index: log_index,
    });

    // Spawn event emission as a background task to avoid blocking
    let event_bus = db.event_bus.clone();
    tokio::spawn(async move {
        if let Err(e) = event_bus.publish(event).await {
            tracing::warn!("Failed to publish channel state change event: {}", e);
        }
    });

    Ok(inserted)
}

/// Defines DB API for accessing information about HOPR payment channels.
#[async_trait]
pub trait BlokliDbChannelOperations {
    /// Retrieves channel by its channel ID hash.
    ///
    /// See [generate_channel_id] on how to generate a channel ID hash from source and destination [Addresses](Address).
    async fn get_channel_by_id<'a>(&'a self, tx: OptTx<'a>, id: &Hash) -> Result<Option<ChannelEntry>>;

    /// Start changes to channel entry.
    /// If the channel with the given ID exists, the [ChannelEditor] is returned.
    /// Use [`BlokliDbChannelOperations::finish_channel_update`] to commit edits to the DB when done.
    async fn begin_channel_update<'a>(&'a self, tx: OptTx<'a>, id: &Hash) -> Result<Option<ChannelEditor>>;

    /// Commits changes of the channel to the database.
    /// Returns the updated channel, or on deletion, the deleted channel entry.
    async fn finish_channel_update<'a>(&'a self, tx: OptTx<'a>, editor: ChannelEditor) -> Result<Option<ChannelEntry>>;

    /// Retrieves the channel by source and destination.
    async fn get_channel_by_parties<'a>(
        &'a self,
        tx: OptTx<'a>,
        src: &Address,
        dst: &Address,
    ) -> Result<Option<ChannelEntry>>;

    /// Fetches all channels that are `Incoming` to the given `target`, or `Outgoing` from the given `target`
    async fn get_channels_via<'a>(
        &'a self,
        tx: OptTx<'a>,
        direction: ChannelDirection,
        target: &Address,
    ) -> Result<Vec<ChannelEntry>>;

    /// Retrieves all channels information from the DB.
    async fn get_all_channels<'a>(&'a self, tx: OptTx<'a>) -> Result<Vec<ChannelEntry>>;

    /// Returns a stream of all channels that are `Open` or `PendingToClose` with an active grace period.s
    async fn stream_active_channels<'a>(&'a self) -> Result<BoxStream<'a, Result<ChannelEntry>>>;

    /// Inserts or updates the given channel entry.
    async fn upsert_channel<'a>(&'a self, tx: OptTx<'a>, channel_entry: ChannelEntry) -> Result<()>;
}

#[async_trait]
impl BlokliDbChannelOperations for BlokliDb {
    async fn get_channel_by_id<'a>(&'a self, _tx: OptTx<'a>, _id: &Hash) -> Result<Option<ChannelEntry>> {
        // TODO(Phase 3): Complete implementation to query channel + channel_state and reconstruct ChannelEntry
        // This requires:
        // 1. Query channel identity record
        // 2. Query current channel_state (using get_current_channel_state helper)
        // 3. Lookup source and destination Account records
        // 4. Reconstruct ChannelEntry from combined data
        //
        // For now, returning error to indicate incomplete implementation
        Err(DbSqlError::LogicalError(
            "get_channel_by_id requires full channel_state + account lookup implementation - Phase 3".to_string(),
        ))
    }

    async fn begin_channel_update<'a>(&'a self, _tx: OptTx<'a>, _id: &Hash) -> Result<Option<ChannelEditor>> {
        // TODO(Phase 3): Complete implementation
        // Channel updates need to be refactored to work with state tables
        Err(DbSqlError::LogicalError(
            "begin_channel_update requires refactoring for channel_state table - Phase 3".to_string(),
        ))
    }

    async fn finish_channel_update<'a>(
        &'a self,
        _tx: OptTx<'a>,
        _editor: ChannelEditor,
    ) -> Result<Option<ChannelEntry>> {
        // TODO(Phase 3): Complete implementation
        // Channel updates need to insert new channel_state records and emit events
        Err(DbSqlError::LogicalError(
            "finish_channel_update requires refactoring for channel_state table - Phase 3".to_string(),
        ))
    }

    #[instrument(level = "trace", skip(self, _tx), err)]
    async fn get_channel_by_parties<'a>(
        &'a self,
        _tx: OptTx<'a>,
        _src: &Address,
        _dst: &Address,
    ) -> Result<Option<ChannelEntry>> {
        // TODO(Phase 3): Complete implementation with channel_state lookup
        Err(DbSqlError::LogicalError(
            "get_channel_by_parties requires full channel_state + account lookup implementation - Phase 3".to_string(),
        ))
    }

    async fn get_channels_via<'a>(
        &'a self,
        _tx: OptTx<'a>,
        _direction: ChannelDirection,
        _target: &Address,
    ) -> Result<Vec<ChannelEntry>> {
        // TODO(Phase 3): Complete implementation with channel_state lookup
        Err(DbSqlError::LogicalError(
            "get_channels_via requires full channel_state + account lookup implementation - Phase 3".to_string(),
        ))
    }

    async fn get_all_channels<'a>(&'a self, _tx: OptTx<'a>) -> Result<Vec<ChannelEntry>> {
        // TODO(Phase 3): Complete implementation with channel_state lookup
        Err(DbSqlError::LogicalError(
            "get_all_channels requires full channel_state + account lookup implementation - Phase 3".to_string(),
        ))
    }

    async fn stream_active_channels<'a>(&'a self) -> Result<BoxStream<'a, Result<ChannelEntry>>> {
        // TODO(Phase 2-3): Update to query channel_current view or join with channel_state
        // Status and ClosureTime are now in channel_state table
        // For now, return error as this requires complex query changes
        Err(DbSqlError::LogicalError(
            "stream_active_channels requires channel_state table - use channel_current view in Phase 2-3".to_string(),
        ))
    }

    async fn upsert_channel<'a>(&'a self, tx: OptTx<'a>, channel_entry: ChannelEntry) -> Result<()> {
        // TODO(Phase 3): Add position parameters (block, tx_index, log_index) to this function
        // For now, using placeholder position (0, 0, 0) which will need to be updated
        // when integrating with the indexer that knows the actual blockchain position
        let (block, tx_index, log_index) = (0i64, 0i64, 0i64);

        let channel_id_hex = channel_entry.get_id().to_hex();
        let db_clone = self.clone();

        self.nest_transaction(tx)
            .await?
            .perform(|tx| {
                Box::pin(async move {
                    // Step 1: Insert or update channel identity record
                    let channel_model = channel::ActiveModel {
                        concrete_channel_id: Set(channel_id_hex.clone()),
                        source: Set(0),      // TODO: Need to lookup/create account for source address
                        destination: Set(0), // TODO: Need to lookup/create account for destination address
                        ..Default::default()
                    };

                    // Find existing channel or create new one
                    let channel_id = if let Some(existing_channel) = channel::Entity::find()
                        .filter(channel::Column::ConcreteChannelId.eq(channel_id_hex))
                        .one(tx.as_ref())
                        .await?
                    {
                        existing_channel.id
                    } else {
                        let inserted = channel_model.insert(tx.as_ref()).await?;
                        inserted.id
                    };

                    // Step 2: Insert channel_state record with state information
                    insert_channel_state_and_emit(
                        &db_clone,
                        tx.as_ref(),
                        channel_id,
                        &channel_entry,
                        block,
                        tx_index,
                        log_index,
                    )
                    .await?;

                    Ok::<_, DbSqlError>(())
                })
            })
            .await?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use anyhow::Context;
    use hopr_crypto_random::random_bytes;
    use hopr_crypto_types::{keypairs::ChainKeypair, prelude::Keypair};
    use hopr_internal_types::{
        channels::ChannelStatus,
        prelude::{ChannelDirection, ChannelEntry},
    };
    use hopr_primitive_types::prelude::Address;

    use crate::{BlokliDbGeneralModelOperations, channels::BlokliDbChannelOperations, db::BlokliDb};

    #[tokio::test]
    #[ignore = "TODO(Phase 3): Requires channel_state lookup implementation"]
    async fn test_insert_get_by_id() -> anyhow::Result<()> {
        let db = BlokliDb::new_in_memory().await?;

        let ce = ChannelEntry::new(
            Address::default(),
            Address::default(),
            0.into(),
            0_u32.into(),
            ChannelStatus::Open,
            0_u32.into(),
        );

        db.upsert_channel(None, ce).await?;
        let from_db = db
            .get_channel_by_id(None, &ce.get_id())
            .await?
            .expect("channel must be present");

        assert_eq!(ce, from_db, "channels must be equal");

        Ok(())
    }

    #[tokio::test]
    #[ignore = "TODO(Phase 3): Requires channel_state lookup implementation"]
    async fn test_insert_get_by_parties() -> anyhow::Result<()> {
        let db = BlokliDb::new_in_memory().await?;

        let a = Address::from(random_bytes());
        let b = Address::from(random_bytes());

        let ce = ChannelEntry::new(a, b, 0.into(), 0_u32.into(), ChannelStatus::Open, 0_u32.into());

        db.upsert_channel(None, ce).await?;
        let from_db = db
            .get_channel_by_parties(None, &a, &b)
            .await?
            .context("channel must be present")?;

        assert_eq!(ce, from_db, "channels must be equal");

        Ok(())
    }

    #[tokio::test]
    #[ignore = "TODO(Phase 3): Requires channel_state lookup implementation"]
    async fn test_channel_get_for_destination_that_does_not_exist_returns_none() -> anyhow::Result<()> {
        let db = BlokliDb::new_in_memory().await?;

        let from_db = db
            .get_channels_via(None, ChannelDirection::Incoming, &Address::default())
            .await?
            .first()
            .cloned();

        assert_eq!(None, from_db, "should return None");

        Ok(())
    }

    #[tokio::test]
    #[ignore = "TODO(Phase 3): Requires channel_state lookup implementation"]
    async fn test_channel_get_for_destination_that_exists_should_be_returned() -> anyhow::Result<()> {
        let db = BlokliDb::new_in_memory().await?;

        let expected_destination = Address::default();

        let ce = ChannelEntry::new(
            Address::default(),
            expected_destination,
            0.into(),
            0_u32.into(),
            ChannelStatus::Open,
            0_u32.into(),
        );

        db.upsert_channel(None, ce).await?;
        let from_db = db
            .get_channels_via(None, ChannelDirection::Incoming, &Address::default())
            .await?
            .first()
            .cloned();

        assert_eq!(Some(ce), from_db, "should return a valid channel");

        Ok(())
    }

    #[tokio::test]
    #[ignore = "TODO(Phase 3): Requires channel_state lookup implementation"]
    async fn test_incoming_outgoing_channels() -> anyhow::Result<()> {
        let ckp = ChainKeypair::random();
        let addr_1 = ckp.public().to_address();
        let addr_2 = ChainKeypair::random().public().to_address();

        let db = BlokliDb::new_in_memory().await?;

        let ce_1 = ChannelEntry::new(
            addr_1,
            addr_2,
            0.into(),
            1_u32.into(),
            ChannelStatus::Open,
            0_u32.into(),
        );

        let ce_2 = ChannelEntry::new(
            addr_2,
            addr_1,
            0.into(),
            2_u32.into(),
            ChannelStatus::Open,
            0_u32.into(),
        );

        let db_clone = db.clone();
        db.begin_transaction()
            .await?
            .perform(|tx| {
                Box::pin(async move {
                    db_clone.upsert_channel(Some(tx), ce_1).await?;
                    db_clone.upsert_channel(Some(tx), ce_2).await
                })
            })
            .await?;

        // TODO(Phase 3): Replace with get_channels_via when implemented
        // assert_eq!(vec![ce_2], db.get_channels_via(None, ChannelDirection::Incoming, &addr_1).await?);
        // assert_eq!(vec![ce_1], db.get_channels_via(None, ChannelDirection::Outgoing, &addr_1).await?);

        Ok(())
    }
}
