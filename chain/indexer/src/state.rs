//! Indexer state management for coordinating block processing and subscriptions
//!
//! This module provides the IndexerState type which coordinates between:
//! - Block processing in the indexer (requires write lock)
//! - Subscription initialization (requires read lock)
//!
//! The synchronization ensures no race conditions when capturing the watermark
//! for subscriptions, guaranteeing no duplicates or data loss.

use std::sync::Arc;

use async_broadcast::{Receiver, Sender, broadcast};
use blokli_api_types::{Account, ChannelUpdate, TokenValueString};
use hopr_primitive_types::prelude::Address;
use tokio::sync::RwLock;

/// Event type for the subscription event bus
///
/// Represents changes to accounts, channels, and protocol parameters that should be broadcast to subscribers.
/// Events contain complete GraphQL data to avoid additional database queries per subscriber
/// and to ensure temporal consistency.
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
    event_bus: Sender<IndexerEvent>,

    /// Shutdown signal sender for reorg handling
    ///
    /// When a reorg is detected, a signal is sent to all active subscriptions
    /// to terminate and allow clients to reconnect with fresh state.
    shutdown_signal: Sender<()>,

    /// Initial receiver kept alive to maintain channel state
    ///
    /// This receiver is never used but must be kept alive to prevent the
    /// async_broadcast channel from entering a closed state.
    _event_bus_rx: Arc<Receiver<IndexerEvent>>,

    /// Initial shutdown receiver kept alive to maintain channel state
    _shutdown_rx: Arc<Receiver<()>>,
}

impl std::fmt::Debug for IndexerState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("IndexerState")
            .field("coordination_lock", &"Arc<RwLock<()>>")
            .field("event_bus", &"Sender<IndexerEvent>")
            .field("shutdown_signal", &"Sender<()>")
            .field("_event_bus_rx", &"Arc<Receiver<IndexerEvent>>")
            .field("_shutdown_rx", &"Arc<Receiver<()>>")
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
            _event_bus_rx: Arc::new(event_bus_rx),
            _shutdown_rx: Arc::new(shutdown_rx),
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
    /// `true` if the event was broadcast successfully, `false` if the channel is closed
    pub fn publish_event(&self, event: IndexerEvent) -> bool {
        self.event_bus.try_broadcast(event).is_ok()
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
    pub fn subscribe_to_events(&self) -> Receiver<IndexerEvent> {
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
        self.shutdown_signal.try_broadcast(()).is_ok()
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
        match received {
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
        match receiver1.recv().await.unwrap() {
            IndexerEvent::AccountUpdated(account) => assert_eq!(account.keyid, 123),
            _ => panic!("Expected AccountUpdated event"),
        }

        match receiver2.recv().await.unwrap() {
            IndexerEvent::AccountUpdated(account) => assert_eq!(account.keyid, 123),
            _ => panic!("Expected AccountUpdated event"),
        }
    }
}
