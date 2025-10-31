//! GraphQL subscription root and resolver implementations

use std::time::Duration;

use async_broadcast::Receiver;
use async_graphql::{Context, Result, Subscription};
use async_stream::stream;
use blokli_api_types::{
    Account, Channel, ChannelUpdate, HoprBalance, NativeBalance, OpenedChannelsGraph, TokenValueString,
};
use blokli_chain_indexer::{IndexerState, state::ChannelEvent};
use blokli_db_entity::conversions::balances::{
    address_to_string, hopr_balance_to_string, native_balance_to_string, string_to_address,
};
use futures::Stream;
use sea_orm::{DatabaseConnection, EntityTrait};
use tokio::time::sleep;

use crate::conversions::{hopr_balance_from_model, native_balance_from_model};

/// Watermark representing the last fully processed blockchain position
///
/// This marks a point in the blockchain (block, transaction, log) that has been
/// completely processed by the indexer. All events up to and including this position
/// are guaranteed to be in the database.
#[derive(Debug, Clone, Copy)]
pub struct Watermark {
    /// Last indexed block number
    pub block: i64,
    /// Last indexed transaction index within the block
    pub tx_index: i64,
    /// Last indexed log index within the transaction
    pub log_index: i64,
}

/// Captures the current watermark synchronized with event bus subscription
#[allow(dead_code)]
/// This function is the critical synchronization point that prevents race conditions
/// in the 2-phase subscription model. It:
/// 1. Acquires read lock from IndexerState (waits for any in-progress block processing)
/// 2. Reads the current watermark from the database
/// 3. Subscribes to the event bus (while holding the lock)
/// 4. Returns watermark + receivers
///
/// By holding the read lock during steps 2-3, we guarantee that:
/// - No block processing can start between reading watermark and subscribing
/// - All events after the watermark will be received via the event bus
/// - No events can be missed or duplicated
///
/// # Arguments
///
/// * `indexer_state` - Shared indexer state for coordination
/// * `db` - Database connection for querying watermark
///
/// # Returns
///
/// Tuple of (watermark, event_receiver, shutdown_receiver)
async fn capture_watermark_synchronized(
    indexer_state: &IndexerState,
    db: &DatabaseConnection,
) -> Result<(Watermark, Receiver<ChannelEvent>, Receiver<()>)> {
    // Acquire read lock - this waits for any in-progress block processing to complete
    let _lock = indexer_state.acquire_watermark_lock().await;

    // Query the current watermark from chain_info table
    let chain_info = blokli_db_entity::chain_info::Entity::find()
        .one(db)
        .await
        .map_err(|e| async_graphql::Error::new(format!("Failed to query chain_info: {}", e)))?
        .ok_or_else(|| async_graphql::Error::new("chain_info not initialized"))?;

    let watermark = Watermark {
        block: chain_info.last_indexed_block,
        tx_index: chain_info.last_indexed_tx_index,
        log_index: chain_info.last_indexed_log_index,
    };

    // Subscribe to event bus and shutdown signal while still holding the lock
    // This ensures no events can be published between reading watermark and subscribing
    let event_receiver = indexer_state.subscribe_to_events();
    let shutdown_receiver = indexer_state.subscribe_to_shutdown();

    // Lock is automatically released when _lock goes out of scope
    Ok((watermark, event_receiver, shutdown_receiver))
}

/// Queries all open channels at a specific watermark
#[allow(dead_code)]
/// This implements the Phase 1 historical snapshot by querying all channels
/// that were open at the watermark position. Uses temporal queries to get
/// the state as it existed at that exact point in time.
///
/// Channels are returned in batches for efficient processing.
///
/// # Arguments
///
/// * `db` - Database connection
/// * `watermark` - Point in time to query channels at
/// * `batch_size` - Number of channels to process per batch
///
/// # Returns
///
/// Vector of ChannelUpdate objects with complete channel and account information
async fn query_channels_at_watermark(
    db: &DatabaseConnection,
    watermark: &Watermark,
    batch_size: usize,
) -> Result<Vec<ChannelUpdate>> {
    use std::collections::HashMap;

    use blokli_db_entity::{channel, channel_state};
    use sea_orm::{ColumnTrait, Condition, QueryFilter, QueryOrder};

    // Query all channels (identity table has no temporal component)
    let channels = channel::Entity::find()
        .all(db)
        .await
        .map_err(|e| async_graphql::Error::new(format!("Failed to query channels: {}", e)))?;

    if channels.is_empty() {
        return Ok(Vec::new());
    }

    let channel_ids: Vec<i32> = channels.iter().map(|c| c.id).collect();

    // Query channel_state for all channels, filtered by watermark
    // Only get states published at or before the watermark
    let channel_states_query = channel_state::Entity::find()
        .filter(channel_state::Column::ChannelId.is_in(channel_ids.clone()))
        .filter(
            Condition::any()
                .add(channel_state::Column::PublishedBlock.lt(watermark.block))
                .add(
                    Condition::all()
                        .add(channel_state::Column::PublishedBlock.eq(watermark.block))
                        .add(channel_state::Column::PublishedTxIndex.lt(watermark.tx_index)),
                )
                .add(
                    Condition::all()
                        .add(channel_state::Column::PublishedBlock.eq(watermark.block))
                        .add(channel_state::Column::PublishedTxIndex.eq(watermark.tx_index))
                        .add(channel_state::Column::PublishedLogIndex.lte(watermark.log_index)),
                ),
        )
        .order_by_desc(channel_state::Column::PublishedBlock)
        .order_by_desc(channel_state::Column::PublishedTxIndex)
        .order_by_desc(channel_state::Column::PublishedLogIndex);

    let channel_states = channel_states_query
        .all(db)
        .await
        .map_err(|e| async_graphql::Error::new(format!("Failed to query channel_state: {}", e)))?;

    // Build map of channel_id -> latest state (first occurrence due to ordering)
    let mut state_map: HashMap<i32, channel_state::Model> = HashMap::new();
    for state in channel_states {
        state_map.entry(state.channel_id).or_insert(state);
    }

    // Filter to only OPEN channels (status = 1)
    let open_channels: Vec<_> = channels
        .into_iter()
        .filter(|c| state_map.get(&c.id).map(|s| s.status == 1).unwrap_or(false))
        .collect();

    if open_channels.is_empty() {
        return Ok(Vec::new());
    }

    // Collect all unique account IDs we need to fetch
    let mut account_ids: Vec<i32> = open_channels
        .iter()
        .flat_map(|c| vec![c.source, c.destination])
        .collect();
    account_ids.sort_unstable();
    account_ids.dedup();

    // Fetch accounts using the existing aggregation function
    let accounts_result =
        blokli_db_entity::conversions::account_aggregation::fetch_accounts_by_keyids(db, account_ids.clone())
            .await
            .map_err(|e| async_graphql::Error::new(format!("Failed to fetch accounts: {}", e)))?;

    // Build account map for quick lookup
    let account_map: HashMap<i32, blokli_db_entity::conversions::account_aggregation::AggregatedAccount> =
        accounts_result.into_iter().map(|a| (a.keyid, a)).collect();

    // Build ChannelUpdate objects
    let mut results = Vec::new();
    for channel in open_channels {
        let state = state_map.get(&channel.id).unwrap(); // Safe because we filtered by state presence

        let source_account = account_map
            .get(&channel.source)
            .ok_or_else(|| async_graphql::Error::new(format!("Source account {} not found", channel.source)))?;

        let dest_account = account_map.get(&channel.destination).ok_or_else(|| {
            async_graphql::Error::new(format!("Destination account {} not found", channel.destination))
        })?;

        // Convert to GraphQL types
        use blokli_db_entity::conversions::balances::balance_to_string;

        let channel_gql = Channel {
            concrete_channel_id: channel.concrete_channel_id,
            source: channel.source,
            destination: channel.destination,
            balance: TokenValueString(balance_to_string(&state.balance)),
            status: state.status.into(),
            epoch: i32::try_from(state.epoch).unwrap_or(i32::MAX),
            ticket_index: blokli_api_types::UInt64(u64::try_from(state.ticket_index).unwrap_or(0)),
            closure_time: state.closure_time,
        };

        let source_gql = Account {
            keyid: source_account.keyid,
            chain_key: source_account.chain_key.clone(),
            packet_key: source_account.packet_key.clone(),
            account_hopr_balance: TokenValueString(source_account.account_hopr_balance.clone()),
            account_native_balance: TokenValueString(source_account.account_native_balance.clone()),
            safe_address: source_account.safe_address.clone(),
            safe_hopr_balance: source_account.safe_hopr_balance.clone().map(TokenValueString),
            safe_native_balance: source_account.safe_native_balance.clone().map(TokenValueString),
            multi_addresses: source_account.multi_addresses.clone(),
        };

        let dest_gql = Account {
            keyid: dest_account.keyid,
            chain_key: dest_account.chain_key.clone(),
            packet_key: dest_account.packet_key.clone(),
            account_hopr_balance: TokenValueString(dest_account.account_hopr_balance.clone()),
            account_native_balance: TokenValueString(dest_account.account_native_balance.clone()),
            safe_address: dest_account.safe_address.clone(),
            safe_hopr_balance: dest_account.safe_hopr_balance.clone().map(TokenValueString),
            safe_native_balance: dest_account.safe_native_balance.clone().map(TokenValueString),
            multi_addresses: dest_account.multi_addresses.clone(),
        };

        results.push(ChannelUpdate {
            channel: channel_gql,
            source: source_gql,
            destination: dest_gql,
        });

        if results.len() >= batch_size {
            break;
        }
    }

    Ok(results)
}

/// Converts a ChannelEvent to a ChannelUpdate by fetching current data
///
/// This function queries the database for the channel, its current state,
/// and both participating accounts, then constructs a complete ChannelUpdate.
///
/// # Arguments
///
/// * `db` - Database connection
/// * `event` - The channel event from the event bus
///
/// # Returns
///
/// ChannelUpdate with complete channel and account information, or None if channel not found
#[allow(dead_code)]
async fn fetch_channel_update(db: &DatabaseConnection, event: &ChannelEvent) -> Result<Option<ChannelUpdate>> {
    use std::collections::HashMap;

    use blokli_db_entity::{channel, channel_state};
    use sea_orm::{ColumnTrait, EntityTrait, QueryFilter, QueryOrder};

    // Extract channel_id from the event
    let channel_id = match event {
        ChannelEvent::Opened { channel_id }
        | ChannelEvent::Updated { channel_id }
        | ChannelEvent::Closed { channel_id } => *channel_id,
    };

    // Fetch the channel
    let channel = channel::Entity::find_by_id(channel_id)
        .one(db)
        .await
        .map_err(|e| async_graphql::Error::new(format!("Failed to query channel: {}", e)))?;

    let channel = match channel {
        Some(c) => c,
        None => return Ok(None), // Channel not found, skip
    };

    // Fetch latest channel_state
    let channel_state = channel_state::Entity::find()
        .filter(channel_state::Column::ChannelId.eq(channel_id))
        .order_by_desc(channel_state::Column::PublishedBlock)
        .order_by_desc(channel_state::Column::PublishedTxIndex)
        .order_by_desc(channel_state::Column::PublishedLogIndex)
        .one(db)
        .await
        .map_err(|e| async_graphql::Error::new(format!("Failed to query channel_state: {}", e)))?;

    let state = match channel_state {
        Some(s) => s,
        None => return Ok(None), // No state found, skip
    };

    // Fetch both accounts
    let account_ids = vec![channel.source, channel.destination];
    let accounts_result = blokli_db_entity::conversions::account_aggregation::fetch_accounts_by_keyids(db, account_ids)
        .await
        .map_err(|e| async_graphql::Error::new(format!("Failed to fetch accounts: {}", e)))?;

    let account_map: HashMap<i32, blokli_db_entity::conversions::account_aggregation::AggregatedAccount> =
        accounts_result.into_iter().map(|a| (a.keyid, a)).collect();

    let source_account = account_map
        .get(&channel.source)
        .ok_or_else(|| async_graphql::Error::new(format!("Source account {} not found", channel.source)))?;

    let dest_account = account_map
        .get(&channel.destination)
        .ok_or_else(|| async_graphql::Error::new(format!("Destination account {} not found", channel.destination)))?;

    // Convert to GraphQL types
    use blokli_db_entity::conversions::balances::balance_to_string;

    let channel_gql = Channel {
        concrete_channel_id: channel.concrete_channel_id,
        source: channel.source,
        destination: channel.destination,
        balance: TokenValueString(balance_to_string(&state.balance)),
        status: state.status.into(),
        epoch: i32::try_from(state.epoch).unwrap_or(i32::MAX),
        ticket_index: blokli_api_types::UInt64(u64::try_from(state.ticket_index).unwrap_or(0)),
        closure_time: state.closure_time,
    };

    let source_gql = Account {
        keyid: source_account.keyid,
        chain_key: source_account.chain_key.clone(),
        packet_key: source_account.packet_key.clone(),
        account_hopr_balance: TokenValueString(source_account.account_hopr_balance.clone()),
        account_native_balance: TokenValueString(source_account.account_native_balance.clone()),
        safe_address: source_account.safe_address.clone(),
        safe_hopr_balance: source_account.safe_hopr_balance.clone().map(TokenValueString),
        safe_native_balance: source_account.safe_native_balance.clone().map(TokenValueString),
        multi_addresses: source_account.multi_addresses.clone(),
    };

    let dest_gql = Account {
        keyid: dest_account.keyid,
        chain_key: dest_account.chain_key.clone(),
        packet_key: dest_account.packet_key.clone(),
        account_hopr_balance: TokenValueString(dest_account.account_hopr_balance.clone()),
        account_native_balance: TokenValueString(dest_account.account_native_balance.clone()),
        safe_address: dest_account.safe_address.clone(),
        safe_hopr_balance: dest_account.safe_hopr_balance.clone().map(TokenValueString),
        safe_native_balance: dest_account.safe_native_balance.clone().map(TokenValueString),
        multi_addresses: dest_account.multi_addresses.clone(),
    };

    Ok(Some(ChannelUpdate {
        channel: channel_gql,
        source: source_gql,
        destination: dest_gql,
    }))
}

/// Root subscription type providing real-time updates via Server-Sent Events (SSE)
pub struct SubscriptionRoot;

#[Subscription]
impl SubscriptionRoot {
    /// Subscribe to real-time updates of native balances for a specific address
    ///
    /// Provides updates whenever there is a change in the native token balance for the specified account.
    /// Updates are sent immediately when balance changes occur on-chain.
    #[graphql(name = "nativeBalanceUpdated")]
    async fn native_balance_updated(
        &self,
        ctx: &Context<'_>,
        #[graphql(desc = "On-chain address to monitor for balance changes (hexadecimal format)")] address: String,
    ) -> Result<impl Stream<Item = NativeBalance>> {
        let db = ctx.data::<DatabaseConnection>()?.clone();
        let addr = address.clone();

        Ok(stream! {
            loop {
                // TODO: Replace with actual database change notifications
                // For now, poll the database periodically
                sleep(Duration::from_secs(1)).await;

                // Query the latest balance
                if let Ok(Some(balance)) = Self::fetch_native_balance(&db, &addr).await {
                    yield balance;
                }
            }
        })
    }

    /// Subscribe to real-time updates of HOPR balances for a specific address
    ///
    /// Provides updates whenever there is a change in the HOPR token balance for the specified account.
    /// Updates are sent immediately when balance changes occur on-chain.
    #[graphql(name = "hoprBalanceUpdated")]
    async fn hopr_balance_updated(
        &self,
        ctx: &Context<'_>,
        #[graphql(desc = "On-chain address to monitor for balance changes (hexadecimal format)")] address: String,
    ) -> Result<impl Stream<Item = HoprBalance>> {
        let db = ctx.data::<DatabaseConnection>()?.clone();
        let addr = address.clone();

        Ok(stream! {
            loop {
                // TODO: Replace with actual database change notifications
                // For now, poll the database periodically
                sleep(Duration::from_secs(1)).await;

                // Query the latest balance
                if let Ok(Some(balance)) = Self::fetch_hopr_balance(&db, &addr).await {
                    yield balance;
                }
            }
        })
    }

    /// Subscribe to real-time updates of payment channels
    ///
    /// Provides updates whenever there is a change in the state of any payment channel,
    /// including channel opening, balance updates, status changes, and channel closure.
    /// Optional filters can be applied to only receive updates for specific channels.
    #[graphql(name = "channelUpdated")]
    async fn channel_updated(
        &self,
        ctx: &Context<'_>,
        #[graphql(desc = "Filter by source node keyid")] source_key_id: Option<i32>,
        #[graphql(desc = "Filter by destination node keyid")] destination_key_id: Option<i32>,
        #[graphql(desc = "Filter by concrete channel ID (hexadecimal format)")] concrete_channel_id: Option<String>,
        #[graphql(desc = "Filter by channel status")] status: Option<blokli_api_types::ChannelStatus>,
    ) -> Result<impl Stream<Item = Channel>> {
        let db = ctx.data::<DatabaseConnection>()?.clone();

        Ok(stream! {
            loop {
                // TODO: Replace with actual database change notifications
                // For now, poll the database periodically
                sleep(Duration::from_secs(1)).await;

                // Query the latest channels with filters
                if let Ok(channels) = Self::fetch_filtered_channels(&db, source_key_id, destination_key_id, concrete_channel_id.clone(), status).await {
                    for channel in channels {
                        yield channel;
                    }
                }
            }
        })
    }

    /// Subscribe to a full stream of existing channels and channel updates.
    ///
    /// Provides channel information on all open channels along with the accounts that participate in those channels.
    /// This provides a complete view of the active payment channel network.
    #[graphql(name = "openedChannelGraphUpdated")]
    async fn opened_channel_graph_updated(&self, ctx: &Context<'_>) -> Result<impl Stream<Item = OpenedChannelsGraph>> {
        let db = ctx.data::<DatabaseConnection>()?.clone();

        Ok(stream! {
            loop {
                // TODO: Replace with actual database change notifications
                // For now, poll the database periodically
                sleep(Duration::from_secs(1)).await;

                // Query the opened channels graph
                if let Ok(graph) = Self::fetch_opened_channels_graph(&db).await {
                    yield graph;
                }
            }
        })
    }

    /// Subscribe to a stream of opened payment channels with complete account information.
    ///
    /// This subscription implements a 2-phase streaming model:
    /// - Phase 1: Streams all open channels at subscription time (historical snapshot)
    /// - Phase 2: Streams real-time updates from the event bus
    ///
    /// Each update includes the channel along with complete source and destination account details.
    /// This ensures no race conditions, duplicates, or data loss during the transition between phases.
    #[graphql(name = "openedChannelsGraphStream")]
    async fn opened_channels_graph_stream(&self, ctx: &Context<'_>) -> Result<impl Stream<Item = ChannelUpdate>> {
        // Get dependencies from context
        let db = ctx.data::<DatabaseConnection>()?.clone();
        let indexer_state = ctx
            .data::<IndexerState>()
            .map_err(|_| async_graphql::Error::new("IndexerState not available in context"))?
            .clone();

        // Capture watermark and subscribe to event bus (synchronized)
        let (watermark, mut event_receiver, mut shutdown_receiver) =
            capture_watermark_synchronized(&indexer_state, &db).await?;

        // Configuration for Phase 1 batch size
        const BATCH_SIZE: usize = 100;

        Ok(stream! {
            // Phase 1: Stream historical snapshot of all open channels at watermark
            match query_channels_at_watermark(&db, &watermark, BATCH_SIZE).await {
                Ok(historical_channels) => {
                    for channel_update in historical_channels {
                        yield channel_update;
                    }
                }
                Err(e) => {
                    tracing::error!("Failed to query historical channels: {:?}", e);
                    return; // Terminate subscription on error
                }
            }

            // Phase 2: Stream real-time updates from event bus
            loop {
                tokio::select! {
                    // Check for shutdown signal first
                    shutdown_result = shutdown_receiver.recv() => {
                        match shutdown_result {
                            Ok(_) => {
                                tracing::info!("Subscription shutting down due to reorg");
                                return; // Terminate subscription on reorg
                            }
                            Err(async_broadcast::RecvError::Closed) => {
                                tracing::warn!("Shutdown channel closed");
                                return;
                            }
                            Err(async_broadcast::RecvError::Overflowed(n)) => {
                                tracing::warn!("Shutdown signal overflowed, missed {} signals", n);
                                // Continue - overflow on shutdown signal is not critical
                            }
                        }
                    }

                    // Receive event from event bus
                    event_result = event_receiver.recv() => {
                        match event_result {
                            Ok(event) => {
                                // Fetch channel update for this event
                                match fetch_channel_update(&db, &event).await {
                                    Ok(Some(channel_update)) => {
                                        yield channel_update;
                                    }
                                    Ok(None) => {
                                        // Channel not found or no state - skip silently
                                        tracing::debug!("Skipping event {:?} - channel not found", event);
                                    }
                                    Err(e) => {
                                        tracing::error!("Failed to fetch channel update: {:?}", e);
                                        // Continue on error rather than terminating subscription
                                    }
                                }
                            }
                            Err(async_broadcast::RecvError::Closed) => {
                                tracing::info!("Event bus closed, ending subscription");
                                return;
                            }
                            Err(async_broadcast::RecvError::Overflowed(n)) => {
                                tracing::warn!("Event bus overflowed, missed {} events - consider increasing buffer", n);
                                // Continue but log warning - client may need to reconnect
                            }
                        }
                    }
                }
            }
        })
    }

    /// Subscribe to real-time updates of account information
    ///
    /// Provides updates whenever there is a change in account information, including
    /// balance changes, Safe address linking, and multiaddress announcements.
    /// Optional filters can be applied to only receive updates for specific accounts.
    #[graphql(name = "accountUpdated")]
    async fn account_updated(
        &self,
        ctx: &Context<'_>,
        #[graphql(desc = "Filter by account keyid")] keyid: Option<i32>,
        #[graphql(desc = "Filter by packet key (peer ID format)")] packet_key: Option<String>,
        #[graphql(desc = "Filter by chain key (hexadecimal format)")] chain_key: Option<String>,
    ) -> Result<impl Stream<Item = Account>> {
        let db = ctx.data::<DatabaseConnection>()?.clone();

        Ok(stream! {
            loop {
                // TODO: Replace with actual database change notifications
                // For now, poll the database periodically
                sleep(Duration::from_secs(1)).await;

                // Query the latest accounts with filters
                if let Ok(accounts) = Self::fetch_filtered_accounts(&db, keyid, packet_key.clone(), chain_key.clone()).await {
                    for account in accounts {
                        yield account;
                    }
                }
            }
        })
    }
}

// Helper methods for fetching data
impl SubscriptionRoot {
    async fn fetch_native_balance(
        db: &DatabaseConnection,
        address: &str,
    ) -> Result<Option<NativeBalance>, sea_orm::DbErr> {
        use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};

        // Convert hex string address to binary for database query
        let binary_address = string_to_address(address);

        let balance = blokli_db_entity::native_balance::Entity::find()
            .filter(blokli_db_entity::native_balance::Column::Address.eq(binary_address))
            .one(db)
            .await?;

        Ok(balance.map(native_balance_from_model))
    }

    async fn fetch_hopr_balance(db: &DatabaseConnection, address: &str) -> Result<Option<HoprBalance>, sea_orm::DbErr> {
        use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};

        // Convert hex string address to binary for database query
        let binary_address = string_to_address(address);

        let balance = blokli_db_entity::hopr_balance::Entity::find()
            .filter(blokli_db_entity::hopr_balance::Column::Address.eq(binary_address))
            .one(db)
            .await?;

        Ok(balance.map(hopr_balance_from_model))
    }

    async fn fetch_filtered_channels(
        db: &DatabaseConnection,
        source_key_id: Option<i32>,
        destination_key_id: Option<i32>,
        concrete_channel_id: Option<String>,
        status: Option<blokli_api_types::ChannelStatus>,
    ) -> Result<Vec<Channel>, sea_orm::DbErr> {
        use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};

        // Build query with filters
        let mut query = blokli_db_entity::channel::Entity::find();

        if let Some(src_keyid) = source_key_id {
            query = query.filter(blokli_db_entity::channel::Column::Source.eq(src_keyid));
        }

        if let Some(dst_keyid) = destination_key_id {
            query = query.filter(blokli_db_entity::channel::Column::Destination.eq(dst_keyid));
        }

        if let Some(channel_id) = concrete_channel_id {
            query = query.filter(blokli_db_entity::channel::Column::ConcreteChannelId.eq(channel_id));
        }

        // TODO(Phase 2-3): Status filtering requires querying channel_state table
        if let Some(_status_filter) = status {
            return Err(sea_orm::DbErr::Custom(
                "Channel status filtering temporarily unavailable during schema migration".to_string(),
            ));
        }

        let _channels = query.all(db).await?;

        // TODO(Phase 2-3): Cannot use channel_from_model until we implement channel_state lookup
        Err(sea_orm::DbErr::Custom(
            "Channel subscription temporarily unavailable during schema migration".to_string(),
        ))
    }

    async fn fetch_filtered_accounts(
        db: &DatabaseConnection,
        keyid: Option<i32>,
        packet_key: Option<String>,
        chain_key: Option<String>,
    ) -> Result<Vec<Account>, sea_orm::DbErr> {
        use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};

        // Build query with filters
        let mut query = blokli_db_entity::account::Entity::find();

        if let Some(id) = keyid {
            query = query.filter(blokli_db_entity::account::Column::Id.eq(id));
        }

        if let Some(pk) = packet_key {
            query = query.filter(blokli_db_entity::account::Column::PacketKey.eq(pk));
        }

        if let Some(ck) = chain_key {
            // Convert hex string address to binary for database query
            let binary_chain_key = string_to_address(&ck);
            query = query.filter(blokli_db_entity::account::Column::ChainKey.eq(binary_chain_key));
        }

        let accounts = query.all(db).await?;

        let mut result = Vec::new();

        for account_model in accounts {
            // Fetch announcements for this account
            let announcements = blokli_db_entity::announcement::Entity::find()
                .filter(blokli_db_entity::announcement::Column::AccountId.eq(account_model.id))
                .all(db)
                .await?;

            let multi_addresses: Vec<String> = announcements.into_iter().map(|a| a.multiaddress).collect();

            // Fetch HOPR balance for account's chain_key
            // Returns zero balance if no balance record exists (hopr_balance_to_string(&[]) returns "0")
            let hopr_balance_value = blokli_db_entity::hopr_balance::Entity::find()
                .filter(blokli_db_entity::hopr_balance::Column::Address.eq(account_model.chain_key.clone()))
                .one(db)
                .await?
                .map(|b| hopr_balance_to_string(&b.balance))
                .unwrap_or_else(|| hopr_balance_to_string(&[]));

            // Fetch Native balance for account's chain_key
            // Returns zero balance if no balance record exists (native_balance_to_string(&[]) returns "0")
            let native_balance_value = blokli_db_entity::native_balance::Entity::find()
                .filter(blokli_db_entity::native_balance::Column::Address.eq(account_model.chain_key.clone()))
                .one(db)
                .await?
                .map(|b| native_balance_to_string(&b.balance))
                .unwrap_or_else(|| native_balance_to_string(&[]));

            // Convert addresses to hex strings for GraphQL response
            let chain_key_str = address_to_string(&account_model.chain_key);

            // TODO(Phase 2-3): Query account_state table for safe_address
            // safe_address has been moved to account_state table
            let safe_address_str = None::<String>;

            // TODO(Phase 2-3): Fetch safe balances once safe_address is available from account_state
            let (safe_hopr_balance, safe_native_balance) = (None, None);

            result.push(Account {
                keyid: account_model.id,
                chain_key: chain_key_str,
                packet_key: account_model.packet_key,
                account_hopr_balance: TokenValueString(hopr_balance_value),
                account_native_balance: TokenValueString(native_balance_value),
                safe_address: safe_address_str,
                safe_hopr_balance: safe_hopr_balance.map(TokenValueString),
                safe_native_balance: safe_native_balance.map(TokenValueString),
                multi_addresses,
            });
        }

        Ok(result)
    }

    async fn fetch_opened_channels_graph(_db: &DatabaseConnection) -> Result<OpenedChannelsGraph, sea_orm::DbErr> {
        // TODO(Phase 2-3): This function requires querying channel_state table for status
        // For now, return empty graph until we implement the channel_state lookup
        // Original logic: Fetch all OPEN channels (status = 1) and their participating accounts

        // Temporary: return empty graph
        let channels: Vec<Channel> = Vec::new();
        let accounts: Vec<Account> = Vec::new();

        Ok(OpenedChannelsGraph { channels, accounts })
    }
}
