// Allow casts for Solidity uint24/uint48 that fit safely in i64
#![allow(clippy::cast_possible_wrap)]
// Allow casts from i64 back to u64 for values that originated from Solidity uints
#![allow(clippy::cast_sign_loss)]

use std::time::SystemTime;

use async_trait::async_trait;
use blokli_db_entity::{
    account, channel, channel_state,
    prelude::{Account, Channel, ChannelState},
};
use chrono::{DateTime, Utc};
use futures::stream::BoxStream;
use hopr_crypto_types::types::Hash;
use hopr_internal_types::channels::{ChannelDirection, ChannelEntry, ChannelStatus};
use hopr_primitive_types::{
    prelude::{Address, HoprBalance, U256},
    traits::{IntoEndian, ToHex},
};
use sea_orm::{ActiveModelTrait, ActiveValue::Set, ColumnTrait, EntityTrait, QueryFilter, QueryOrder};
use tracing::instrument;

use crate::{
    BlokliDbGeneralModelOperations, OptTx,
    db::BlokliDb,
    errors::{DbSqlError, Result},
    events::{ChannelStateChange, StateChange},
};

/// Helper function to find or create an account record and return its database ID
///
/// # Arguments
///
/// * `tx` - Database transaction
/// * `address` - Chain address (20 bytes)
///
/// # Returns
///
/// The account's database ID (primary key)
async fn get_or_create_account_id(tx: &sea_orm::DatabaseTransaction, address: &Address) -> Result<i64> {
    // Try to find existing account
    if let Some(existing) = Account::find()
        .filter(account::Column::ChainKey.eq(address.as_ref().to_vec()))
        .one(tx)
        .await?
    {
        return Ok(existing.id);
    }

    // Account doesn't exist - this shouldn't happen in normal operation
    // as accounts should be created when processing account-related events.
    // For now, return an error indicating the account needs to be created first.
    Err(DbSqlError::LogicalError(format!(
        "Account with address {} not found - accounts must be created via account events before channel operations",
        address.to_hex()
    )))
}

/// Helper function to lookup account addresses by their database IDs
///
/// # Arguments
///
/// * `tx` - Database transaction
/// * `source_id` - Source account database ID
/// * `dest_id` - Destination account database ID
///
/// # Returns
///
/// Tuple of (source_address, dest_address)
async fn lookup_account_addresses(
    tx: &sea_orm::DatabaseTransaction,
    source_id: i64,
    dest_id: i64,
) -> Result<(Address, Address)> {
    let source_account = Account::find_by_id(source_id)
        .one(tx)
        .await?
        .ok_or_else(|| DbSqlError::LogicalError(format!("Source account with ID {} not found", source_id)))?;

    let dest_account = Account::find_by_id(dest_id)
        .one(tx)
        .await?
        .ok_or_else(|| DbSqlError::LogicalError(format!("Destination account with ID {} not found", dest_id)))?;

    let source_addr = Address::try_from(source_account.chain_key.as_slice())?;
    let dest_addr = Address::try_from(dest_account.chain_key.as_slice())?;

    Ok((source_addr, dest_addr))
}

/// Helper function to reconstruct a ChannelEntry from channel and channel_state records
///
/// # Arguments
///
/// * `tx` - Database transaction
/// * `_channel` - Channel identity record (not currently used, but kept for API consistency)
/// * `state` - Channel state record
/// * `source_addr` - Source account address
/// * `dest_addr` - Destination account address
///
/// # Returns
///
/// Reconstructed ChannelEntry
fn reconstruct_channel_entry(
    _channel: &blokli_db_entity::channel::Model,
    state: &blokli_db_entity::channel_state::Model,
    source_addr: Address,
    dest_addr: Address,
) -> Result<ChannelEntry> {
    // Convert balance from Vec<u8> (12 bytes) to HoprBalance
    let balance_bytes: [u8; 12] = state
        .balance
        .as_slice()
        .try_into()
        .map_err(|_| DbSqlError::DecodingError)?;
    let balance = HoprBalance::from_be_bytes(balance_bytes);

    // Convert status from i8 to ChannelStatus
    let status = match state.status {
        0 => ChannelStatus::Closed,
        1 => ChannelStatus::Open,
        2 => {
            // PendingToClose with closure_time
            let closure_time = state
                .closure_time
                .ok_or_else(|| DbSqlError::LogicalError("PendingToClose status requires closure_time".to_string()))?;
            ChannelStatus::PendingToClose(closure_time.into())
        }
        _ => return Err(DbSqlError::DecodingError),
    };

    Ok(ChannelEntry::new(
        source_addr,
        dest_addr,
        balance,
        U256::from(state.ticket_index as u64),
        status,
        U256::from(state.epoch as u64),
    ))
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
    channel_id: i64,
    channel_entry: &ChannelEntry,
    block: i64,
    tx_index: i64,
    log_index: i64,
) -> Result<blokli_db_entity::channel_state::Model> {
    // Helper function to convert SystemTime to DateTime<Utc>
    fn system_time_to_datetime(time: &SystemTime) -> DateTime<Utc> {
        DateTime::from(*time)
    }

    // Create channel_state record with state fields from ChannelEntry
    // HoprBalance.to_be_bytes() returns 32 bytes (U256), but we only need the last 12 bytes
    // for database storage (balances fit in 96 bits)
    let balance_bytes_32 = channel_entry.balance.to_be_bytes();
    let balance_bytes_12: [u8; 12] = balance_bytes_32[20..32]
        .try_into()
        .expect("slice should be exactly 12 bytes");

    let state_model = channel_state::ActiveModel {
        channel_id: Set(channel_id),
        balance: Set(balance_bytes_12.to_vec()),
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

    tracing::debug!("About to insert channel_state");
    let inserted = state_model.insert(tx).await?;
    tracing::debug!("Successfully inserted channel_state with id: {}", inserted.id);

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
    /// Retrieves channel by its channel ID hash (returns latest state).
    ///
    /// See [generate_channel_id] on how to generate a channel ID hash from source and destination [Addresses](Address).
    async fn get_channel_by_id<'a>(&'a self, tx: OptTx<'a>, id: &Hash) -> Result<Option<ChannelEntry>>;

    /// Retrieves the channel by source and destination (returns latest state).
    async fn get_channel_by_parties<'a>(
        &'a self,
        tx: OptTx<'a>,
        src: &Address,
        dst: &Address,
    ) -> Result<Option<ChannelEntry>>;

    /// Fetches all channels that are `Incoming` to the given `target`, or `Outgoing` from the given `target`
    /// (returns latest state for each channel).
    async fn get_channels_via<'a>(
        &'a self,
        tx: OptTx<'a>,
        direction: ChannelDirection,
        target: &Address,
    ) -> Result<Vec<ChannelEntry>>;

    /// Retrieves all channels information from the DB (returns latest state for each channel).
    async fn get_all_channels<'a>(&'a self, tx: OptTx<'a>) -> Result<Vec<ChannelEntry>>;

    /// Returns a stream of all channels that are `Open` or `PendingToClose` with an active grace period.
    ///
    /// **DEPRECATED**: This function needs refactoring for channel_state table.
    async fn stream_active_channels<'a>(&'a self) -> Result<BoxStream<'a, Result<ChannelEntry>>>;

    /// Inserts or updates the given channel entry atomically.
    ///
    /// If the channel doesn't exist, creates the channel identity and initial state.
    /// If the channel exists, adds a new state record with the given temporal coordinates.
    ///
    /// # Arguments
    ///
    /// * `tx` - Optional database transaction
    /// * `channel_entry` - Channel entry containing state information
    /// * `block` - Block number where this state was published
    /// * `tx_index` - Transaction index within the block
    /// * `log_index` - Log index within the transaction
    ///
    /// # Temporal Database Semantics
    ///
    /// This function follows temporal database principles:
    /// - Channel identity is created once and never modified
    /// - Each state change creates a new immutable record in channel_state table
    /// - State records are identified by (channel_id, block, tx_index, log_index)
    /// - Historical states are preserved and can be queried
    async fn upsert_channel<'a>(
        &'a self,
        tx: OptTx<'a>,
        channel_entry: ChannelEntry,
        block: u32,
        tx_index: u32,
        log_index: u32,
    ) -> Result<()>;

    /// Retrieves channel state at a specific block height.
    ///
    /// Returns the most recent state that was published at or before the given block.
    /// If no state exists at or before the block, returns None.
    ///
    /// # Arguments
    ///
    /// * `tx` - Optional database transaction
    /// * `id` - Channel ID hash
    /// * `block` - Block number to query state at
    async fn get_channel_state_at_block<'a>(
        &'a self,
        tx: OptTx<'a>,
        id: &Hash,
        block: u32,
    ) -> Result<Option<ChannelEntry>>;

    /// Retrieves complete state history for a channel.
    ///
    /// Returns all state records for the channel ordered chronologically by
    /// (block, tx_index, log_index) from earliest to latest.
    ///
    /// # Arguments
    ///
    /// * `tx` - Optional database transaction
    /// * `id` - Channel ID hash
    async fn get_channel_history<'a>(&'a self, tx: OptTx<'a>, id: &Hash) -> Result<Vec<ChannelEntry>>;

    /// Marks a specific channel state as corrupted.
    ///
    /// Updates the corrupted_state flag for the state record identified by
    /// the channel ID and temporal coordinates.
    ///
    /// # Arguments
    ///
    /// * `tx` - Optional database transaction
    /// * `id` - Channel ID hash
    /// * `block` - Block number of the state to mark
    /// * `tx_index` - Transaction index of the state to mark
    /// * `log_index` - Log index of the state to mark
    async fn mark_channel_state_corrupted<'a>(
        &'a self,
        tx: OptTx<'a>,
        id: &Hash,
        block: u32,
        tx_index: u32,
        log_index: u32,
    ) -> Result<()>;
}

#[async_trait]
impl BlokliDbChannelOperations for BlokliDb {
    async fn get_channel_by_id<'a>(&'a self, tx: OptTx<'a>, id: &Hash) -> Result<Option<ChannelEntry>> {
        let id_hex = id.to_hex();
        self.nest_transaction(tx)
            .await?
            .perform(|tx| {
                Box::pin(async move {
                    // Step 1: Find channel by concrete_channel_id
                    let channel_model = match Channel::find()
                        .filter(channel::Column::ConcreteChannelId.eq(id_hex))
                        .one(tx.as_ref())
                        .await?
                    {
                        Some(c) => c,
                        None => return Ok(None),
                    };

                    // Step 2: Get the latest channel_state for this channel
                    let state_model = ChannelState::find()
                        .filter(channel_state::Column::ChannelId.eq(channel_model.id))
                        .order_by_desc(channel_state::Column::PublishedBlock)
                        .order_by_desc(channel_state::Column::PublishedTxIndex)
                        .order_by_desc(channel_state::Column::PublishedLogIndex)
                        .one(tx.as_ref())
                        .await?
                        .ok_or_else(|| {
                            DbSqlError::LogicalError(format!(
                                "Channel {} exists but has no state records",
                                channel_model.id
                            ))
                        })?;

                    // Step 3: Lookup account addresses
                    let (source_addr, dest_addr) =
                        lookup_account_addresses(tx.as_ref(), channel_model.source, channel_model.destination).await?;

                    // Step 4: Reconstruct ChannelEntry
                    let entry = reconstruct_channel_entry(&channel_model, &state_model, source_addr, dest_addr)?;

                    Ok::<_, DbSqlError>(Some(entry))
                })
            })
            .await
    }

    #[instrument(level = "trace", skip(self, tx), err)]
    async fn get_channel_by_parties<'a>(
        &'a self,
        tx: OptTx<'a>,
        src: &Address,
        dst: &Address,
    ) -> Result<Option<ChannelEntry>> {
        let src_clone = *src;
        let dst_clone = *dst;

        self.nest_transaction(tx)
            .await?
            .perform(|tx| {
                Box::pin(async move {
                    // Step 1: Lookup account IDs for source and destination
                    let source_account = Account::find()
                        .filter(account::Column::ChainKey.eq(src_clone.as_ref().to_vec()))
                        .one(tx.as_ref())
                        .await?;

                    let dest_account = Account::find()
                        .filter(account::Column::ChainKey.eq(dst_clone.as_ref().to_vec()))
                        .one(tx.as_ref())
                        .await?;

                    // If either account doesn't exist, channel doesn't exist
                    let (source_id, dest_id) = match (source_account, dest_account) {
                        (Some(s), Some(d)) => (s.id, d.id),
                        _ => return Ok(None),
                    };

                    // Step 2: Find channel by source and destination IDs
                    let channel_model = match Channel::find()
                        .filter(channel::Column::Source.eq(source_id))
                        .filter(channel::Column::Destination.eq(dest_id))
                        .one(tx.as_ref())
                        .await?
                    {
                        Some(c) => c,
                        None => return Ok(None),
                    };

                    // Step 3: Get the latest channel_state
                    let state_model = ChannelState::find()
                        .filter(channel_state::Column::ChannelId.eq(channel_model.id))
                        .order_by_desc(channel_state::Column::PublishedBlock)
                        .order_by_desc(channel_state::Column::PublishedTxIndex)
                        .order_by_desc(channel_state::Column::PublishedLogIndex)
                        .one(tx.as_ref())
                        .await?
                        .ok_or_else(|| {
                            DbSqlError::LogicalError(format!(
                                "Channel {} exists but has no state records",
                                channel_model.id
                            ))
                        })?;

                    // Step 4: Reconstruct ChannelEntry
                    let entry = reconstruct_channel_entry(&channel_model, &state_model, src_clone, dst_clone)?;

                    Ok::<_, DbSqlError>(Some(entry))
                })
            })
            .await
    }

    async fn get_channels_via<'a>(
        &'a self,
        tx: OptTx<'a>,
        direction: ChannelDirection,
        target: &Address,
    ) -> Result<Vec<ChannelEntry>> {
        let target_clone = *target;

        self.nest_transaction(tx)
            .await?
            .perform(|tx| {
                Box::pin(async move {
                    // Step 1: Lookup target account ID
                    let target_account = Account::find()
                        .filter(account::Column::ChainKey.eq(target_clone.as_ref().to_vec()))
                        .one(tx.as_ref())
                        .await?;

                    let target_id = match target_account {
                        Some(a) => a.id,
                        None => return Ok(vec![]), // No account means no channels
                    };

                    // Step 2: Find channels based on direction
                    let channels = match direction {
                        ChannelDirection::Incoming => {
                            // Channels where target is the destination
                            Channel::find()
                                .filter(channel::Column::Destination.eq(target_id))
                                .all(tx.as_ref())
                                .await?
                        }
                        ChannelDirection::Outgoing => {
                            // Channels where target is the source
                            Channel::find()
                                .filter(channel::Column::Source.eq(target_id))
                                .all(tx.as_ref())
                                .await?
                        }
                    };

                    // Step 3: For each channel, get latest state and reconstruct ChannelEntry
                    let mut results = Vec::new();
                    for channel_model in channels {
                        // Get latest channel_state
                        let state_model = ChannelState::find()
                            .filter(channel_state::Column::ChannelId.eq(channel_model.id))
                            .order_by_desc(channel_state::Column::PublishedBlock)
                            .order_by_desc(channel_state::Column::PublishedTxIndex)
                            .order_by_desc(channel_state::Column::PublishedLogIndex)
                            .one(tx.as_ref())
                            .await?;

                        if let Some(state) = state_model {
                            // Lookup account addresses
                            let (source_addr, dest_addr) =
                                lookup_account_addresses(tx.as_ref(), channel_model.source, channel_model.destination)
                                    .await?;

                            // Reconstruct ChannelEntry
                            let entry = reconstruct_channel_entry(&channel_model, &state, source_addr, dest_addr)?;
                            results.push(entry);
                        }
                    }

                    Ok::<_, DbSqlError>(results)
                })
            })
            .await
    }

    async fn get_all_channels<'a>(&'a self, tx: OptTx<'a>) -> Result<Vec<ChannelEntry>> {
        self.nest_transaction(tx)
            .await?
            .perform(|tx| {
                Box::pin(async move {
                    use blokli_db_entity::{
                        channel_state,
                        prelude::{Channel, ChannelState},
                    };
                    use sea_orm::QueryOrder;

                    // Step 1: Get all channels
                    let channels = Channel::find().all(tx.as_ref()).await?;

                    // Step 2: For each channel, get latest state and reconstruct ChannelEntry
                    let mut results = Vec::new();
                    for channel_model in channels {
                        // Get latest channel_state
                        let state_model = ChannelState::find()
                            .filter(channel_state::Column::ChannelId.eq(channel_model.id))
                            .order_by_desc(channel_state::Column::PublishedBlock)
                            .order_by_desc(channel_state::Column::PublishedTxIndex)
                            .order_by_desc(channel_state::Column::PublishedLogIndex)
                            .one(tx.as_ref())
                            .await?;

                        if let Some(state) = state_model {
                            // Lookup account addresses
                            let (source_addr, dest_addr) =
                                lookup_account_addresses(tx.as_ref(), channel_model.source, channel_model.destination)
                                    .await?;

                            // Reconstruct ChannelEntry
                            let entry = reconstruct_channel_entry(&channel_model, &state, source_addr, dest_addr)?;
                            results.push(entry);
                        }
                    }

                    Ok::<_, DbSqlError>(results)
                })
            })
            .await
    }

    async fn stream_active_channels<'a>(&'a self) -> Result<BoxStream<'a, Result<ChannelEntry>>> {
        // TODO(Phase 2-3): Update to query channel_current view or join with channel_state
        // Status and ClosureTime are now in channel_state table
        // For now, return error as this requires complex query changes
        Err(DbSqlError::LogicalError(
            "stream_active_channels requires channel_state table - use channel_current view in Phase 2-3".to_string(),
        ))
    }

    async fn upsert_channel<'a>(
        &'a self,
        tx: OptTx<'a>,
        channel_entry: ChannelEntry,
        block: u32,
        tx_index: u32,
        log_index: u32,
    ) -> Result<()> {
        let channel_id_hex = channel_entry.get_id().to_hex();
        let source_addr = channel_entry.source;
        let dest_addr = channel_entry.destination;
        let db_clone = self.clone();

        // Convert u32 to i64 for database storage
        let block_i64 = block as i64;
        let tx_index_i64 = tx_index as i64;
        let log_index_i64 = log_index as i64;

        self.nest_transaction(tx)
            .await?
            .perform(|tx| {
                Box::pin(async move {
                    // Step 1: Lookup account IDs for source and destination
                    let source_id = get_or_create_account_id(tx.as_ref(), &source_addr).await?;
                    let dest_id = get_or_create_account_id(tx.as_ref(), &dest_addr).await?;

                    // Step 2: Insert or find channel identity record
                    let channel_id = if let Some(existing_channel) = channel::Entity::find()
                        .filter(channel::Column::ConcreteChannelId.eq(&channel_id_hex))
                        .one(tx.as_ref())
                        .await?
                    {
                        // Channel exists - we're adding a new state
                        existing_channel.id
                    } else {
                        // Channel doesn't exist - create identity + initial state
                        let channel_model = channel::ActiveModel {
                            concrete_channel_id: Set(channel_id_hex),
                            source: Set(source_id),
                            destination: Set(dest_id),
                            ..Default::default()
                        };
                        let inserted = channel_model.insert(tx.as_ref()).await?;
                        inserted.id
                    };

                    // Step 3: Insert channel_state record with state information
                    // This creates a new immutable state record (never updates existing records)
                    insert_channel_state_and_emit(
                        &db_clone,
                        tx.as_ref(),
                        channel_id,
                        &channel_entry,
                        block_i64,
                        tx_index_i64,
                        log_index_i64,
                    )
                    .await?;

                    Ok::<_, DbSqlError>(())
                })
            })
            .await?;

        Ok(())
    }

    async fn get_channel_state_at_block<'a>(
        &'a self,
        tx: OptTx<'a>,
        id: &Hash,
        block: u32,
    ) -> Result<Option<ChannelEntry>> {
        let id_hex = id.to_hex();
        let block_i64 = block as i64;

        self.nest_transaction(tx)
            .await?
            .perform(|tx| {
                Box::pin(async move {
                    // Step 1: Find channel by concrete_channel_id
                    let channel_model = match Channel::find()
                        .filter(channel::Column::ConcreteChannelId.eq(id_hex))
                        .one(tx.as_ref())
                        .await?
                    {
                        Some(c) => c,
                        None => return Ok(None),
                    };

                    // Step 2: Get the most recent channel_state at or before the given block
                    let state_model = ChannelState::find()
                        .filter(channel_state::Column::ChannelId.eq(channel_model.id))
                        .filter(channel_state::Column::PublishedBlock.lte(block_i64))
                        .order_by_desc(channel_state::Column::PublishedBlock)
                        .order_by_desc(channel_state::Column::PublishedTxIndex)
                        .order_by_desc(channel_state::Column::PublishedLogIndex)
                        .one(tx.as_ref())
                        .await?;

                    // If no state exists at or before this block, return None
                    let state = match state_model {
                        Some(s) => s,
                        None => return Ok(None),
                    };

                    // Step 3: Lookup account addresses
                    let (source_addr, dest_addr) =
                        lookup_account_addresses(tx.as_ref(), channel_model.source, channel_model.destination).await?;

                    // Step 4: Reconstruct ChannelEntry
                    let entry = reconstruct_channel_entry(&channel_model, &state, source_addr, dest_addr)?;

                    Ok::<_, DbSqlError>(Some(entry))
                })
            })
            .await
    }

    async fn get_channel_history<'a>(&'a self, tx: OptTx<'a>, id: &Hash) -> Result<Vec<ChannelEntry>> {
        let id_hex = id.to_hex();

        self.nest_transaction(tx)
            .await?
            .perform(|tx| {
                Box::pin(async move {
                    // Step 1: Find channel by concrete_channel_id
                    let channel_model = match Channel::find()
                        .filter(channel::Column::ConcreteChannelId.eq(id_hex))
                        .one(tx.as_ref())
                        .await?
                    {
                        Some(c) => c,
                        None => return Ok(vec![]),
                    };

                    // Step 2: Get all channel_state records ordered chronologically
                    let state_models = ChannelState::find()
                        .filter(channel_state::Column::ChannelId.eq(channel_model.id))
                        .order_by_asc(channel_state::Column::PublishedBlock)
                        .order_by_asc(channel_state::Column::PublishedTxIndex)
                        .order_by_asc(channel_state::Column::PublishedLogIndex)
                        .all(tx.as_ref())
                        .await?;

                    // Step 3: Lookup account addresses once
                    let (source_addr, dest_addr) =
                        lookup_account_addresses(tx.as_ref(), channel_model.source, channel_model.destination).await?;

                    // Step 4: Reconstruct ChannelEntry for each state
                    let mut history = Vec::new();
                    for state in state_models {
                        let entry = reconstruct_channel_entry(&channel_model, &state, source_addr, dest_addr)?;
                        history.push(entry);
                    }

                    Ok::<_, DbSqlError>(history)
                })
            })
            .await
    }

    async fn mark_channel_state_corrupted<'a>(
        &'a self,
        tx: OptTx<'a>,
        id: &Hash,
        block: u32,
        tx_index: u32,
        log_index: u32,
    ) -> Result<()> {
        let id_hex = id.to_hex();
        let id_hex_clone = id_hex.clone();
        let id_hex_clone2 = id_hex.clone();
        let block_i64 = block as i64;
        let tx_index_i64 = tx_index as i64;
        let log_index_i64 = log_index as i64;

        self.nest_transaction(tx)
            .await?
            .perform(|tx| {
                Box::pin(async move {
                    // Step 1: Find channel by concrete_channel_id
                    let channel_model = Channel::find()
                        .filter(channel::Column::ConcreteChannelId.eq(&id_hex))
                        .one(tx.as_ref())
                        .await?
                        .ok_or_else(|| DbSqlError::LogicalError(format!("Channel {} not found", id_hex_clone)))?;

                    // Step 2: Find the specific channel_state by temporal coordinates
                    let state_model = ChannelState::find()
                        .filter(channel_state::Column::ChannelId.eq(channel_model.id))
                        .filter(channel_state::Column::PublishedBlock.eq(block_i64))
                        .filter(channel_state::Column::PublishedTxIndex.eq(tx_index_i64))
                        .filter(channel_state::Column::PublishedLogIndex.eq(log_index_i64))
                        .one(tx.as_ref())
                        .await?
                        .ok_or_else(|| {
                            DbSqlError::LogicalError(format!(
                                "Channel state not found for channel {} at block {}, tx_index {}, log_index {}",
                                id_hex_clone2, block, tx_index, log_index
                            ))
                        })?;

                    // Step 3: Update the corrupted_state flag
                    let mut active_model: channel_state::ActiveModel = state_model.into();
                    active_model.corrupted_state = Set(true);
                    active_model.update(tx.as_ref()).await?;

                    Ok::<_, DbSqlError>(())
                })
            })
            .await
    }
}

#[cfg(test)]
mod tests {
    use anyhow::Context;
    use hopr_crypto_random::random_bytes;
    use hopr_crypto_types::{
        keypairs::{ChainKeypair, OffchainKeypair},
        prelude::Keypair,
    };
    use hopr_internal_types::{
        channels::ChannelStatus,
        prelude::{ChannelDirection, ChannelEntry},
    };
    use hopr_primitive_types::prelude::{Address, HoprBalance};

    use crate::{
        BlokliDbGeneralModelOperations, accounts::BlokliDbAccountOperations, channels::BlokliDbChannelOperations,
        db::BlokliDb,
    };

    #[tokio::test]
    async fn test_insert_get_by_id() -> anyhow::Result<()> {
        let db = BlokliDb::new_in_memory().await?;

        // Create accounts first (required before channels can be created)
        let addr = Address::default();
        let packet_key = *OffchainKeypair::random().public();
        db.upsert_account(None, addr, packet_key, None, 1, 0, 0).await?;

        let ce = ChannelEntry::new(addr, addr, 0.into(), 0_u32.into(), ChannelStatus::Open, 0_u32.into());

        db.upsert_channel(None, ce, 1, 0, 0).await?;

        let from_db = db
            .get_channel_by_id(None, &ce.get_id())
            .await?
            .expect("channel must be present");

        assert_eq!(ce, from_db, "channels must be equal");

        Ok(())
    }

    #[tokio::test]
    async fn test_insert_get_by_parties() -> anyhow::Result<()> {
        let db = BlokliDb::new_in_memory().await?;

        let a = Address::from(random_bytes());
        let b = Address::from(random_bytes());

        // Create accounts first (required before channels can be created)
        let packet_key_a = *OffchainKeypair::random().public();
        let packet_key_b = *OffchainKeypair::random().public();
        db.upsert_account(None, a, packet_key_a, None, 1, 0, 0).await?;
        db.upsert_account(None, b, packet_key_b, None, 2, 0, 0).await?;

        let ce = ChannelEntry::new(a, b, 0.into(), 0_u32.into(), ChannelStatus::Open, 0_u32.into());

        db.upsert_channel(None, ce, 1, 0, 0).await?;
        let from_db = db
            .get_channel_by_parties(None, &a, &b)
            .await?
            .context("channel must be present")?;

        assert_eq!(ce, from_db, "channels must be equal");

        Ok(())
    }

    #[tokio::test]
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
    async fn test_channel_get_for_destination_that_exists_should_be_returned() -> anyhow::Result<()> {
        let db = BlokliDb::new_in_memory().await?;

        let expected_destination = Address::default();

        // Create account first (required before channels can be created)
        let packet_key_expected_destination = *OffchainKeypair::random().public();
        db.upsert_account(
            None,
            expected_destination,
            packet_key_expected_destination,
            None,
            1,
            0,
            0,
        )
        .await?;

        let ce = ChannelEntry::new(
            expected_destination,
            expected_destination,
            0.into(),
            0_u32.into(),
            ChannelStatus::Open,
            0_u32.into(),
        );

        db.upsert_channel(None, ce, 1, 0, 0).await?;
        let from_db = db
            .get_channels_via(None, ChannelDirection::Incoming, &Address::default())
            .await?
            .first()
            .cloned();

        assert_eq!(Some(ce), from_db, "should return a valid channel");

        Ok(())
    }

    #[tokio::test]
    async fn test_incoming_outgoing_channels() -> anyhow::Result<()> {
        let ckp = ChainKeypair::random();
        let addr_1 = ckp.public().to_address();
        let addr_2 = ChainKeypair::random().public().to_address();

        let db = BlokliDb::new_in_memory().await?;

        // Create accounts first (required before channels can be created)
        let packet_key_addr_1 = *OffchainKeypair::random().public();
        db.upsert_account(None, addr_1, packet_key_addr_1, None, 1, 0, 0)
            .await?;
        let packet_key_addr_2 = *OffchainKeypair::random().public();
        db.upsert_account(None, addr_2, packet_key_addr_2, None, 2, 0, 0)
            .await?;

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
                    db_clone.upsert_channel(Some(tx), ce_1, 1, 0, 0).await?;
                    db_clone.upsert_channel(Some(tx), ce_2, 1, 0, 1).await
                })
            })
            .await?;

        // Verify incoming and outgoing channels
        assert_eq!(
            vec![ce_2],
            db.get_channels_via(None, ChannelDirection::Incoming, &addr_1).await?,
            "should return incoming channel"
        );
        assert_eq!(
            vec![ce_1],
            db.get_channels_via(None, ChannelDirection::Outgoing, &addr_1).await?,
            "should return outgoing channel"
        );

        Ok(())
    }

    // ============================================================================
    // TDD Tests for Temporal Database Operations
    // ============================================================================

    #[tokio::test]
    async fn test_upsert_channel_creates_new_with_initial_state() -> anyhow::Result<()> {
        let db = BlokliDb::new_in_memory().await?;

        let addr_1 = Address::from(random_bytes());
        let addr_2 = Address::from(random_bytes());

        // Create accounts first
        let packet_key_addr_1 = *OffchainKeypair::random().public();
        db.upsert_account(None, addr_1, packet_key_addr_1, None, 1, 0, 0)
            .await?;
        let packet_key_addr_2 = *OffchainKeypair::random().public();
        db.upsert_account(None, addr_2, packet_key_addr_2, None, 1, 0, 0)
            .await?;

        // Create channel with initial state at block 100
        let initial_balance = HoprBalance::from(1000u32);
        let ce = ChannelEntry::new(
            addr_1,
            addr_2,
            initial_balance,
            0_u32.into(),
            ChannelStatus::Open,
            1_u32.into(),
        );

        db.upsert_channel(None, ce, 100, 0, 0).await?;

        // Verify channel can be retrieved
        let retrieved = db
            .get_channel_by_id(None, &ce.get_id())
            .await?
            .expect("channel should exist");

        assert_eq!(ce, retrieved, "channel should match initial state");

        Ok(())
    }

    #[tokio::test]
    async fn test_upsert_channel_adds_state_to_existing() -> anyhow::Result<()> {
        let db = BlokliDb::new_in_memory().await?;

        let addr_1 = Address::from(random_bytes());
        let addr_2 = Address::from(random_bytes());

        // Create accounts first
        let packet_key_addr_1 = *OffchainKeypair::random().public();
        db.upsert_account(None, addr_1, packet_key_addr_1, None, 1, 0, 0)
            .await?;
        let packet_key_addr_2 = *OffchainKeypair::random().public();
        db.upsert_account(None, addr_2, packet_key_addr_2, None, 1, 0, 0)
            .await?;

        // Create channel with initial state at block 100
        let initial_balance = HoprBalance::from(1000u32);
        let ce_initial = ChannelEntry::new(
            addr_1,
            addr_2,
            initial_balance,
            0_u32.into(),
            ChannelStatus::Open,
            1_u32.into(),
        );

        db.upsert_channel(None, ce_initial, 100, 0, 0).await?;

        // Update channel with new balance at block 200
        let new_balance = HoprBalance::from(2000u32);
        let ce_updated = ChannelEntry::new(
            addr_1,
            addr_2,
            new_balance,
            0_u32.into(),
            ChannelStatus::Open,
            1_u32.into(),
        );

        db.upsert_channel(None, ce_updated, 200, 0, 0).await?;

        // Verify latest state reflects new balance
        let latest = db
            .get_channel_by_id(None, &ce_updated.get_id())
            .await?
            .expect("channel should exist");

        assert_eq!(new_balance, latest.balance, "latest state should have new balance");

        // Verify old state still exists at block 100
        let historical = db
            .get_channel_state_at_block(None, &ce_initial.get_id(), 150)
            .await?
            .expect("historical state should exist");

        assert_eq!(
            initial_balance, historical.balance,
            "historical state should have original balance"
        );

        // Verify full history contains both states
        let history = db.get_channel_history(None, &ce_initial.get_id()).await?;

        assert_eq!(2, history.len(), "should have 2 state records");
        assert_eq!(
            initial_balance, history[0].balance,
            "first state should have initial balance"
        );
        assert_eq!(new_balance, history[1].balance, "second state should have new balance");

        Ok(())
    }

    #[tokio::test]
    async fn test_upsert_channel_status_transitions() -> anyhow::Result<()> {
        let db = BlokliDb::new_in_memory().await?;

        let addr_1 = Address::from(random_bytes());
        let addr_2 = Address::from(random_bytes());

        // Create accounts first
        let packet_key_addr_1 = *OffchainKeypair::random().public();
        db.upsert_account(None, addr_1, packet_key_addr_1, None, 1, 0, 0)
            .await?;
        let packet_key_addr_2 = *OffchainKeypair::random().public();
        db.upsert_account(None, addr_2, packet_key_addr_2, None, 1, 0, 0)
            .await?;

        let balance = HoprBalance::from(1000u32);

        // State 1: Open at block 100
        let ce_open = ChannelEntry::new(addr_1, addr_2, balance, 0_u32.into(), ChannelStatus::Open, 1_u32.into());
        db.upsert_channel(None, ce_open, 100, 0, 0).await?;

        // State 2: PendingToClose at block 200
        let closure_time = SystemTime::now() + std::time::Duration::from_secs(3600);
        let ce_pending = ChannelEntry::new(
            addr_1,
            addr_2,
            balance,
            0_u32.into(),
            ChannelStatus::PendingToClose(closure_time),
            1_u32.into(),
        );
        db.upsert_channel(None, ce_pending, 200, 0, 0).await?;

        // State 3: Closed at block 300
        let ce_closed = ChannelEntry::new(
            addr_1,
            addr_2,
            balance,
            0_u32.into(),
            ChannelStatus::Closed,
            1_u32.into(),
        );
        db.upsert_channel(None, ce_closed, 300, 0, 0).await?;

        // Verify full history has all 3 states
        let history = db.get_channel_history(None, &ce_open.get_id()).await?;

        assert_eq!(3, history.len(), "should have 3 state records");
        assert_eq!(ChannelStatus::Open, history[0].status, "first state should be Open");
        assert!(
            matches!(history[1].status, ChannelStatus::PendingToClose(_)),
            "second state should be PendingToClose"
        );
        assert_eq!(ChannelStatus::Closed, history[2].status, "third state should be Closed");

        // Verify temporal queries return correct states
        let at_block_150 = db
            .get_channel_state_at_block(None, &ce_open.get_id(), 150)
            .await?
            .expect("should exist");
        assert_eq!(ChannelStatus::Open, at_block_150.status);

        let at_block_250 = db
            .get_channel_state_at_block(None, &ce_open.get_id(), 250)
            .await?
            .expect("should exist");
        assert!(matches!(at_block_250.status, ChannelStatus::PendingToClose(_)));

        let at_block_350 = db
            .get_channel_state_at_block(None, &ce_open.get_id(), 350)
            .await?
            .expect("should exist");
        assert_eq!(ChannelStatus::Closed, at_block_350.status);

        Ok(())
    }

    #[tokio::test]
    async fn test_upsert_channel_ticket_index_updates() -> anyhow::Result<()> {
        let db = BlokliDb::new_in_memory().await?;

        let addr_1 = Address::from(random_bytes());
        let addr_2 = Address::from(random_bytes());

        // Create accounts first
        let packet_key_addr_1 = *OffchainKeypair::random().public();
        db.upsert_account(None, addr_1, packet_key_addr_1, None, 1, 0, 0)
            .await?;
        let packet_key_addr_2 = *OffchainKeypair::random().public();
        db.upsert_account(None, addr_2, packet_key_addr_2, None, 1, 0, 0)
            .await?;

        let balance = HoprBalance::from(1000u32);

        // Initial state: ticket_index = 0
        let ce_initial = ChannelEntry::new(addr_1, addr_2, balance, 0_u32.into(), ChannelStatus::Open, 1_u32.into());
        db.upsert_channel(None, ce_initial, 100, 0, 0).await?;

        // Update: ticket_index = 5
        let ce_updated = ChannelEntry::new(addr_1, addr_2, balance, 5_u32.into(), ChannelStatus::Open, 1_u32.into());
        db.upsert_channel(None, ce_updated, 200, 0, 0).await?;

        // Verify history preserved
        let history = db.get_channel_history(None, &ce_initial.get_id()).await?;

        assert_eq!(2, history.len(), "should have 2 state records");
        assert_eq!(
            0u32,
            history[0].ticket_index.as_u32(),
            "first state should have ticket_index 0"
        );
        assert_eq!(
            5u32,
            history[1].ticket_index.as_u32(),
            "second state should have ticket_index 5"
        );

        Ok(())
    }

    #[tokio::test]
    async fn test_get_channel_state_at_block() -> anyhow::Result<()> {
        let db = BlokliDb::new_in_memory().await?;

        let addr_1 = Address::from(random_bytes());
        let addr_2 = Address::from(random_bytes());

        // Create accounts first
        let packet_key_addr_1 = *OffchainKeypair::random().public();
        db.upsert_account(None, addr_1, packet_key_addr_1, None, 1, 0, 0)
            .await?;
        let packet_key_addr_2 = *OffchainKeypair::random().public();
        db.upsert_account(None, addr_2, packet_key_addr_2, None, 1, 0, 0)
            .await?;

        // Create multiple states at different blocks
        let ce_block_100 = ChannelEntry::new(
            addr_1,
            addr_2,
            HoprBalance::from(1000u32),
            0_u32.into(),
            ChannelStatus::Open,
            1_u32.into(),
        );
        db.upsert_channel(None, ce_block_100, 100, 0, 0).await?;

        let ce_block_200 = ChannelEntry::new(
            addr_1,
            addr_2,
            HoprBalance::from(2000u32),
            1_u32.into(),
            ChannelStatus::Open,
            1_u32.into(),
        );
        db.upsert_channel(None, ce_block_200, 200, 0, 0).await?;

        let ce_block_300 = ChannelEntry::new(
            addr_1,
            addr_2,
            HoprBalance::from(3000u32),
            2_u32.into(),
            ChannelStatus::Open,
            1_u32.into(),
        );
        db.upsert_channel(None, ce_block_300, 300, 0, 0).await?;

        // Query state at various blocks
        let at_50 = db.get_channel_state_at_block(None, &ce_block_100.get_id(), 50).await?;
        assert!(at_50.is_none(), "no state should exist before block 100");

        let at_100 = db
            .get_channel_state_at_block(None, &ce_block_100.get_id(), 100)
            .await?
            .expect("state should exist at block 100");
        assert_eq!(HoprBalance::from(1000u32), at_100.balance);

        let at_150 = db
            .get_channel_state_at_block(None, &ce_block_100.get_id(), 150)
            .await?
            .expect("state should exist at block 150");
        assert_eq!(
            HoprBalance::from(1000u32),
            at_150.balance,
            "should return state from block 100"
        );

        let at_200 = db
            .get_channel_state_at_block(None, &ce_block_100.get_id(), 200)
            .await?
            .expect("state should exist at block 200");
        assert_eq!(HoprBalance::from(2000u32), at_200.balance);

        let at_250 = db
            .get_channel_state_at_block(None, &ce_block_100.get_id(), 250)
            .await?
            .expect("state should exist at block 250");
        assert_eq!(
            HoprBalance::from(2000u32),
            at_250.balance,
            "should return state from block 200"
        );

        let at_300 = db
            .get_channel_state_at_block(None, &ce_block_100.get_id(), 300)
            .await?
            .expect("state should exist at block 300");
        assert_eq!(HoprBalance::from(3000u32), at_300.balance);

        let at_400 = db
            .get_channel_state_at_block(None, &ce_block_100.get_id(), 400)
            .await?
            .expect("state should exist at block 400");
        assert_eq!(
            HoprBalance::from(3000u32),
            at_400.balance,
            "should return latest state from block 300"
        );

        Ok(())
    }

    #[tokio::test]
    async fn test_get_channel_full_history() -> anyhow::Result<()> {
        let db = BlokliDb::new_in_memory().await?;

        let addr_1 = Address::from(random_bytes());
        let addr_2 = Address::from(random_bytes());

        // Create accounts first
        let packet_key_addr_1 = *OffchainKeypair::random().public();
        db.upsert_account(None, addr_1, packet_key_addr_1, None, 1, 0, 0)
            .await?;
        let packet_key_addr_2 = *OffchainKeypair::random().public();
        db.upsert_account(None, addr_2, packet_key_addr_2, None, 1, 0, 0)
            .await?;

        // Create 5 state changes across different blocks
        let channel_id = ChannelEntry::new(
            addr_1,
            addr_2,
            HoprBalance::from(1000u32),
            0_u32.into(),
            ChannelStatus::Open,
            1_u32.into(),
        )
        .get_id();

        for i in 0..5 {
            let balance = HoprBalance::from((i + 1) * 1000);
            let ce = ChannelEntry::new(addr_1, addr_2, balance, i.into(), ChannelStatus::Open, 1_u32.into());
            db.upsert_channel(None, ce, (i + 1) * 100, 0, 0).await?;
        }

        // Get full history
        let history = db.get_channel_history(None, &channel_id).await?;

        assert_eq!(5, history.len(), "should have 5 state records");

        // Verify chronological ordering
        for (i, state) in history.iter().enumerate() {
            let expected_balance = HoprBalance::from(((i + 1) * 1000) as u32);
            assert_eq!(
                expected_balance, state.balance,
                "state {} should have correct balance",
                i
            );
            assert_eq!(
                i as u32,
                state.ticket_index.as_u32(),
                "state {} should have correct ticket_index",
                i
            );
        }

        Ok(())
    }

    #[tokio::test]
    async fn test_mark_channel_state_corrupted() -> anyhow::Result<()> {
        let db = BlokliDb::new_in_memory().await?;

        let addr_1 = Address::from(random_bytes());
        let addr_2 = Address::from(random_bytes());

        // Create accounts first
        let packet_key_addr_1 = *OffchainKeypair::random().public();
        db.upsert_account(None, addr_1, packet_key_addr_1, None, 1, 0, 0)
            .await?;
        let packet_key_addr_2 = *OffchainKeypair::random().public();
        db.upsert_account(None, addr_2, packet_key_addr_2, None, 1, 0, 0)
            .await?;

        // Create channel with state at block 100
        let ce = ChannelEntry::new(
            addr_1,
            addr_2,
            HoprBalance::from(1000u32),
            0_u32.into(),
            ChannelStatus::Open,
            1_u32.into(),
        );
        db.upsert_channel(None, ce, 100, 0, 0).await?;

        // Mark the state as corrupted
        db.mark_channel_state_corrupted(None, &ce.get_id(), 100, 0, 0).await?;

        // TODO: Add query to verify corrupted flag is set
        // This requires adding a way to query the corrupted_state field
        // For now, just verify the operation doesn't error

        Ok(())
    }

    #[tokio::test]
    async fn test_concurrent_channel_upserts() -> anyhow::Result<()> {
        let db = BlokliDb::new_in_memory().await?;

        let addr_1 = Address::from(random_bytes());
        let addr_2 = Address::from(random_bytes());

        // Create accounts first
        let packet_key_addr_1 = *OffchainKeypair::random().public();
        db.upsert_account(None, addr_1, packet_key_addr_1, None, 1, 0, 0)
            .await?;
        let packet_key_addr_2 = *OffchainKeypair::random().public();
        db.upsert_account(None, addr_2, packet_key_addr_2, None, 1, 0, 0)
            .await?;

        // Create initial channel
        let ce_initial = ChannelEntry::new(
            addr_1,
            addr_2,
            HoprBalance::from(1000u32),
            0_u32.into(),
            ChannelStatus::Open,
            1_u32.into(),
        );
        db.upsert_channel(None, ce_initial, 100, 0, 0).await?;

        // Spawn multiple concurrent updates
        let mut handles = vec![];
        for i in 0..10 {
            let db_clone = db.clone();
            let channel_id = ce_initial.get_id();
            let handle = tokio::spawn(async move {
                let balance = HoprBalance::from((i + 2) * 1000);
                let ce = ChannelEntry::new(
                    addr_1,
                    addr_2,
                    balance,
                    (i + 1).into(),
                    ChannelStatus::Open,
                    1_u32.into(),
                );
                db_clone.upsert_channel(None, ce, 200 + i, 0, 0).await
            });
            handles.push(handle);
        }

        // Wait for all updates to complete
        for handle in handles {
            handle.await??;
        }

        // Verify all states were persisted (no lost updates)
        let history = db.get_channel_history(None, &ce_initial.get_id()).await?;

        assert_eq!(
            11,
            history.len(),
            "should have 11 state records (1 initial + 10 updates)"
        );

        Ok(())
    }
}
