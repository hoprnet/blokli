//! GraphQL subscription root and resolver implementations

use std::{collections::HashMap, sync::Arc, time::Duration};

use async_broadcast::Receiver;
use async_graphql::{Context, ID, Result, Subscription};
use async_stream::stream;
use blokli_api_types::{
    Account, Channel, ChannelUpdate, Hex32, OpenedChannelsGraphEntry, Safe, TicketParameters, TokenValueString,
    Transaction, TransactionStatus as GqlTransactionStatus, UInt64,
};
use blokli_chain_api::transaction_store::{TransactionStatus as StoreTransactionStatus, TransactionStore};
use blokli_chain_indexer::{IndexerState, state::IndexerEvent};
use blokli_db_entity::{
    chain_info,
    chain_info::Entity as ChainInfoEntity,
    channel, channel_state,
    conversions::{
        account_aggregation::{fetch_accounts_by_keyids, fetch_accounts_with_filters},
        channel_aggregation::fetch_channels_with_state,
    },
    hopr_node_safe_registration,
};
use chrono::Utc;
use futures::Stream;
use hopr_primitive_types::{
    prelude::HoprBalance as PrimitiveHoprBalance,
    primitives::Address,
    traits::{IntoEndian, ToHex},
};
use sea_orm::{
    ColumnTrait, Condition, ConnectionTrait, DatabaseBackend, DatabaseConnection, EntityTrait, QueryFilter, QueryOrder,
    Statement,
};
use tokio::time::sleep;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

use crate::errors;

#[derive(Debug)]
struct SafeContractCurrentRow {
    address: Vec<u8>,
    module_address: Vec<u8>,
    chain_key: Vec<u8>,
}

fn current_row_statement(backend: DatabaseBackend, value: Vec<u8>) -> Statement {
    let placeholder = if backend == DatabaseBackend::Postgres {
        "$1"
    } else {
        "?"
    };
    let sql = format!(
        "SELECT address, module_address, chain_key FROM safe_contract_current WHERE address = {}",
        placeholder
    );
    Statement::from_sql_and_values(backend, sql, vec![value.into()])
}

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
///
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
) -> Result<(Watermark, Receiver<IndexerEvent>, Receiver<()>)> {
    // Acquire read lock - this waits for any in-progress block processing to complete
    let _lock = indexer_state.acquire_watermark_lock().await;

    // Query the current watermark from chain_info table
    let chain_info = ChainInfoEntity::find()
        .one(db)
        .await
        .map_err(|e| async_graphql::Error::new(errors::messages::query_error("chain_info query", e)))?
        .ok_or_else(|| async_graphql::Error::new(errors::messages::not_found("chain_info", "not initialized")))?;

    let watermark = Watermark {
        block: chain_info.last_indexed_block,
        tx_index: chain_info.last_indexed_tx_index.unwrap_or(0),
        log_index: chain_info.last_indexed_log_index.unwrap_or(0),
    };

    // Subscribe to event bus and shutdown signal while still holding the lock
    // This ensures no events can be published between reading watermark and subscribing
    let event_receiver = indexer_state.subscribe_to_events();
    let shutdown_receiver = indexer_state.subscribe_to_shutdown();

    // Lock is automatically released when _lock goes out of scope
    Ok((watermark, event_receiver, shutdown_receiver))
}

/// Queries all open channels at a specific watermark
///
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
    _batch_size: usize,
) -> Result<Vec<ChannelUpdate>> {
    // Query all channels (identity table has no temporal component)
    let channels = channel::Entity::find()
        .all(db)
        .await
        .map_err(|e| async_graphql::Error::new(errors::messages::query_error("channels query", e)))?;

    if channels.is_empty() {
        return Ok(Vec::new());
    }

    let channel_ids: Vec<i64> = channels.iter().map(|c| c.id).collect();

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
        .map_err(|e| async_graphql::Error::new(errors::messages::query_error("channel_state query", e)))?;

    // Build map of channel_id -> latest state (first occurrence due to ordering)
    let mut state_map: HashMap<i64, channel_state::Model> = HashMap::new();
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
    let mut account_ids: Vec<i64> = open_channels
        .iter()
        .flat_map(|c| vec![c.source, c.destination])
        .collect();
    account_ids.sort_unstable();
    account_ids.dedup();

    // Fetch accounts using the existing aggregation function
    let accounts_result = fetch_accounts_by_keyids(db, account_ids.clone())
        .await
        .map_err(|e| async_graphql::Error::new(errors::messages::query_error("accounts fetch", e)))?;

    // Build account map for quick lookup
    let account_map: HashMap<i64, blokli_db_entity::conversions::account_aggregation::AggregatedAccount> =
        accounts_result.into_iter().map(|a| (a.keyid, a)).collect();

    // Build ChannelUpdate objects
    let mut results = Vec::new();
    for channel in open_channels {
        let state = state_map.get(&channel.id).unwrap(); // Safe because we filtered by state presence

        let source_account = account_map
            .get(&channel.source)
            .ok_or_else(|| async_graphql::Error::new(errors::messages::not_found("Source account", channel.source)))?;

        let dest_account = account_map.get(&channel.destination).ok_or_else(|| {
            async_graphql::Error::new(errors::messages::not_found("Destination account", channel.destination))
        })?;

        // Convert to GraphQL types
        let channel_gql = Channel {
            concrete_channel_id: channel.concrete_channel_id,
            source: channel.source,
            destination: channel.destination,
            balance: TokenValueString(PrimitiveHoprBalance::from_be_bytes(&state.balance).to_string()),
            status: state.status.into(),
            epoch: i32::try_from(state.epoch).map_err(|e| {
                async_graphql::Error::new(errors::messages::conversion_error(
                    "i64",
                    "i32",
                    format!("{} ({})", state.epoch, e),
                ))
            })?,
            ticket_index: blokli_api_types::UInt64(u64::try_from(state.ticket_index).map_err(|e| {
                async_graphql::Error::new(errors::messages::conversion_error(
                    "i64",
                    "u64",
                    format!("{} ({})", state.ticket_index, e),
                ))
            })?),
            closure_time: state.closure_time.map(|time| time.with_timezone(&Utc)),
        };

        let source_gql = Account {
            keyid: source_account.keyid,
            chain_key: source_account.chain_key.clone(),
            packet_key: source_account.packet_key.clone(),
            safe_address: source_account.safe_address.clone(),
            multi_addresses: source_account.multi_addresses.clone(),
        };

        let dest_gql = Account {
            keyid: dest_account.keyid,
            chain_key: dest_account.chain_key.clone(),
            packet_key: dest_account.packet_key.clone(),
            safe_address: dest_account.safe_address.clone(),
            multi_addresses: dest_account.multi_addresses.clone(),
        };

        results.push(ChannelUpdate {
            channel: channel_gql,
            source: source_gql,
            destination: dest_gql,
        });
    }

    Ok(results)
}

/// fetch_channel_update is no longer needed - events now contain complete data
/// Root subscription type providing real-time updates via Server-Sent Events (SSE)
pub struct SubscriptionRoot;

#[Subscription]
impl SubscriptionRoot {
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

    /// Subscribe to the opened payment channels graph with real-time updates
    ///
    /// **Streaming Behavior:**
    /// - Emits one OpenedChannelsGraphEntry per open channel
    /// - Each entry contains a single channel with its source and destination accounts
    /// - On subscription start, emits all existing open channels as separate entries
    /// - Subsequently, emits updates when any channel changes
    ///
    /// **Building the Graph:**
    /// Clients receive entries incrementally (one per channel) and should accumulate
    /// them to build the complete network topology. The full graph is the union of all entries.
    ///
    /// **Update Triggers:**
    /// An entry is re-emitted for a channel when:
    /// - The channel's status changes (e.g., OPEN -> PENDINGTOCLOSE)
    /// - The channel's balance changes
    /// - The channel closes (no longer emitted)
    /// - A new channel opens (new entry emitted)
    ///
    /// **Example:**
    /// If the network has three open channels: channelA (A->B), channelB (B->A), channelC (A->C),
    /// the subscription emits three separate OpenedChannelsGraphEntry objects, each containing
    /// one channel with its source and destination accounts.
    ///
    /// **Note:** This is a directed graph. Bidirectional communication requires
    /// channels in both directions, each emitted as a separate entry.
    #[graphql(name = "openedChannelGraphUpdated")]
    async fn opened_channel_graph_updated(
        &self,
        ctx: &Context<'_>,
    ) -> Result<impl Stream<Item = OpenedChannelsGraphEntry>> {
        // Get dependencies from context
        let db = ctx.data::<DatabaseConnection>()?.clone();
        let indexer_state = ctx
            .data::<IndexerState>()
            .map_err(|e| async_graphql::Error::new(errors::messages::context_error("IndexerState", e.message)))?
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
                        yield channel_update_to_graph_entry(channel_update);
                    }
                }
                Err(e) => {
                    error!("Failed to query historical channels: {:?}", e);
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
                                info!("Subscription shutting down due to reorg");
                                return; // Terminate subscription on reorg
                            }
                            Err(async_broadcast::RecvError::Closed) => {
                                warn!("Shutdown channel closed");
                                return;
                            }
                            Err(async_broadcast::RecvError::Overflowed(n)) => {
                                warn!("Shutdown signal overflowed, missed {} signals", n);
                                // Continue - overflow on shutdown signal is not critical
                            }
                        }
                    }

                    // Receive event from event bus
                    event_result = event_receiver.recv() => {
                        match event_result {
                            Ok(IndexerEvent::ChannelUpdated(channel_update)) => {
                                // Convert from ChannelUpdate (internal) to OpenedChannelsGraphEntry (API)
                                yield channel_update_to_graph_entry(*channel_update);
                            }
                            Ok(IndexerEvent::AccountUpdated(_)) => {
                                // Account updates don't affect this subscription
                                // Just continue to next event
                            }
                            Ok(IndexerEvent::KeyBindingFeeUpdated(_)) => {
                                // Key binding fee updates don't affect this subscription
                                // Just continue to next event
                            }
                            Ok(IndexerEvent::SafeDeployed(_)) => {
                                // Safe deployment doesn't affect this subscription
                            }
                            Ok(IndexerEvent::TicketParametersUpdated(_)) => {
                                // Ticket parameter updates don't affect this subscription
                            }
                            Err(async_broadcast::RecvError::Closed) => {
                                info!("Event bus closed, ending subscription");
                                return;
                            }
                            Err(async_broadcast::RecvError::Overflowed(n)) => {
                                warn!("Event bus overflowed, missed {} events - consider increasing buffer", n);
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
    ///
    /// Uses the IndexerState event bus for real-time notifications:
    /// - Emits matching accounts on subscription start
    /// - Streams updates when `IndexerEvent::AccountUpdated` events are received
    /// - Automatically shuts down on blockchain reorganization
    #[graphql(name = "accountUpdated")]
    async fn account_updated(
        &self,
        ctx: &Context<'_>,
        #[graphql(desc = "Filter by account keyid")] keyid: Option<i64>,
        #[graphql(desc = "Filter by packet key (peer ID format)")] packet_key: Option<String>,
        #[graphql(desc = "Filter by chain key (hexadecimal format)")] chain_key: Option<String>,
    ) -> Result<impl Stream<Item = Account>> {
        let db = ctx.data::<DatabaseConnection>()?.clone();
        let indexer_state = ctx
            .data::<IndexerState>()
            .map_err(|e| async_graphql::Error::new(errors::messages::context_error("IndexerState", e.message)))?
            .clone();

        Ok(stream! {
            // Subscribe to events and shutdown first to avoid missing events
            let mut event_receiver = indexer_state.subscribe_to_events();
            let mut shutdown_receiver = indexer_state.subscribe_to_shutdown();

            // Emit initial matching accounts
            match Self::fetch_filtered_accounts(&db, keyid, packet_key.clone(), chain_key.clone()).await {
                Ok(accounts) => {
                    for account in accounts {
                        yield account;
                    }
                }
                Err(e) => {
                    error!("Failed to fetch initial accounts: {:?}", e);
                }
            }

            // Stream updates from event bus
            loop {
                tokio::select! {
                    shutdown_result = shutdown_receiver.recv() => {
                        match shutdown_result {
                            Ok(_) => {
                                info!("accountUpdated subscription shutting down due to reorg");
                                return;
                            }
                            Err(async_broadcast::RecvError::Closed) => {
                                warn!("Shutdown channel closed for accountUpdated");
                                return;
                            }
                            Err(async_broadcast::RecvError::Overflowed(n)) => {
                                warn!("Shutdown signal overflowed ({}), continuing", n);
                            }
                        }
                    }
                    event_result = event_receiver.recv() => {
                        match event_result {
                            Ok(IndexerEvent::AccountUpdated(account)) => {
                                // Apply filters
                                if matches_account_filters(&account, keyid, packet_key.as_deref(), chain_key.as_deref()) {
                                    yield account;
                                }
                            }
                            Ok(_) => {
                                // Other events (ChannelUpdated, etc.) - ignore
                            }
                            Err(async_broadcast::RecvError::Closed) => {
                                info!("Event bus closed, ending accountUpdated subscription");
                                return;
                            }
                            Err(async_broadcast::RecvError::Overflowed(n)) => {
                                warn!("Event bus overflowed ({}); accountUpdated may miss events", n);
                            }
                        }
                    }
                }
            }
        })
    }

    /// Subscribe to real-time updates of ticket price and winning probability
    ///
    /// Provides updates whenever there is a change in the ticket price or minimum
    /// winning probability on-chain. These values are essential for ticket validation
    /// and payment channel operation.
    ///
    /// Uses the IndexerState event bus for real-time notifications:
    /// - Emits current value on subscription start
    /// - Streams updates when TicketParametersUpdated events are received
    /// - Automatically shuts down on blockchain reorganization
    #[graphql(name = "ticketParametersUpdated")]
    async fn ticket_parameters_updated(&self, ctx: &Context<'_>) -> Result<impl Stream<Item = TicketParameters>> {
        let db = ctx.data::<DatabaseConnection>()?.clone();
        let indexer_state = ctx
            .data::<IndexerState>()
            .map_err(|e| async_graphql::Error::new(errors::messages::context_error("IndexerState", e.message)))?
            .clone();

        Ok(stream! {
            // Create subscription channels early to avoid missing events between initial fetch and loop
            let mut event_receiver = indexer_state.subscribe_to_events();
            let mut shutdown_receiver = indexer_state.subscribe_to_shutdown();

            // Track last emitted value to avoid duplicate emissions
            let mut last_params: Option<TicketParameters> = None;

            // Emit current value from DB (if available)
            match Self::fetch_ticket_parameters(&db).await {
                Ok(Some(params)) => {
                    last_params = Some(params.clone());
                    yield params;
                }
                Ok(None) => {
                    warn!("Ticket parameters not initialized when subscribing to ticketParametersUpdated");
                }
                Err(e) => {
                    error!("Failed to fetch ticket parameters: {:?}", e);
                }
            }

            // Stream real-time updates from event bus
            loop {
                tokio::select! {
                    shutdown_result = shutdown_receiver.recv() => {
                        match shutdown_result {
                            Ok(_) => {
                                info!("ticketParametersUpdated subscription shutting down due to reorg");
                                return;
                            }
                            Err(async_broadcast::RecvError::Closed) => {
                                warn!("Shutdown channel closed for ticketParametersUpdated");
                                return;
                            }
                            Err(async_broadcast::RecvError::Overflowed(n)) => {
                                warn!("Shutdown signal overflowed ({}), continuing", n);
                            }
                        }
                    }
                    event_result = event_receiver.recv() => {
                        match event_result {
                            Ok(IndexerEvent::TicketParametersUpdated(params)) => {
                                // Only emit if value changed
                                if last_params.as_ref() != Some(&params) {
                                    last_params = Some(params.clone());
                                    yield params;
                                }
                            }
                            Ok(IndexerEvent::AccountUpdated(_)) => {
                                // Irrelevant for this subscription
                            }
                            Ok(IndexerEvent::ChannelUpdated(_)) => {
                                // Irrelevant for this subscription
                            }
                            Ok(IndexerEvent::KeyBindingFeeUpdated(_)) => {
                                // Irrelevant for this subscription
                            }
                            Ok(IndexerEvent::SafeDeployed(_)) => {
                                // Irrelevant for this subscription
                            }
                            Err(async_broadcast::RecvError::Closed) => {
                                info!("Event bus closed, ending ticketParametersUpdated subscription");
                                return;
                            }
                            Err(async_broadcast::RecvError::Overflowed(n)) => {
                                warn!("Event bus overflowed ({}); ticketParametersUpdated may miss events", n);
                            }
                        }
                    }
                }
            }
        })
    }

    /// Streams updates to the key binding fee.
    ///
    /// Emits the current fee once when the subscription starts, then emits new fee
    /// values whenever a `KeyBindingFeeUpdated` event is processed while the indexer
    /// is synced. Consecutive duplicate fee values are suppressed.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use futures::StreamExt;
    ///
    /// // In an async context with a GraphQL `Context` available:
    /// // let stream = root.key_binding_fee_updated(&ctx).await.unwrap();
    /// // let mut stream = Box::pin(stream);
    /// // if let Some(fee) = stream.next().await {
    /// //     println!("current fee: {}", fee.0);
    /// // }
    /// ```
    #[graphql(name = "keyBindingFeeUpdated")]
    async fn key_binding_fee_updated(&self, ctx: &Context<'_>) -> Result<impl Stream<Item = TokenValueString>> {
        let db = ctx.data::<DatabaseConnection>()?.clone();
        let indexer_state = ctx
            .data::<IndexerState>()
            .map_err(|e| async_graphql::Error::new(errors::messages::context_error("IndexerState", e.message)))?
            .clone();

        Ok(stream! {
            // Create subscription channels early to avoid missing events between initial fetch and loop
            let mut event_receiver = indexer_state.subscribe_to_events();
            let mut shutdown_receiver = indexer_state.subscribe_to_shutdown();

            // Track last value to avoid duplicate emissions
            let mut last_fee: Option<TokenValueString> = None;

            // Emit current value from DB (if available)
            match chain_info::Entity::find().one(&db).await {
                Ok(Some(info)) => {
                    if let Some(bytes) = info.key_binding_fee.as_ref() {
                        let fee_str = PrimitiveHoprBalance::from_be_bytes(bytes).to_string();
                        let current = TokenValueString(fee_str);
                        last_fee = Some(current.clone());
                        yield current;
                    }
                }
                Ok(None) => {
                    warn!("chain_info not initialized when subscribing to keyBindingFeeUpdated");
                }
                Err(e) => {
                    error!("Failed to fetch chain_info for keyBindingFeeUpdated: {:?}", e);
                }
            }

            loop {
                tokio::select! {
                    shutdown_result = shutdown_receiver.recv() => {
                        match shutdown_result {
                            Ok(_) => {
                                info!("keyBindingFeeUpdated subscription shutting down due to reorg");
                                return;
                            }
                            Err(async_broadcast::RecvError::Closed) => {
                                warn!("Shutdown channel closed for keyBindingFeeUpdated");
                                return;
                            }
                            Err(async_broadcast::RecvError::Overflowed(n)) => {
                                warn!("Shutdown signal overflowed ({}), continuing", n);
                            }
                        }
                    }
                    event_result = event_receiver.recv() => {
                        match event_result {
                            Ok(IndexerEvent::KeyBindingFeeUpdated(fee)) => {
                                if last_fee.as_ref() != Some(&fee) {
                                    last_fee = Some(fee.clone());
                                    yield fee;
                                }
                            }
                            Ok(IndexerEvent::AccountUpdated(_)) => {
                                // Irrelevant for this subscription
                            }
                            Ok(IndexerEvent::ChannelUpdated(_)) => {
                                // Irrelevant for this subscription
                            }
                            Ok(IndexerEvent::SafeDeployed(_)) => {
                                // Irrelevant for this subscription
                            }
                            Ok(IndexerEvent::TicketParametersUpdated(_)) => {
                                // Irrelevant for this subscription
                            }
                            Err(async_broadcast::RecvError::Closed) => {
                                info!("Event bus closed, ending keyBindingFeeUpdated subscription");
                                return;
                            }
                            Err(async_broadcast::RecvError::Overflowed(n)) => {
                                warn!("Event bus overflowed ({}); keyBindingFeeUpdated may miss events", n);
                            }
                        }
                    }
                }
            }
        })
    }

    /// Streams newly deployed safes as `Safe` objects.
    ///
    /// The stream yields a `Safe` for each `SafeDeployed` event observed by the indexer.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// # use futures::StreamExt;
    /// // `root` is a `SubscriptionRoot` and `ctx` is an `async_graphql::Context<'_>`
    /// let mut stream = root.safe_deployed(&ctx).await.unwrap();
    /// while let Some(safe) = stream.next().await {
    ///     println!("{}", safe.address);
    /// }
    /// ```
    #[graphql(name = "safeDeployed")]
    async fn safe_deployed(&self, ctx: &Context<'_>) -> Result<impl Stream<Item = Safe>> {
        debug!("safeDeployed subscription");
        let db = ctx.data::<DatabaseConnection>()?.clone();
        let indexer_state = ctx
            .data::<IndexerState>()
            .map_err(|e| async_graphql::Error::new(errors::messages::context_error("IndexerState", e.message)))?
            .clone();

        // Capture watermark and subscribe to event bus (synchronized)
        let (_watermark, mut event_receiver, mut shutdown_receiver) =
            capture_watermark_synchronized(&indexer_state, &db).await?;

        debug!("subscribed to event bus for safeDeployed");

        Ok(stream! {
            loop {
                tokio::select! {
                    shutdown_result = shutdown_receiver.recv() => {
                        match shutdown_result {
                            Ok(_) => {
                                info!("Subscription shutting down due to reorg");
                                return;
                            }
                            Err(async_broadcast::RecvError::Closed) => {
                                warn!("Shutdown channel closed");
                                return;
                            }
                            Err(async_broadcast::RecvError::Overflowed(n)) => {
                                warn!("Shutdown signal overflowed, missed {} signals", n);
                                // Continue - overflow on shutdown signal is not critical
                            }
                        }
                    }
                    event_result = event_receiver.recv() => {
                        debug!(?event_result, "received an event in safe_deployed subscription");
                        match event_result {
                            Ok(IndexerEvent::SafeDeployed(safe_addr)) => {
                                let safe_addr_bytes = safe_addr.as_ref().to_vec();
                                debug!(?safe_addr_bytes, "safe address");
                                let stmt = current_row_statement(db.get_database_backend(), safe_addr_bytes);
                                let row = match db.query_one_raw(stmt).await {
                                    Ok(Some(row)) => row,
                                    Ok(None) => {
                                        error!("Safe deployed event received but safe not found in DB: {}", safe_addr);
                                        continue;
                                    }
                                    Err(e) => {
                                        error!("Failed to query safe for deployed event: {}", e);
                                        continue;
                                    }
                                };

                                let address: Vec<u8> = match row.try_get("", "address") {
                                    Ok(value) => value,
                                    Err(e) => {
                                        error!("Failed to parse safe address from view: {}", e);
                                        continue;
                                    }
                                };
                                let module_address: Vec<u8> = match row.try_get("", "module_address") {
                                    Ok(value) => value,
                                    Err(e) => {
                                        error!("Failed to parse module address from view: {}", e);
                                        continue;
                                    }
                                };
                                let chain_key: Vec<u8> = match row.try_get("", "chain_key") {
                                    Ok(value) => value,
                                    Err(e) => {
                                        error!("Failed to parse chain key from view: {}", e);
                                        continue;
                                    }
                                };

                                let current = SafeContractCurrentRow {
                                    address,
                                    module_address,
                                    chain_key,
                                };

                                debug!(safe_address = ?current.address, "processing SafeDeployed event");
                                let registered_nodes = match hopr_node_safe_registration::Entity::find()
                                    .filter(hopr_node_safe_registration::Column::SafeAddress.eq(current.address.clone()))
                                    .all(&db)
                                    .await
                                {
                                    Ok(registrations) => registrations
                                        .into_iter()
                                        .filter_map(|reg| Address::try_from(reg.node_address.as_slice()).ok())
                                        .map(|addr| addr.to_hex())
                                        .collect(),
                                    Err(e) => {
                                        warn!(
                                            safe_address = ?current.address,
                                            error = %e,
                                            "Failed to fetch registered nodes for safe, returning empty list"
                                        );
                                        Vec::new()
                                    }
                                };
                                debug!(?current, "yielding Safe from SafeDeployed event");
                                yield Safe {
                                    address: Address::new(&current.address).to_hex(),
                                    module_address: Address::new(&current.module_address).to_hex(),
                                    chain_key: Address::new(&current.chain_key).to_hex(),
                                    registered_nodes,
                                };
                            }
                            Ok(_) => {}
                            Err(async_broadcast::RecvError::Closed) => {
                                info!("Event bus closed, ending subscription");
                                return;
                            }
                            Err(async_broadcast::RecvError::Overflowed(n)) => {
                                warn!("Event bus overflowed, missed {} events - consider increasing buffer", n);
                                // Continue but log warning - client may need to reconnect
                            }
                        }
                    }
                }
            }
        })
    }

    /// Subscribe to real-time updates of a specific transaction
    ///
    /// Provides updates whenever the status of the specified transaction changes,
    /// including validation, submission, confirmation, revert, and failure events.
    #[graphql(name = "transactionUpdated")]
    async fn transaction_updated(
        &self,
        ctx: &Context<'_>,
        #[graphql(desc = "Transaction ID to monitor (UUID)")] id: ID,
    ) -> Result<impl Stream<Item = Transaction>> {
        let transaction_id = Uuid::parse_str(id.as_str()).map_err(|e| {
            async_graphql::Error::new(format!("Invalid transaction ID format: {}. Expected UUID format.", e))
        })?;

        let transaction_store = ctx.data::<Arc<TransactionStore>>()?.clone();

        Ok(stream! {
            // Track last status to emit only when it changes
            let mut last_status: Option<StoreTransactionStatus> = None;

            loop {
                // Poll the transaction store periodically
                // TODO: use event-driven approach
                sleep(Duration::from_millis(100)).await;

                // Try to get the transaction from the store
                if let Ok(record) = transaction_store.get(transaction_id) {
                    // Only emit if status has changed
                    if last_status.as_ref() != Some(&record.status) {
                        last_status = Some(record.status);

                        // Convert store transaction status to GraphQL status
                        let gql_status = match record.status {
                            StoreTransactionStatus::Pending => GqlTransactionStatus::Pending,
                            StoreTransactionStatus::Submitted => GqlTransactionStatus::Submitted,
                            StoreTransactionStatus::Confirmed => GqlTransactionStatus::Confirmed,
                            StoreTransactionStatus::Reverted => GqlTransactionStatus::Reverted,
                            StoreTransactionStatus::Timeout => GqlTransactionStatus::Timeout,
                            StoreTransactionStatus::ValidationFailed => GqlTransactionStatus::ValidationFailed,
                            StoreTransactionStatus::SubmissionFailed => GqlTransactionStatus::SubmissionFailed,
                        };

                        // Convert transaction hash to hex string
                        let hash_hex = record.transaction_hash.to_hex();

                        yield Transaction {
                            id: ID::from(record.id.to_string()),
                            status: gql_status,
                            submitted_at: record.submitted_at,
                            transaction_hash: Hex32(hash_hex),
                        };
                    }
                }
                // If transaction not found, continue polling (it might be added later)
            }
        })
    }
}

/// Convert ChannelUpdate (from events) to OpenedChannelsGraphEntry (for GraphQL)
fn channel_update_to_graph_entry(update: ChannelUpdate) -> OpenedChannelsGraphEntry {
    OpenedChannelsGraphEntry {
        channel: update.channel,
        source: update.source,
        destination: update.destination,
    }
}

// Helper methods for fetching data
impl SubscriptionRoot {
    async fn fetch_ticket_parameters(db: &DatabaseConnection) -> Result<Option<TicketParameters>, sea_orm::DbErr> {
        let chain_info = chain_info::Entity::find().one(db).await?;

        Ok(chain_info.map(|info| {
            let ticket_price = if let Some(price_bytes) = info.ticket_price {
                PrimitiveHoprBalance::from_be_bytes(&price_bytes).to_string()
            } else {
                "0 wxHOPR".to_string()
            };

            TicketParameters {
                min_ticket_winning_probability: info.min_incoming_ticket_win_prob as f64,
                ticket_price: TokenValueString(ticket_price),
            }
        }))
    }

    async fn fetch_filtered_channels(
        db: &DatabaseConnection,
        source_key_id: Option<i32>,
        destination_key_id: Option<i32>,
        concrete_channel_id: Option<String>,
        status: Option<blokli_api_types::ChannelStatus>,
    ) -> Result<Vec<Channel>, sea_orm::DbErr> {
        // Convert i32 keyids to i64 for channel aggregation function
        let source_i64 = source_key_id.map(|k| k as i64);
        let dest_i64 = destination_key_id.map(|k| k as i64);

        // Convert GraphQL ChannelStatus to internal status code (i16)
        let status_code = status.map(|s| match s {
            blokli_api_types::ChannelStatus::Closed => 0i16,
            blokli_api_types::ChannelStatus::Open => 1i16,
            blokli_api_types::ChannelStatus::PendingToClose => 2i16,
        });

        // Use optimized channel aggregation function
        let aggregated_channels =
            fetch_channels_with_state(db, source_i64, dest_i64, concrete_channel_id, status_code).await?;

        // Convert to GraphQL Channel type
        let channels: Vec<Channel> = aggregated_channels
            .iter()
            .map(|agg| -> Result<Channel, sea_orm::DbErr> {
                Ok(Channel {
                    concrete_channel_id: agg.concrete_channel_id.clone(),
                    source: agg.source,
                    destination: agg.destination,
                    balance: TokenValueString(agg.balance.clone()),
                    status: agg.status.into(),
                    epoch: i32::try_from(agg.epoch).map_err(|e| {
                        sea_orm::DbErr::Custom(errors::messages::conversion_error(
                            "i64",
                            "i32",
                            format!("{} ({})", agg.epoch, e),
                        ))
                    })?,
                    ticket_index: UInt64(u64::try_from(agg.ticket_index).map_err(|e| {
                        sea_orm::DbErr::Custom(errors::messages::conversion_error(
                            "i64",
                            "u64",
                            format!("{} ({})", agg.ticket_index, e),
                        ))
                    })?),
                    closure_time: agg.closure_time,
                })
            })
            .collect::<Result<Vec<_>, _>>()?;

        Ok(channels)
    }

    async fn fetch_filtered_accounts(
        db: &DatabaseConnection,
        keyid: Option<i64>,
        packet_key: Option<String>,
        chain_key: Option<String>,
    ) -> Result<Vec<Account>, sea_orm::DbErr> {
        // Use optimized batch loading from account_aggregation
        let aggregated_accounts = fetch_accounts_with_filters(db, keyid, packet_key, chain_key).await?;

        // Convert to GraphQL Account type
        let result = aggregated_accounts
            .into_iter()
            .map(|agg| Account {
                keyid: agg.keyid,
                chain_key: agg.chain_key,
                packet_key: agg.packet_key,
                safe_address: agg.safe_address,
                multi_addresses: agg.multi_addresses,
            })
            .collect();

        Ok(result)
    }
}

/// Helper to check if an account matches the subscription filters
///
/// This is used to filter events from the broadcast channel to only emit
/// accounts that match the subscription's filter criteria.
fn matches_account_filters(
    account: &Account,
    keyid: Option<i64>,
    packet_key: Option<&str>,
    chain_key: Option<&str>,
) -> bool {
    if let Some(k) = keyid
        && account.keyid != k
    {
        return false;
    }
    if let Some(pk) = packet_key {
        // Strip 0x prefix if present
        let pk_normalized = pk.strip_prefix("0x").unwrap_or(pk);
        if account.packet_key != pk_normalized {
            return false;
        }
    }
    if let Some(ck) = chain_key {
        // Normalize to canonical hex format
        match Address::from_hex(ck) {
            Ok(addr) => {
                if account.chain_key != addr.to_hex() {
                    return false;
                }
            }
            Err(_) => {
                // Invalid address format cannot match
                return false;
            }
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use async_graphql::{EmptyMutation, Object, Schema};
    use blokli_chain_indexer::state::IndexerEvent;
    use blokli_db::{BlokliDbGeneralModelOperations, db::BlokliDb};
    use blokli_db_entity::{hopr_safe_contract, hopr_safe_contract_state};
    use futures::StreamExt;
    use sea_orm::{ActiveModelTrait, Set};

    use super::*;

    // Dummy Query root
    #[derive(Default)]
    struct DummyQuery;
    #[Object]
    impl DummyQuery {
        async fn dummy(&self) -> bool {
            true
        }
    }

    // Helper: Create test channel in database
    async fn create_test_channel(
        db: &DatabaseConnection,
        source: i64,
        destination: i64,
        concrete_id: &str,
    ) -> anyhow::Result<i64> {
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
    async fn create_test_account(db: &DatabaseConnection, chain_key: Vec<u8>, packet_key: &str) -> anyhow::Result<i64> {
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
        channel_id: i64,
        block: i64,
        tx_index: i64,
        log_index: i64,
        balance: Vec<u8>,
        status: i16,
    ) -> anyhow::Result<i64> {
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
        // Delete any existing chain_info to ensure test isolation
        chain_info::Entity::delete_many().exec(db).await?;

        let chain_info = chain_info::ActiveModel {
            id: Set(1),
            last_indexed_block: Set(block),
            last_indexed_tx_index: Set(Some(tx_index)),
            last_indexed_log_index: Set(Some(log_index)),
            ticket_price: Set(None),
            key_binding_fee: Set(None),
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

        // Create a test account for the event
        let test_account = Account {
            keyid: 1,
            chain_key: "0x1111".to_string(),
            packet_key: "peer1".to_string(),
            safe_address: None,
            multi_addresses: vec![],
        };

        // Publish event to bus
        indexer_state.publish_event(IndexerEvent::AccountUpdated(test_account.clone()));

        // Verify subscriber receives the event
        let received = event_rx.recv().await.unwrap();
        assert!(matches!(received, IndexerEvent::AccountUpdated(_)));
    }

    #[tokio::test]
    async fn test_capture_watermark_fails_when_chain_info_missing() {
        let db = BlokliDb::new_in_memory().await.unwrap();
        let indexer_state = IndexerState::new(10, 100);

        // Ensure chain_info does NOT exist (delete any from previous tests)
        chain_info::Entity::delete_many()
            .exec(db.conn(blokli_db::TargetDb::Index))
            .await
            .unwrap();

        // Do NOT initialize chain_info - should fail
        let result = capture_watermark_synchronized(&indexer_state, db.conn(blokli_db::TargetDb::Index)).await;

        assert!(result.is_err());
        let error = result.unwrap_err();
        // Check that the error message contains expected content (from error_builders::not_found)
        assert!(
            error.message.contains("chain_info") && error.message.contains("not found"),
            "Expected error about chain_info not found, got: {}",
            error.message
        );
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
        // Note: Balance bytes must be big-endian (most significant byte first)
        insert_channel_state(
            db.conn(blokli_db::TargetDb::Index),
            channel_id,
            50,
            0,
            0,
            vec![0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1], // balance = 1 (big-endian)
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
            vec![0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 2], // balance = 2 (updated, big-endian)
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

        // Should return channel with latest balance (2 wei, not 2 HOPR)
        assert_eq!(result.len(), 1);
        // Balance is 2 wei with token identifier
        assert_eq!(result[0].channel.balance.0, "0.000000000000000002 wxHOPR");
    }

    #[tokio::test]
    async fn test_query_channels_at_watermark_returns_all_channels() {
        let db = BlokliDb::new_in_memory().await.unwrap();

        // Create 5 open channels
        for i in 0u8..5 {
            let source_id = create_test_account(
                db.conn(blokli_db::TargetDb::Index),
                vec![i; 20],
                &format!("peer{}_src", i),
            )
            .await
            .unwrap();
            let dest_id = create_test_account(
                db.conn(blokli_db::TargetDb::Index),
                vec![i + 10; 20],
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

        // Query with batch_size = 3 (parameter is ignored, all channels returned)
        let result = query_channels_at_watermark(db.conn(blokli_db::TargetDb::Index), &watermark, 3)
            .await
            .unwrap();

        // Should return all 5 channels (batch_size is now unused)
        assert_eq!(result.len(), 5);
    }

    // Tests for fetch_channel_update removed - events now contain complete data
    // No need to fetch from database as IndexerEvent contains full ChannelUpdate

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
    async fn test_key_binding_fee_updated_subscription() {
        let db = BlokliDb::new_in_memory().await.unwrap();
        let indexer_state = IndexerState::new(10, 100);

        // Initialize chain_info with a keybinding fee
        let starting_fee = 12345u16;

        chain_info::Entity::delete_many()
            .exec(db.conn(blokli_db::TargetDb::Index))
            .await
            .unwrap();

        let chain_info = chain_info::ActiveModel {
            id: Set(1),
            last_indexed_block: Set(100),
            last_indexed_tx_index: Set(Some(0)),
            last_indexed_log_index: Set(Some(0)),
            ticket_price: Set(None),
            key_binding_fee: Set(Some(starting_fee.to_be_bytes().into())),
            channels_dst: Set(None),
            ledger_dst: Set(None),
            safe_registry_dst: Set(None),
            min_incoming_ticket_win_prob: Set(0.0),
            channel_closure_grace_period: Set(None),
        };
        chain_info.insert(db.conn(blokli_db::TargetDb::Index)).await.unwrap();

        // Setup schema
        let schema = Schema::build(DummyQuery, EmptyMutation, SubscriptionRoot)
            .data(db.conn(blokli_db::TargetDb::Index).clone())
            .data(indexer_state.clone())
            .finish();

        let query = "subscription { keyBindingFeeUpdated }";
        let stream = schema.execute_stream(query);
        let mut stream = stream.boxed();

        // 1. Expect initial value
        let response = stream.next().await.expect("Stream should return initial value");
        let data = response.into_result().expect("Response should be ok").data;
        // Balance is expected in format: "amount unit" (e.g., "0.000000000000012345 wxHOPR")
        let expected_fee = PrimitiveHoprBalance::from_be_bytes(starting_fee.to_be_bytes()).to_string();
        assert_eq!(
            data,
            async_graphql::Value::from_json(serde_json::json!({ "keyBindingFeeUpdated": expected_fee })).unwrap()
        );

        // 2. Publish update via event bus
        let new_fee = "67890".to_string();
        indexer_state.publish_event(IndexerEvent::KeyBindingFeeUpdated(TokenValueString(new_fee.clone())));

        // 3. Expect update
        let response = stream.next().await.expect("Stream should return update");
        let data = response.into_result().expect("Response should be ok").data;
        assert_eq!(
            data,
            async_graphql::Value::from_json(serde_json::json!({ "keyBindingFeeUpdated": new_fee })).unwrap()
        );
    }

    #[tokio::test]
    async fn test_safe_deployed_subscription_yields_on_event() {
        let db = BlokliDb::new_in_memory().await.unwrap();
        let indexer_state = IndexerState::new(10, 100);

        // Initialize chain_info
        init_chain_info(db.conn(blokli_db::TargetDb::Index), 100, 0, 0)
            .await
            .unwrap();

        // Create a test safe in the database (identity + state)
        let safe_address = vec![1; 20];
        let module_address = vec![2; 20];
        let chain_key = vec![3; 20];

        // Create identity
        let safe_identity = hopr_safe_contract::ActiveModel {
            id: Default::default(),
            address: Set(safe_address.clone()),
        };
        let identity = safe_identity.insert(db.conn(blokli_db::TargetDb::Index)).await.unwrap();

        // Create state
        let safe_state = hopr_safe_contract_state::ActiveModel {
            id: Default::default(),
            hopr_safe_contract_id: Set(identity.id),
            module_address: Set(module_address.clone()),
            chain_key: Set(chain_key.clone()),
            published_block: Set(100),
            published_tx_index: Set(0),
            published_log_index: Set(0),
        };
        safe_state.insert(db.conn(blokli_db::TargetDb::Index)).await.unwrap();

        // Setup schema
        let schema = Schema::build(DummyQuery, EmptyMutation, SubscriptionRoot)
            .data(db.conn(blokli_db::TargetDb::Index).clone())
            .data(indexer_state.clone())
            .finish();

        let query = "subscription { safeDeployed { address moduleAddress chainKey } }";
        let stream = schema.execute_stream(query);
        let mut stream = stream.boxed();

        // Spawn task to publish event after a brief delay, allowing subscription to start listening
        let indexer_state_clone = indexer_state.clone();
        let safe_addr_copy = safe_address.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(50)).await;
            let safe_addr = Address::new(&safe_addr_copy);
            indexer_state_clone.publish_event(IndexerEvent::SafeDeployed(safe_addr));
        });

        // Expect safe object in stream with timeout
        let response = tokio::time::timeout(Duration::from_secs(2), stream.next())
            .await
            .expect("timeout waiting for safe deployed event")
            .expect("Stream should return safe deployed");
        let data = response.into_result().expect("Response should be ok").data;

        let expected_address = Address::new(&safe_address).to_hex();
        let expected_module = Address::new(&module_address).to_hex();
        let expected_chain_key = Address::new(&chain_key).to_hex();

        assert_eq!(
            data,
            async_graphql::Value::from_json(serde_json::json!({
                "safeDeployed": {
                    "address": expected_address,
                    "moduleAddress": expected_module,
                    "chainKey": expected_chain_key,
                }
            }))
            .unwrap()
        );
    }

    #[tokio::test]
    async fn test_safe_deployed_subscription_initial_behavior_no_events() {
        let db = BlokliDb::new_in_memory().await.unwrap();
        let indexer_state = IndexerState::new(10, 100);

        // Initialize chain_info
        init_chain_info(db.conn(blokli_db::TargetDb::Index), 100, 0, 0)
            .await
            .unwrap();

        // Setup schema
        let schema = Schema::build(DummyQuery, EmptyMutation, SubscriptionRoot)
            .data(db.conn(blokli_db::TargetDb::Index).clone())
            .data(indexer_state.clone())
            .finish();

        let query = "subscription { safeDeployed { address } }";
        let stream = schema.execute_stream(query);
        let mut stream = stream.boxed();

        // No events emitted, stream should not yield until event occurs
        // Use timeout to ensure stream doesn't immediately produce items
        let timeout_result = tokio::time::timeout(Duration::from_millis(100), stream.next()).await;

        // Should timeout waiting for event, confirming stream is waiting
        assert!(
            timeout_result.is_err(),
            "Stream should not yield items before SafeDeployed events"
        );
    }

    #[tokio::test]
    async fn test_safe_deployed_subscription_safe_not_found() {
        let db = BlokliDb::new_in_memory().await.unwrap();
        let indexer_state = IndexerState::new(10, 100);

        // Initialize chain_info
        init_chain_info(db.conn(blokli_db::TargetDb::Index), 100, 0, 0)
            .await
            .unwrap();

        // Setup schema
        let schema = Schema::build(DummyQuery, EmptyMutation, SubscriptionRoot)
            .data(db.conn(blokli_db::TargetDb::Index).clone())
            .data(indexer_state.clone())
            .finish();

        let query = "subscription { safeDeployed { address } }";
        let stream = schema.execute_stream(query);
        let mut stream = stream.boxed();

        // Emit SafeDeployed event for address that is NOT in the database
        let non_existent_address = Address::new(&[99; 20]);
        indexer_state.publish_event(IndexerEvent::SafeDeployed(non_existent_address));

        // Stream should not yield an item (safe not found logged, stream continues)
        let timeout_result = tokio::time::timeout(Duration::from_millis(200), stream.next()).await;

        // Should timeout because no item is yielded for missing safe
        // (error is logged, stream continues waiting for next event)
        assert!(
            timeout_result.is_err(),
            "Stream should not yield when safe is not in database"
        );
    }

    #[tokio::test]
    async fn test_safe_deployed_subscription_multiple_events() {
        let db = BlokliDb::new_in_memory().await.unwrap();
        let indexer_state = IndexerState::new(10, 100);

        // Initialize chain_info
        init_chain_info(db.conn(blokli_db::TargetDb::Index), 100, 0, 0)
            .await
            .unwrap();

        // Create two test safes in the database (identity + state)
        let safe_address_1 = vec![1; 20];
        let module_address_1 = vec![2; 20];
        let chain_key_1 = vec![3; 20];

        // Create identity 1
        let safe_identity_1 = hopr_safe_contract::ActiveModel {
            id: Default::default(),
            address: Set(safe_address_1.clone()),
        };
        let identity_1 = safe_identity_1
            .insert(db.conn(blokli_db::TargetDb::Index))
            .await
            .unwrap();

        // Create state 1
        let safe_state_1 = hopr_safe_contract_state::ActiveModel {
            id: Default::default(),
            hopr_safe_contract_id: Set(identity_1.id),
            module_address: Set(module_address_1.clone()),
            chain_key: Set(chain_key_1.clone()),
            published_block: Set(100),
            published_tx_index: Set(0),
            published_log_index: Set(0),
        };
        safe_state_1.insert(db.conn(blokli_db::TargetDb::Index)).await.unwrap();

        let safe_address_2 = vec![4; 20];
        let module_address_2 = vec![5; 20];
        let chain_key_2 = vec![6; 20];

        // Create identity 2
        let safe_identity_2 = hopr_safe_contract::ActiveModel {
            id: Default::default(),
            address: Set(safe_address_2.clone()),
        };
        let identity_2 = safe_identity_2
            .insert(db.conn(blokli_db::TargetDb::Index))
            .await
            .unwrap();

        // Create state 2
        let safe_state_2 = hopr_safe_contract_state::ActiveModel {
            id: Default::default(),
            hopr_safe_contract_id: Set(identity_2.id),
            module_address: Set(module_address_2.clone()),
            chain_key: Set(chain_key_2.clone()),
            published_block: Set(101),
            published_tx_index: Set(0),
            published_log_index: Set(0),
        };
        safe_state_2.insert(db.conn(blokli_db::TargetDb::Index)).await.unwrap();

        // Setup schema
        let schema = Schema::build(DummyQuery, EmptyMutation, SubscriptionRoot)
            .data(db.conn(blokli_db::TargetDb::Index).clone())
            .data(indexer_state.clone())
            .finish();

        let query = "subscription { safeDeployed { address } }";
        let stream = schema.execute_stream(query);
        let mut stream = stream.boxed();

        // Spawn task to emit events after a brief delay
        let indexer_state_clone = indexer_state.clone();
        let safe_addr_1_copy = safe_address_1.clone();
        let safe_addr_2_copy = safe_address_2.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(50)).await;
            let safe_addr_1 = Address::new(&safe_addr_1_copy);
            indexer_state_clone.publish_event(IndexerEvent::SafeDeployed(safe_addr_1));
            tokio::time::sleep(Duration::from_millis(50)).await;
            let safe_addr_2 = Address::new(&safe_addr_2_copy);
            indexer_state_clone.publish_event(IndexerEvent::SafeDeployed(safe_addr_2));
        });

        // Expect first safe
        let response = tokio::time::timeout(Duration::from_secs(2), stream.next())
            .await
            .expect("timeout waiting for first safe")
            .expect("Stream should return first safe");
        let data = response.into_result().expect("Response should be ok").data;
        let expected_address_1 = Address::new(&safe_address_1).to_hex();
        assert_eq!(
            data,
            async_graphql::Value::from_json(serde_json::json!({
                "safeDeployed": { "address": expected_address_1 }
            }))
            .unwrap()
        );

        // Expect second safe
        let response = tokio::time::timeout(Duration::from_secs(2), stream.next())
            .await
            .expect("timeout waiting for second safe")
            .expect("Stream should return second safe");
        let data = response.into_result().expect("Response should be ok").data;
        let expected_address_2 = Address::new(&safe_address_2).to_hex();
        assert_eq!(
            data,
            async_graphql::Value::from_json(serde_json::json!({
                "safeDeployed": { "address": expected_address_2 }
            }))
            .unwrap()
        );
    }

    #[tokio::test]
    async fn test_safe_deployed_subscription_ignores_other_events() {
        let db = BlokliDb::new_in_memory().await.unwrap();
        let indexer_state = IndexerState::new(10, 100);

        // Initialize chain_info
        init_chain_info(db.conn(blokli_db::TargetDb::Index), 100, 0, 0)
            .await
            .unwrap();

        // Setup schema
        let schema = Schema::build(DummyQuery, EmptyMutation, SubscriptionRoot)
            .data(db.conn(blokli_db::TargetDb::Index).clone())
            .data(indexer_state.clone())
            .finish();

        let query = "subscription { safeDeployed { address } }";
        let stream = schema.execute_stream(query);
        let mut stream = stream.boxed();

        // Spawn task to emit non-SafeDeployed events after a brief delay
        let indexer_state_clone = indexer_state.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(50)).await;
            indexer_state_clone.publish_event(IndexerEvent::KeyBindingFeeUpdated(TokenValueString("999".to_string())));
        });

        // Stream should not yield an item for non-SafeDeployed events
        let timeout_result = tokio::time::timeout(Duration::from_millis(500), stream.next()).await;

        // Should timeout because non-SafeDeployed events are ignored
        assert!(timeout_result.is_err(), "Stream should ignore non-SafeDeployed events");
    }
}
