// Allow casts for Solidity uint24/uint48 that fit safely in i64
#![allow(clippy::cast_possible_wrap)]
// Allow casts from i64 back to u64 for values that originated from Solidity uints
#![allow(clippy::cast_sign_loss)]

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
async fn get_or_create_account_id(tx: &sea_orm::DatabaseTransaction, address: &Address) -> Result<i32> {
    use blokli_db_entity::{account, prelude::Account};

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
    source_id: i32,
    dest_id: i32,
) -> Result<(Address, Address)> {
    use blokli_db_entity::prelude::Account;

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
    async fn get_channel_by_id<'a>(&'a self, tx: OptTx<'a>, id: &Hash) -> Result<Option<ChannelEntry>> {
        let id_hex = id.to_hex();
        self.nest_transaction(tx)
            .await?
            .perform(|tx| {
                Box::pin(async move {
                    use blokli_db_entity::{
                        channel, channel_state,
                        prelude::{Channel, ChannelState},
                    };
                    use sea_orm::QueryOrder;

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
                    use blokli_db_entity::{
                        account, channel, channel_state,
                        prelude::{Account, Channel, ChannelState},
                    };
                    use sea_orm::QueryOrder;

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
                    use blokli_db_entity::{
                        account, channel, channel_state,
                        prelude::{Account, Channel, ChannelState},
                    };
                    use sea_orm::QueryOrder;

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

    async fn upsert_channel<'a>(&'a self, tx: OptTx<'a>, channel_entry: ChannelEntry) -> Result<()> {
        // TODO(Phase 3): Add position parameters (block, tx_index, log_index) to this function
        // For now, using placeholder position (0, 0, 0) which will need to be updated
        // when integrating with the indexer that knows the actual blockchain position
        let (block, tx_index, log_index) = (0i64, 0i64, 0i64);

        let channel_id_hex = channel_entry.get_id().to_hex();
        let source_addr = channel_entry.source;
        let dest_addr = channel_entry.destination;
        let db_clone = self.clone();

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
                        existing_channel.id
                    } else {
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
    use hopr_crypto_types::{
        keypairs::{ChainKeypair, OffchainKeypair},
        prelude::Keypair,
    };
    use hopr_internal_types::{
        account::AccountType,
        channels::ChannelStatus,
        prelude::{AccountEntry, ChannelDirection, ChannelEntry},
    };
    use hopr_primitive_types::prelude::Address;

    use crate::{
        BlokliDbGeneralModelOperations, accounts::BlokliDbAccountOperations, channels::BlokliDbChannelOperations,
        db::BlokliDb,
    };

    #[tokio::test]
    async fn test_insert_get_by_id() -> anyhow::Result<()> {
        let db = BlokliDb::new_in_memory().await?;

        // Create accounts first (required before channels can be created)
        let addr = Address::default();
        db.insert_account(
            None,
            AccountEntry {
                public_key: *OffchainKeypair::random().public(),
                chain_addr: addr,
                published_at: 1,
                entry_type: AccountType::NotAnnounced,
            },
        )
        .await?;

        let ce = ChannelEntry::new(addr, addr, 0.into(), 0_u32.into(), ChannelStatus::Open, 0_u32.into());

        db.upsert_channel(None, ce).await?;

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
        db.insert_account(
            None,
            AccountEntry {
                public_key: *OffchainKeypair::random().public(),
                chain_addr: a,
                published_at: 1,
                entry_type: AccountType::NotAnnounced,
            },
        )
        .await?;
        db.insert_account(
            None,
            AccountEntry {
                public_key: *OffchainKeypair::random().public(),
                chain_addr: b,
                published_at: 2,
                entry_type: AccountType::NotAnnounced,
            },
        )
        .await?;

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
        db.insert_account(
            None,
            AccountEntry {
                public_key: *OffchainKeypair::random().public(),
                chain_addr: expected_destination,
                published_at: 1,
                entry_type: AccountType::NotAnnounced,
            },
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
    async fn test_incoming_outgoing_channels() -> anyhow::Result<()> {
        let ckp = ChainKeypair::random();
        let addr_1 = ckp.public().to_address();
        let addr_2 = ChainKeypair::random().public().to_address();

        let db = BlokliDb::new_in_memory().await?;

        // Create accounts first (required before channels can be created)
        db.insert_account(
            None,
            AccountEntry {
                public_key: *OffchainKeypair::random().public(),
                chain_addr: addr_1,
                published_at: 1,
                entry_type: AccountType::NotAnnounced,
            },
        )
        .await?;
        db.insert_account(
            None,
            AccountEntry {
                public_key: *OffchainKeypair::random().public(),
                chain_addr: addr_2,
                published_at: 2,
                entry_type: AccountType::NotAnnounced,
            },
        )
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
                    db_clone.upsert_channel(Some(tx), ce_1).await?;
                    db_clone.upsert_channel(Some(tx), ce_2).await
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
}
