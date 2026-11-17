//! Indexer state management for coordinating block processing and subscriptions
//!
//! This module provides the IndexerState type which coordinates between:
//! - Block processing in the indexer (requires write lock)
//! - Subscription initialization (requires read lock)
//!
//! The synchronization ensures no race conditions when capturing the watermark
//! for subscriptions, guaranteeing no duplicates or data loss.

use std::{
    collections::{HashMap, HashSet},
    future::Future,
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};

use async_broadcast::{InactiveReceiver, Receiver, Sender, TrySendError, broadcast};
use blokli_api_types::{
    Account, ChannelUpdate, CurvyCommittedNote, CurvyCommittedNullifier, CurvyPendingNote, RedeemTicketDetails,
    RedemptionResult, ServiceTypeUpdate, ServiceUpdate, TicketParameters, TokenValueString, UInt64,
};
use hopr_types::primitive::prelude::Address;
use tokio::{sync::RwLock, time::error::Elapsed};

/// Internal event payload for a ticket redemption observed on-chain.
///
/// Carries all fields needed to identify the ticket and report the outcome.
/// The `channel_id` field is used for subscription filtering and stripped
/// before the event is yielded to GraphQL clients (see `From` impl below).
///
/// # Examples
///
/// ```
/// use blokli_api_types::{RedeemTicketDetails, RedemptionResult};
/// use blokli_chain_indexer::state::RedeemTicketDetailsInfo;
///
/// let info = RedeemTicketDetailsInfo {
///     issuer_address: "0xaabbccdd".to_string(),
///     recipient_address: "0xddeeff00".to_string(),
///     epoch: 1,
///     index: 0,
///     channel_id: "14a2c4f5".to_string(),
///     result: RedemptionResult::Redeemed,
/// };
/// let gql: RedeemTicketDetails = info.into();
/// ```
#[derive(Debug, Clone)]
pub struct RedeemTicketDetailsInfo {
    /// Issuer account on-chain address in hexadecimal format
    pub issuer_address: String,
    /// Recipient account on-chain address in hexadecimal format
    pub recipient_address: String,
    /// Epoch of the channel where the ticket was redeemed
    pub epoch: u32,
    /// Channel ID in lowercase hex (no `0x` prefix), used for subscription filtering
    pub channel_id: String,
    /// Index of the ticket within the channel epoch
    pub index: u64,
    /// Outcome of the redemption attempt
    pub result: RedemptionResult,
}

impl From<RedeemTicketDetailsInfo> for RedeemTicketDetails {
    fn from(value: RedeemTicketDetailsInfo) -> Self {
        Self {
            issuer_address: value.issuer_address,
            recipient_address: value.recipient_address,
            epoch: UInt64(value.epoch as u64),
            index: UInt64(value.index),
            result: value.result,
        }
    }
}

/// Event type for the subscription event bus
///
/// Represents changes to accounts, channels, and protocol parameters that should be broadcast to subscribers.
/// Events contain complete GraphQL data to avoid additional database queries per subscriber
/// and to ensure temporal consistency.
///
/// The bus carries each event as an [`Arc`], so delivering it to a subscriber is a reference count
/// increment; a subscriber clones only the payload it actually yields.
#[derive(Clone, Debug)]
pub enum IndexerEvent {
    /// An account was updated (balance change, announcement, safe registration, etc.)
    ///
    /// Contains complete account data including all balances and multiaddresses.
    AccountUpdated(Account),

    /// A channel was updated (opened, closed, balance changed, etc.)
    ///
    /// Contains complete channel data plus both participating accounts.
    ChannelUpdated(Box<ChannelUpdate>),

    /// Key binding fee was updated
    KeyBindingFeeUpdated(TokenValueString),

    /// A new safe was deployed
    SafeDeployed(Address),

    /// Ticket parameters (price or winning probability) were updated
    ///
    /// Contains both ticket price and winning probability since both are needed
    /// for the GraphQL subscription.
    TicketParametersUpdated(TicketParameters),

    /// A ticket redemption was observed on-chain (either accepted or rejected).
    ///
    /// Carries all fields needed to identify the ticket and report the outcome.
    /// Subscribers filter by channel ID, issuer, or recipient address.
    TicketRedeemed(RedeemTicketDetailsInfo),

    /// A note emitted by the Curvy Aggregator's `PendingNotes` event.
    CurvyPendingNote(CurvyPendingNote),

    /// A note emitted by the Curvy Aggregator's `CommittedNotes` event.
    CurvyCommittedNote(CurvyCommittedNote),

    /// A nullifier emitted by the Curvy Aggregator's `CommittedNullifiers` event.
    CurvyCommittedNullifier(CurvyCommittedNullifier),

    /// A service registry entry was registered, updated or deregistered.
    ///
    /// Carries the kind of change explicitly, because a deregistration leaves no entry behind
    /// and a registration is not distinguishable from an update by its timestamps alone.
    ServiceEntryUpdated(ServiceUpdate),

    /// Service-type or registry-wide configuration changed.
    ServiceTypeUpdated(ServiceTypeUpdate),
}

/// Shared state for coordinating indexer operations with subscriptions
///
/// The IndexerState uses a RwLock to coordinate between:
/// - Block processing (acquires write lock while processing)
/// - Subscription initialization (acquires read lock to capture watermark)
///
/// This ensures that when a subscription captures its watermark, no block
/// processing is in progress, preventing race conditions.
#[derive(Clone)]
pub struct IndexerState {
    /// RwLock for coordinating block processing and watermark capture
    ///
    /// - Write lock: Held during block processing
    /// - Read lock: Held during subscription watermark capture
    coordination_lock: Arc<RwLock<()>>,

    /// Event bus sender for broadcasting account and channel updates
    ///
    /// Block processing publishes events here after completing each block.
    /// Subscriptions receive events via receivers cloned from this sender.
    event_bus: Sender<Arc<IndexerEvent>>,

    /// Shutdown signal sender for reorg handling
    ///
    /// When a reorg is detected, a signal is sent to all active subscriptions
    /// to terminate and allow clients to reconnect with fresh state.
    shutdown_signal: Sender<()>,

    /// Initial receiver kept alive to maintain channel state
    ///
    /// Kept inactive so the channel stays open without counting as a subscriber: an active receiver
    /// that is never read would keep the whole buffer of events alive, and would force every
    /// subscriber to clone each event instead of taking the last copy.
    _event_bus_rx: Arc<InactiveReceiver<Arc<IndexerEvent>>>,

    /// Initial shutdown receiver kept alive to maintain channel state
    _shutdown_rx: Arc<Receiver<()>>,

    /// Monotonic counter used to trigger Safe filter refreshes.
    safe_filter_epoch: Arc<AtomicU64>,

    /// Number of reorganizations signaled to subscriptions.
    reorg_epoch: Arc<AtomicU64>,

    /// Number of blocks whose writes completed but whose indexed-block update failed.
    failed_indexed_block_updates: Arc<AtomicU64>,

    /// Coordination between block processing and the Safe owner refresh task.
    safe_owner_sync: Arc<SafeOwnerSync>,
}

/// Shared bookkeeping between block processing and the background Safe owner refresh.
///
/// Block processing reports Safes that need an owner snapshot (a read at discovery failed, a stored
/// snapshot was dropped) and the owner events it processed. The refresh task reads the reported
/// Safes over RPC, and the indexing loop uses the event record to drop snapshot reads that were
/// pinned before an event that has been processed since.
#[derive(Default)]
struct SafeOwnerSync {
    /// Safes reported since the refresh task last looked.
    pending: Mutex<HashSet<Address>>,
    /// Every Safe not yet synced: reported and not yet stored (queued, being read, being stored, in backoff).
    unsynced: Mutex<HashSet<Address>>,
    /// Highest block of an owner or threshold event processed per Safe. An entry is removed when a stored
    /// snapshot covers it, so the map is bounded by the number of Safes that currently await a snapshot.
    event_blocks: Mutex<HashMap<Address, u64>>,
    /// Safes discovered live whose `SafeDeployed` announcement waits for their first stored owner snapshot.
    awaiting_announcement: Mutex<HashSet<Address>>,
    wakeup: tokio::sync::Notify,
}

impl std::fmt::Debug for IndexerState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("IndexerState")
            .field("coordination_lock", &"Arc<RwLock<()>>")
            .field("event_bus", &"Sender<Arc<IndexerEvent>>")
            .field("shutdown_signal", &"Sender<()>")
            .field("_event_bus_rx", &"Arc<InactiveReceiver<Arc<IndexerEvent>>>")
            .field("_shutdown_rx", &"Arc<Receiver<()>>")
            .field("safe_filter_epoch", &"Arc<AtomicU64>")
            .field("reorg_epoch", &"Arc<AtomicU64>")
            .field("failed_indexed_block_updates", &"Arc<AtomicU64>")
            .field("safe_owner_sync", &"Arc<SafeOwnerSync>")
            .finish()
    }
}

impl IndexerState {
    /// Creates a new IndexerState with specified buffer sizes
    ///
    /// # Arguments
    ///
    /// * `event_bus_capacity` - Capacity of the event bus channel (default: 1000)
    /// * `shutdown_signal_capacity` - Capacity of the shutdown signal channel (default: 10)
    ///
    /// # Returns
    ///
    /// A new IndexerState instance ready for use
    pub fn new(event_bus_capacity: usize, shutdown_signal_capacity: usize) -> Self {
        // Create broadcast channels, keeping the initial receivers alive
        let (mut event_bus, event_bus_rx) = broadcast(event_bus_capacity);
        let (mut shutdown_signal, shutdown_rx) = broadcast(shutdown_signal_capacity);

        // Set overflow behavior to allow new receivers to miss old messages if they can't keep up
        event_bus.set_overflow(true);
        shutdown_signal.set_overflow(true);

        Self {
            coordination_lock: Arc::new(RwLock::new(())),
            event_bus,
            shutdown_signal,
            _event_bus_rx: Arc::new(event_bus_rx.deactivate()),
            _shutdown_rx: Arc::new(shutdown_rx),
            safe_filter_epoch: Arc::new(AtomicU64::new(0)),
            reorg_epoch: Arc::new(AtomicU64::new(0)),
            failed_indexed_block_updates: Arc::new(AtomicU64::new(0)),
            safe_owner_sync: Arc::new(SafeOwnerSync::default()),
        }
    }

    /// Acquires write lock for block processing
    ///
    /// This should be called at the start of processing each block and held
    /// until all database writes for that block are complete.
    ///
    /// # Returns
    ///
    /// A write lock guard that must be held until block processing completes
    pub async fn acquire_processing_lock(&self) -> tokio::sync::RwLockWriteGuard<'_, ()> {
        self.coordination_lock.write().await
    }

    /// Acquires read lock for subscription watermark capture
    ///
    /// This should be called when initializing a subscription to ensure
    /// the watermark is captured atomically with subscribing to the event bus.
    ///
    /// # Returns
    ///
    /// A read lock guard that should be held during watermark capture
    pub async fn acquire_watermark_lock(&self) -> tokio::sync::RwLockReadGuard<'_, ()> {
        self.coordination_lock.read().await
    }

    /// Runs `read` while holding the watermark lock, giving up once it has taken `timeout`
    ///
    /// Block processing waits for every holder of the watermark lock, so a read that stalls
    /// while holding it - a database query waiting for a free connection, say - stalls the
    /// indexer with it. Bounding the read keeps that wait short: on timeout the read is dropped
    /// and the lock is released.
    ///
    /// The time spent waiting for the lock itself is not counted.
    ///
    /// # Errors
    ///
    /// Returns [`Elapsed`] if `read` did not complete within `timeout`
    pub async fn read_synchronized<F>(&self, timeout: Duration, read: F) -> Result<F::Output, Elapsed>
    where
        F: Future,
    {
        let _lock = self.acquire_watermark_lock().await;
        tokio::time::timeout(timeout, read).await
    }

    /// Publishes an indexer event to the event bus
    ///
    /// This should be called after successfully processing a database update
    /// for accounts or channels.
    ///
    /// # Arguments
    ///
    /// * `event` - The indexer event to broadcast (AccountUpdated or ChannelUpdated)
    ///
    /// # Returns
    ///
    /// `true` if the event was broadcast or nobody is subscribed, `false` if the channel is closed
    pub fn publish_event(&self, event: IndexerEvent) -> bool {
        !matches!(
            self.event_bus.try_broadcast(Arc::new(event)),
            Err(TrySendError::Closed(_))
        )
    }

    /// Subscribes to the event bus
    ///
    /// Creates a new receiver that will receive all future indexer events.
    /// This should be called while holding the watermark lock to ensure
    /// proper synchronization.
    ///
    /// # Returns
    ///
    /// A receiver for indexer events
    pub fn subscribe_to_events(&self) -> Receiver<Arc<IndexerEvent>> {
        // Get a fresh receiver from the sender to avoid inheriting backlog
        self.event_bus.new_receiver()
    }

    /// Signals all subscriptions to shut down due to reorg
    ///
    /// This should be called when a blockchain reorganization is detected.
    /// All active subscriptions will terminate, forcing clients to reconnect
    /// and re-establish state.
    ///
    /// # Returns
    ///
    /// `true` if the signal was broadcast successfully, `false` if the channel is closed
    pub fn signal_shutdown(&self) -> bool {
        self.reorg_epoch.fetch_add(1, Ordering::Relaxed);
        self.shutdown_signal.try_broadcast(()).is_ok()
    }

    /// Returns how many reorganizations have been signaled.
    ///
    /// The indexer returns to block numbers it has already indexed when it handles a
    /// reorganization. Together with the epoch, a block number still identifies one state of the
    /// index, which is what anything derived from "the state at block N" has to be keyed by.
    /// Read it while holding the watermark lock to pair it with a watermark.
    pub fn reorg_epoch(&self) -> u64 {
        self.reorg_epoch.load(Ordering::Relaxed)
    }

    /// Records that a block's writes completed but updating the indexed block failed.
    ///
    /// The watermark stays behind the writes of such a block until a later block moves it, so a
    /// snapshot taken from it must not be reused as if the block had been marked.
    pub fn mark_indexed_block_update_failed(&self) {
        self.failed_indexed_block_updates.fetch_add(1, Ordering::Relaxed);
    }

    /// Returns how many blocks completed their writes but failed to update the indexed block.
    ///
    /// Read it while holding the watermark lock to pair it with a watermark.
    pub fn failed_indexed_block_updates(&self) -> u64 {
        self.failed_indexed_block_updates.load(Ordering::Relaxed)
    }

    /// Subscribes to shutdown signals
    ///
    /// Creates a receiver that will be notified when a shutdown is signaled.
    /// Subscriptions should monitor this receiver and terminate gracefully
    /// when a signal is received.
    ///
    /// # Returns
    ///
    /// A receiver for shutdown signals
    pub fn subscribe_to_shutdown(&self) -> Receiver<()> {
        // Get a fresh receiver from the sender to avoid inheriting backlog
        self.shutdown_signal.new_receiver()
    }

    /// Returns the current Safe filter epoch.
    /// Reports a Safe that needs an owner snapshot and wakes the refresh task.
    pub fn mark_safe_owner_snapshot_pending(&self, safe_address: Address) {
        if let Ok(mut pending) = self.safe_owner_sync.pending.lock() {
            pending.insert(safe_address);
        }
        if let Ok(mut unsynced) = self.safe_owner_sync.unsynced.lock() {
            unsynced.insert(safe_address);
        }
        self.safe_owner_sync.wakeup.notify_one();
    }

    /// Remembers that the `SafeDeployed` announcement of the Safe is held back until its owner snapshot is stored.
    pub fn mark_safe_awaiting_deployment_announcement(&self, safe_address: Address) {
        if let Ok(mut awaiting) = self.safe_owner_sync.awaiting_announcement.lock() {
            awaiting.insert(safe_address);
        }
    }

    /// Publishes `SafeDeployed` for the Safe if its announcement was held back, and clears the flag.
    ///
    /// Returns whether an announcement was made. Called once the owner snapshot of the Safe is stored, so
    /// subscribers read the owners and threshold from the database.
    pub fn announce_safe_deployed_if_awaiting(&self, safe_address: Address) -> bool {
        let was_awaiting = self
            .safe_owner_sync
            .awaiting_announcement
            .lock()
            .map(|mut awaiting| awaiting.remove(&safe_address))
            .unwrap_or(false);
        if was_awaiting {
            self.publish_event(IndexerEvent::SafeDeployed(safe_address));
        }
        was_awaiting
    }

    /// Records that a snapshot of the Safe has been stored.
    pub fn mark_safe_owner_snapshot_synced(&self, safe_address: Address) {
        if let Ok(mut unsynced) = self.safe_owner_sync.unsynced.lock() {
            unsynced.remove(&safe_address);
        }
    }

    /// Whether the Safe has an owner snapshot that was requested and is not stored yet.
    pub fn is_safe_owner_snapshot_unsynced(&self, safe_address: Address) -> bool {
        self.safe_owner_sync
            .unsynced
            .lock()
            .map(|unsynced| unsynced.contains(&safe_address))
            .unwrap_or(false)
    }

    /// Number of Safes whose owner snapshot has been requested and not stored yet.
    pub fn unsynced_safe_owner_snapshots(&self) -> usize {
        self.safe_owner_sync
            .unsynced
            .lock()
            .map(|unsynced| unsynced.len())
            .unwrap_or(0)
    }

    /// Takes the Safes reported since the last call.
    pub fn take_pending_safe_owner_snapshots(&self) -> Vec<Address> {
        self.safe_owner_sync
            .pending
            .lock()
            .map(|mut pending| pending.drain().collect())
            .unwrap_or_default()
    }

    /// Resolves when a Safe has been reported (a report made before the call is not lost).
    pub async fn safe_owner_snapshot_reported(&self) {
        self.safe_owner_sync.wakeup.notified().await;
    }

    /// Records that an owner or threshold event of the Safe was processed at `block`.
    pub fn record_safe_owner_event(&self, safe_address: Address, block: u64) {
        if let Ok(mut blocks) = self.safe_owner_sync.event_blocks.lock() {
            let entry = blocks.entry(safe_address).or_insert(block);
            *entry = (*entry).max(block);
        }
    }

    /// Highest block of an owner or threshold event processed for the Safe since the last snapshot read
    /// that covered it.
    pub fn safe_owner_event_block(&self, safe_address: Address) -> Option<u64> {
        self.safe_owner_sync
            .event_blocks
            .lock()
            .ok()
            .and_then(|blocks| blocks.get(&safe_address).copied())
    }

    /// Forgets the recorded events of the Safe up to and including `block` (they are covered by a stored
    /// snapshot pinned at `block`).
    pub fn clear_safe_owner_events_up_to(&self, safe_address: Address, block: u64) {
        if let Ok(mut blocks) = self.safe_owner_sync.event_blocks.lock() {
            if blocks.get(&safe_address).is_some_and(|recorded| *recorded <= block) {
                blocks.remove(&safe_address);
            }
        }
    }

    pub fn safe_filter_epoch(&self) -> u64 {
        self.safe_filter_epoch.load(Ordering::Relaxed)
    }

    /// Marks the current Safe filter set as stale and returns the new epoch.
    pub fn mark_safe_filters_dirty(&self) -> u64 {
        self.safe_filter_epoch.fetch_add(1, Ordering::Relaxed) + 1
    }
}

impl Default for IndexerState {
    fn default() -> Self {
        Self::new(1000, 10)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_indexer_state_coordination_locks() {
        let state = IndexerState::default();

        // Test that we can acquire write lock
        let write_guard = state.acquire_processing_lock().await;
        drop(write_guard);

        // Test that we can acquire read lock
        let read_guard = state.acquire_watermark_lock().await;
        drop(read_guard);
    }

    #[tokio::test]
    async fn test_read_synchronized_returns_the_result_of_the_read() {
        let state = IndexerState::default();

        let result = state.read_synchronized(Duration::from_secs(1), async { 42 }).await;

        assert_eq!(result.ok(), Some(42));
    }

    #[tokio::test]
    async fn test_read_synchronized_gives_up_and_releases_the_lock_when_the_read_stalls() {
        let state = IndexerState::default();

        let result = state
            .read_synchronized(Duration::from_millis(20), std::future::pending::<()>())
            .await;

        assert!(result.is_err());

        // Block processing must be able to continue right away
        let processing = tokio::time::timeout(Duration::from_millis(100), state.acquire_processing_lock()).await;
        assert!(processing.is_ok());
    }

    #[tokio::test]
    async fn test_event_bus_publish_subscribe() {
        let state = IndexerState::default();

        // Subscribe before publishing
        let mut receiver = state.subscribe_to_events();

        // Create a test account
        let test_account = blokli_api_types::Account {
            keyid: 42,
            chain_key: "0x1234".to_string(),
            packet_key: "peer123".to_string(),
            safe_address: None,
            multi_addresses: vec![],
        };

        // Publish an event
        let event = IndexerEvent::AccountUpdated(test_account.clone());
        state.publish_event(event);

        // Receive the event
        let received = receiver.recv().await.unwrap();
        match &*received {
            IndexerEvent::AccountUpdated(account) => assert_eq!(account.keyid, 42),
            _ => panic!("Expected AccountUpdated event"),
        }
    }

    #[tokio::test]
    async fn test_shutdown_signal() {
        let state = IndexerState::default();

        // Subscribe to shutdown
        let mut receiver = state.subscribe_to_shutdown();

        // Signal shutdown
        state.signal_shutdown();

        // Receive shutdown signal
        assert!(receiver.recv().await.is_ok());
    }

    #[test]
    fn test_reorg_epoch_advances_with_every_shutdown_signal() {
        let state = IndexerState::default();
        assert_eq!(state.reorg_epoch(), 0);

        state.signal_shutdown();
        assert_eq!(state.reorg_epoch(), 1);

        state.signal_shutdown();
        assert_eq!(state.reorg_epoch(), 2);
    }

    #[test]
    fn test_reorg_epoch_is_shared_between_clones() {
        let state = IndexerState::default();
        let clone = state.clone();

        state.signal_shutdown();

        assert_eq!(clone.reorg_epoch(), 1);
    }

    #[test]
    fn test_failed_indexed_block_updates_start_at_zero_and_increment() {
        let state = IndexerState::default();
        assert_eq!(state.failed_indexed_block_updates(), 0);

        state.mark_indexed_block_update_failed();
        assert_eq!(state.failed_indexed_block_updates(), 1);

        state.mark_indexed_block_update_failed();
        assert_eq!(state.failed_indexed_block_updates(), 2);
    }

    #[tokio::test]
    async fn test_multiple_subscribers() {
        let state = IndexerState::default();

        // Create multiple subscribers
        let mut receiver1 = state.subscribe_to_events();
        let mut receiver2 = state.subscribe_to_events();

        // Create a test account
        let test_account = blokli_api_types::Account {
            keyid: 123,
            chain_key: "0xabcd".to_string(),
            packet_key: "peer456".to_string(),
            safe_address: None,
            multi_addresses: vec![],
        };

        // Publish event
        let event = IndexerEvent::AccountUpdated(test_account.clone());
        assert!(state.publish_event(event));

        // Verify both received the event
        match &*receiver1.recv().await.unwrap() {
            IndexerEvent::AccountUpdated(account) => assert_eq!(account.keyid, 123),
            _ => panic!("Expected AccountUpdated event"),
        }

        match &*receiver2.recv().await.unwrap() {
            IndexerEvent::AccountUpdated(account) => assert_eq!(account.keyid, 123),
            _ => panic!("Expected AccountUpdated event"),
        }
    }

    #[test]
    fn test_safe_filter_epoch_increments() {
        let state = IndexerState::default();

        assert_eq!(state.safe_filter_epoch(), 0);
        assert_eq!(state.mark_safe_filters_dirty(), 1);
        assert_eq!(state.safe_filter_epoch(), 1);
        assert_eq!(state.mark_safe_filters_dirty(), 2);
        assert_eq!(state.safe_filter_epoch(), 2);
    }
}
