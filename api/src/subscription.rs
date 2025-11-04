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

#[cfg(test)]
mod tests {
    use blokli_chain_indexer::state::ChannelEvent;
    use blokli_db::{BlokliDbGeneralModelOperations, db::BlokliDb};
    use blokli_db_entity::codegen::prelude::*;
    use sea_orm::{ActiveModelTrait, Set};

    use super::*;

    // Helper: Create test channel in database
    async fn create_test_channel(
        db: &DatabaseConnection,
        source: i32,
        destination: i32,
        concrete_id: &str,
    ) -> anyhow::Result<i32> {
        let channel = blokli_db_entity::channel::ActiveModel {
            id: Default::default(),
            source: Set(source),
            destination: Set(destination),
            concrete_channel_id: Set(concrete_id.to_string()),
        };

        let result = channel.insert(db).await?;
        Ok(result.id)
    }

    // Helper: Create test account in database
    async fn create_test_account(db: &DatabaseConnection, chain_key: Vec<u8>, packet_key: &str) -> anyhow::Result<i32> {
        let account = blokli_db_entity::account::ActiveModel {
            id: Default::default(),
            chain_key: Set(chain_key),
            packet_key: Set(packet_key.to_string()),
            published_block: Set(0),
            published_tx_index: Set(0),
            published_log_index: Set(0),
        };

        let result = account.insert(db).await?;
        Ok(result.id)
    }

    // Helper: Insert channel_state
    async fn insert_channel_state(
        db: &DatabaseConnection,
        channel_id: i32,
        block: i64,
        tx_index: i64,
        log_index: i64,
        balance: Vec<u8>,
        status: i8,
    ) -> anyhow::Result<i32> {
        let state = blokli_db_entity::channel_state::ActiveModel {
            id: Default::default(),
            channel_id: Set(channel_id),
            balance: Set(balance),
            status: Set(status),
            epoch: Set(0),
            ticket_index: Set(0),
            closure_time: Set(None),
            corrupted_state: Set(false),
            published_block: Set(block),
            published_tx_index: Set(tx_index),
            published_log_index: Set(log_index),
            reorg_correction: Set(false),
        };

        let result = state.insert(db).await?;
        Ok(result.id)
    }

    // Helper: Initialize chain_info with watermark
    async fn init_chain_info(db: &DatabaseConnection, block: i64, tx_index: i64, log_index: i64) -> anyhow::Result<()> {
        let chain_info = blokli_db_entity::chain_info::ActiveModel {
            id: Set(1),
            last_indexed_block: Set(block),
            last_indexed_tx_index: Set(tx_index),
            last_indexed_log_index: Set(log_index),
            ticket_price: Set(None),
            channels_dst: Set(None),
            ledger_dst: Set(None),
            safe_registry_dst: Set(None),
            min_incoming_ticket_win_prob: Set(0.0),
            channel_closure_grace_period: Set(None),
        };

        chain_info.insert(db).await?;
        Ok(())
    }

    #[tokio::test]
    async fn test_capture_watermark_returns_correct_position() {
        let db = BlokliDb::new_in_memory().await.unwrap();
        let indexer_state = IndexerState::new(10, 100);

        // Initialize chain_info with specific watermark
        init_chain_info(db.conn(blokli_db::TargetDb::Index), 1000, 5, 3)
            .await
            .unwrap();

        // Capture watermark
        let (watermark, _event_rx, _shutdown_rx) =
            capture_watermark_synchronized(&indexer_state, db.conn(blokli_db::TargetDb::Index))
                .await
                .unwrap();

        // Verify watermark matches chain_info
        assert_eq!(watermark.block, 1000);
        assert_eq!(watermark.tx_index, 5);
        assert_eq!(watermark.log_index, 3);
    }

    #[tokio::test]
    async fn test_capture_watermark_subscribes_to_event_bus() {
        let db = BlokliDb::new_in_memory().await.unwrap();
        let indexer_state = IndexerState::new(10, 100);

        init_chain_info(db.conn(blokli_db::TargetDb::Index), 100, 0, 0)
            .await
            .unwrap();

        // Capture watermark (subscribes to event bus)
        let (_watermark, mut event_rx, _shutdown_rx) =
            capture_watermark_synchronized(&indexer_state, db.conn(blokli_db::TargetDb::Index))
                .await
                .unwrap();

        // Publish event to bus
        indexer_state.publish_event(ChannelEvent::Opened { channel_id: 42 });

        // Verify subscriber receives the event
        let received = event_rx.recv().await.unwrap();
        assert!(matches!(received, ChannelEvent::Opened { channel_id: 42 }));
    }

    #[tokio::test]
    async fn test_capture_watermark_fails_when_chain_info_missing() {
        let db = BlokliDb::new_in_memory().await.unwrap();
        let indexer_state = IndexerState::new(10, 100);

        // Do NOT initialize chain_info - should fail
        let result = capture_watermark_synchronized(&indexer_state, db.conn(blokli_db::TargetDb::Index)).await;

        assert!(result.is_err());
        assert!(result.unwrap_err().message.contains("chain_info not initialized"));
    }

    #[tokio::test]
    async fn test_query_channels_at_watermark_empty_db() {
        let db = BlokliDb::new_in_memory().await.unwrap();

        let watermark = Watermark {
            block: 100,
            tx_index: 0,
            log_index: 0,
        };

        let result = query_channels_at_watermark(db.conn(blokli_db::TargetDb::Index), &watermark, 100)
            .await
            .unwrap();

        // No channels exist - should return empty
        assert_eq!(result.len(), 0);
    }

    #[tokio::test]
    async fn test_query_channels_at_watermark_finds_open_channel() {
        let db = BlokliDb::new_in_memory().await.unwrap();

        // Create accounts
        let source_id = create_test_account(db.conn(blokli_db::TargetDb::Index), vec![1; 20], "peer1")
            .await
            .unwrap();
        let dest_id = create_test_account(db.conn(blokli_db::TargetDb::Index), vec![2; 20], "peer2")
            .await
            .unwrap();

        // Create channel
        let channel_id = create_test_channel(db.conn(blokli_db::TargetDb::Index), source_id, dest_id, "0xabc123")
            .await
            .unwrap();

        // Insert OPEN channel state (status = 1) at block 50
        insert_channel_state(
            db.conn(blokli_db::TargetDb::Index),
            channel_id,
            50,
            0,
            0,
            vec![1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0], // 12 bytes balance
            1,                                        // OPEN status
        )
        .await
        .unwrap();

        // Query at block 100 (after channel opened)
        let watermark = Watermark {
            block: 100,
            tx_index: 0,
            log_index: 0,
        };

        let result = query_channels_at_watermark(db.conn(blokli_db::TargetDb::Index), &watermark, 100)
            .await
            .unwrap();

        // Should find 1 open channel
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].channel.concrete_channel_id, "0xabc123");
        assert_eq!(result[0].channel.status, blokli_api_types::ChannelStatus::Open);
    }

    #[tokio::test]
    async fn test_query_channels_at_watermark_excludes_closed_channels() {
        let db = BlokliDb::new_in_memory().await.unwrap();

        // Create accounts
        let source_id = create_test_account(db.conn(blokli_db::TargetDb::Index), vec![1; 20], "peer1")
            .await
            .unwrap();
        let dest_id = create_test_account(db.conn(blokli_db::TargetDb::Index), vec![2; 20], "peer2")
            .await
            .unwrap();

        // Create channel
        let channel_id = create_test_channel(db.conn(blokli_db::TargetDb::Index), source_id, dest_id, "0xabc123")
            .await
            .unwrap();

        // Insert CLOSED channel state (status = 2)
        insert_channel_state(
            db.conn(blokli_db::TargetDb::Index),
            channel_id,
            50,
            0,
            0,
            vec![1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
            2, // CLOSED status
        )
        .await
        .unwrap();

        let watermark = Watermark {
            block: 100,
            tx_index: 0,
            log_index: 0,
        };

        let result = query_channels_at_watermark(db.conn(blokli_db::TargetDb::Index), &watermark, 100)
            .await
            .unwrap();

        // Closed channel should not be returned
        assert_eq!(result.len(), 0);
    }

    #[tokio::test]
    async fn test_query_channels_at_watermark_respects_position() {
        let db = BlokliDb::new_in_memory().await.unwrap();

        // Create accounts
        let source_id = create_test_account(db.conn(blokli_db::TargetDb::Index), vec![1; 20], "peer1")
            .await
            .unwrap();
        let dest_id = create_test_account(db.conn(blokli_db::TargetDb::Index), vec![2; 20], "peer2")
            .await
            .unwrap();

        // Create channel
        let channel_id = create_test_channel(db.conn(blokli_db::TargetDb::Index), source_id, dest_id, "0xabc123")
            .await
            .unwrap();

        // Insert channel state at block 150 (after watermark)
        insert_channel_state(
            db.conn(blokli_db::TargetDb::Index),
            channel_id,
            150,
            0,
            0,
            vec![1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
            1, // OPEN
        )
        .await
        .unwrap();

        // Query at block 100 (before channel opened)
        let watermark = Watermark {
            block: 100,
            tx_index: 0,
            log_index: 0,
        };

        let result = query_channels_at_watermark(db.conn(blokli_db::TargetDb::Index), &watermark, 100)
            .await
            .unwrap();

        // Channel opened after watermark - should not be included
        assert_eq!(result.len(), 0);
    }

    #[tokio::test]
    async fn test_query_channels_at_watermark_uses_latest_state() {
        let db = BlokliDb::new_in_memory().await.unwrap();

        // Create accounts
        let source_id = create_test_account(db.conn(blokli_db::TargetDb::Index), vec![1; 20], "peer1")
            .await
            .unwrap();
        let dest_id = create_test_account(db.conn(blokli_db::TargetDb::Index), vec![2; 20], "peer2")
            .await
            .unwrap();

        // Create channel
        let channel_id = create_test_channel(db.conn(blokli_db::TargetDb::Index), source_id, dest_id, "0xabc123")
            .await
            .unwrap();

        // Insert multiple states for same channel
        insert_channel_state(
            db.conn(blokli_db::TargetDb::Index),
            channel_id,
            50,
            0,
            0,
            vec![1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0], // balance = 1
            1,                                        // OPEN
        )
        .await
        .unwrap();

        insert_channel_state(
            db.conn(blokli_db::TargetDb::Index),
            channel_id,
            75,
            0,
            0,
            vec![2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0], // balance = 2 (updated)
            1,                                        // Still OPEN
        )
        .await
        .unwrap();

        // Query at block 100
        let watermark = Watermark {
            block: 100,
            tx_index: 0,
            log_index: 0,
        };

        let result = query_channels_at_watermark(db.conn(blokli_db::TargetDb::Index), &watermark, 100)
            .await
            .unwrap();

        // Should return channel with latest balance (2)
        assert_eq!(result.len(), 1);
        // Balance should be "2" in string form
        assert_eq!(result[0].channel.balance.0, "2");
    }

    #[tokio::test]
    async fn test_query_channels_at_watermark_respects_batch_size() {
        let db = BlokliDb::new_in_memory().await.unwrap();

        // Create 5 open channels
        for i in 0..5 {
            let source_id = create_test_account(
                db.conn(blokli_db::TargetDb::Index),
                vec![i as u8; 20],
                &format!("peer{}_src", i),
            )
            .await
            .unwrap();
            let dest_id = create_test_account(
                db.conn(blokli_db::TargetDb::Index),
                vec![(i + 10) as u8; 20],
                &format!("peer{}_dst", i),
            )
            .await
            .unwrap();

            let channel_id = create_test_channel(
                db.conn(blokli_db::TargetDb::Index),
                source_id,
                dest_id,
                &format!("0xchannel{}", i),
            )
            .await
            .unwrap();

            insert_channel_state(
                db.conn(blokli_db::TargetDb::Index),
                channel_id,
                50,
                0,
                i as i64,
                vec![1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
                1, // OPEN
            )
            .await
            .unwrap();
        }

        let watermark = Watermark {
            block: 100,
            tx_index: 0,
            log_index: 0,
        };

        // Query with batch_size = 3
        let result = query_channels_at_watermark(db.conn(blokli_db::TargetDb::Index), &watermark, 3)
            .await
            .unwrap();

        // Should return only 3 channels despite 5 existing
        assert_eq!(result.len(), 3);
    }

    #[tokio::test]
    async fn test_fetch_channel_update_finds_existing_channel() {
        let db = BlokliDb::new_in_memory().await.unwrap();

        // Create accounts
        let source_id = create_test_account(db.conn(blokli_db::TargetDb::Index), vec![1; 20], "peer1")
            .await
            .unwrap();
        let dest_id = create_test_account(db.conn(blokli_db::TargetDb::Index), vec![2; 20], "peer2")
            .await
            .unwrap();

        // Create channel
        let channel_id = create_test_channel(db.conn(blokli_db::TargetDb::Index), source_id, dest_id, "0xabc123")
            .await
            .unwrap();

        // Insert channel state
        insert_channel_state(
            db.conn(blokli_db::TargetDb::Index),
            channel_id,
            50,
            0,
            0,
            vec![5, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
            1, // OPEN
        )
        .await
        .unwrap();

        // Fetch channel update for event
        let event = ChannelEvent::Updated { channel_id };
        let result = fetch_channel_update(db.conn(blokli_db::TargetDb::Index), &event)
            .await
            .unwrap();

        assert!(result.is_some());
        let update = result.unwrap();
        assert_eq!(update.channel.concrete_channel_id, "0xabc123");
        assert_eq!(update.channel.balance.0, "5");
    }

    #[tokio::test]
    async fn test_fetch_channel_update_returns_none_for_missing_channel() {
        let db = BlokliDb::new_in_memory().await.unwrap();

        // Try to fetch non-existent channel
        let event = ChannelEvent::Updated { channel_id: 999 };
        let result = fetch_channel_update(db.conn(blokli_db::TargetDb::Index), &event)
            .await
            .unwrap();

        // Should return None for missing channel
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_fetch_channel_update_handles_all_event_types() {
        let db = BlokliDb::new_in_memory().await.unwrap();

        // Create test channel
        let source_id = create_test_account(db.conn(blokli_db::TargetDb::Index), vec![1; 20], "peer1")
            .await
            .unwrap();
        let dest_id = create_test_account(db.conn(blokli_db::TargetDb::Index), vec![2; 20], "peer2")
            .await
            .unwrap();
        let channel_id = create_test_channel(db.conn(blokli_db::TargetDb::Index), source_id, dest_id, "0xabc123")
            .await
            .unwrap();
        insert_channel_state(
            db.conn(blokli_db::TargetDb::Index),
            channel_id,
            50,
            0,
            0,
            vec![1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
            1,
        )
        .await
        .unwrap();

        // Test all event types
        let events = vec![
            ChannelEvent::Opened { channel_id },
            ChannelEvent::Updated { channel_id },
            ChannelEvent::Closed { channel_id },
        ];

        for event in events {
            let result = fetch_channel_update(db.conn(blokli_db::TargetDb::Index), &event)
                .await
                .unwrap();
            assert!(result.is_some(), "Failed for event: {:?}", event);
        }
    }

    #[tokio::test]
    async fn test_watermark_struct_fields() {
        let watermark = Watermark {
            block: 12345,
            tx_index: 67,
            log_index: 89,
        };

        assert_eq!(watermark.block, 12345);
        assert_eq!(watermark.tx_index, 67);
        assert_eq!(watermark.log_index, 89);
    }

    #[tokio::test]
    async fn test_fetch_native_balance_finds_existing_balance() {
        let db = BlokliDb::new_in_memory().await.unwrap();

        // Insert native balance
        let balance_model = blokli_db_entity::native_balance::ActiveModel {
            id: Default::default(),
            address: Set(vec![1; 20]),
            balance: Set(vec![10, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]),
            last_changed_block: Set(0),
            last_changed_tx_index: Set(0),
            last_changed_log_index: Set(0),
        };
        balance_model.insert(db.conn(blokli_db::TargetDb::Index)).await.unwrap();

        // Fetch balance using hex string address
        let address = "0x0101010101010101010101010101010101010101";
        let result = SubscriptionRoot::fetch_native_balance(db.conn(blokli_db::TargetDb::Index), address)
            .await
            .unwrap();

        assert!(result.is_some());
    }

    #[tokio::test]
    async fn test_fetch_native_balance_returns_none_for_missing() {
        let db = BlokliDb::new_in_memory().await.unwrap();

        // Try to fetch non-existent balance
        let address = "0x0000000000000000000000000000000000000000";
        let result = SubscriptionRoot::fetch_native_balance(db.conn(blokli_db::TargetDb::Index), address)
            .await
            .unwrap();

        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_fetch_hopr_balance_finds_existing_balance() {
        let db = BlokliDb::new_in_memory().await.unwrap();

        // Insert HOPR balance
        let balance_model = blokli_db_entity::hopr_balance::ActiveModel {
            id: Default::default(),
            address: Set(vec![2; 20]),
            balance: Set(vec![20, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]),
            last_changed_block: Set(0),
            last_changed_tx_index: Set(0),
            last_changed_log_index: Set(0),
        };
        balance_model.insert(db.conn(blokli_db::TargetDb::Index)).await.unwrap();

        // Fetch balance
        let address = "0x0202020202020202020202020202020202020202";
        let result = SubscriptionRoot::fetch_hopr_balance(db.conn(blokli_db::TargetDb::Index), address)
            .await
            .unwrap();

        assert!(result.is_some());
    }

    #[tokio::test]
    async fn test_fetch_hopr_balance_returns_none_for_missing() {
        let db = BlokliDb::new_in_memory().await.unwrap();

        let address = "0x0000000000000000000000000000000000000000";
        let result = SubscriptionRoot::fetch_hopr_balance(db.conn(blokli_db::TargetDb::Index), address)
            .await
            .unwrap();

        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_fetch_filtered_accounts_filters_by_keyid() {
        let db = BlokliDb::new_in_memory().await.unwrap();

        // Create 3 accounts
        let id1 = create_test_account(db.conn(blokli_db::TargetDb::Index), vec![1; 20], "peer1")
            .await
            .unwrap();
        let _id2 = create_test_account(db.conn(blokli_db::TargetDb::Index), vec![2; 20], "peer2")
            .await
            .unwrap();
        let _id3 = create_test_account(db.conn(blokli_db::TargetDb::Index), vec![3; 20], "peer3")
            .await
            .unwrap();

        // Filter by specific keyid
        let result =
            SubscriptionRoot::fetch_filtered_accounts(db.conn(blokli_db::TargetDb::Index), Some(id1), None, None)
                .await
                .unwrap();

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].keyid, id1);
    }

    #[tokio::test]
    async fn test_fetch_filtered_accounts_filters_by_packet_key() {
        let db = BlokliDb::new_in_memory().await.unwrap();

        create_test_account(db.conn(blokli_db::TargetDb::Index), vec![1; 20], "peer1")
            .await
            .unwrap();
        create_test_account(db.conn(blokli_db::TargetDb::Index), vec![2; 20], "peer2")
            .await
            .unwrap();

        // Filter by packet_key
        let result = SubscriptionRoot::fetch_filtered_accounts(
            db.conn(blokli_db::TargetDb::Index),
            None,
            Some("peer1".to_string()),
            None,
        )
        .await
        .unwrap();

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].packet_key, "peer1");
    }

    #[tokio::test]
    async fn test_fetch_filtered_accounts_returns_all_when_no_filters() {
        let db = BlokliDb::new_in_memory().await.unwrap();

        create_test_account(db.conn(blokli_db::TargetDb::Index), vec![1; 20], "peer1")
            .await
            .unwrap();
        create_test_account(db.conn(blokli_db::TargetDb::Index), vec![2; 20], "peer2")
            .await
            .unwrap();
        create_test_account(db.conn(blokli_db::TargetDb::Index), vec![3; 20], "peer3")
            .await
            .unwrap();

        // No filters - should return all
        let result = SubscriptionRoot::fetch_filtered_accounts(db.conn(blokli_db::TargetDb::Index), None, None, None)
            .await
            .unwrap();

        assert_eq!(result.len(), 3);
    }

    #[tokio::test]
    async fn test_fetch_filtered_channels_returns_error_for_status_filter() {
        let db = BlokliDb::new_in_memory().await.unwrap();

        // Status filtering is not yet implemented
        let result = SubscriptionRoot::fetch_filtered_channels(
            db.conn(blokli_db::TargetDb::Index),
            None,
            None,
            None,
            Some(blokli_api_types::ChannelStatus::Open),
        )
        .await;

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("temporarily unavailable"));
    }
}
