//! Event bus for real-time state change notifications
//!
//! This module provides an event-driven architecture for broadcasting database state changes
//! to subscribers. It uses async-broadcast for efficient multi-subscriber event distribution.
//!
//! # Architecture
//!
//! - `StateChange` events are emitted when account or channel state records are inserted
//! - Subscribers receive real-time updates for temporal queries and GraphQL subscriptions
//! - Events include full position information (block, tx_index, log_index) for ordering
//!
//! # Usage
//!
//! ```rust,ignore
//! // Create event bus
//! let event_bus = EventBus::new(1000);
//!
//! // Subscribe to events
//! let mut subscriber = event_bus.subscribe();
//! tokio::spawn(async move {
//!     while let Ok(event) = subscriber.recv().await {
//!         match event {
//!             StateChange::AccountState(change) => {
//!                 // Handle account state change
//!             }
//!             StateChange::ChannelState(change) => {
//!                 // Handle channel state change
//!             }
//!         }
//!     }
//! });
//!
//! // Publish events (done by database layer)
//! match event_bus.publish(StateChange::AccountState(AccountStateChange {
//!     account_id: 1,
//!     state_id: 42,
//!     published_block: 1000,
//!     published_tx_index: 5,
//!     published_log_index: 2,
//! })) {
//!     Ok(true) => {
//!         // Event delivered to at least one active subscriber.
//!     }
//!     Ok(false) => {
//!         // No active subscribers; this is an expected no-op.
//!     }
//!     Err(error) => {
//!         // Handle a full or closed event bus.
//!         eprintln!("failed to publish state change: {error}");
//!     }
//! }
//! ```

use async_broadcast::{InactiveReceiver, Receiver, Sender, TrySendError, broadcast};

/// Position in the blockchain for ordering events and temporal queries
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct BlockPosition {
    pub block: i64,
    pub tx_index: i64,
    pub log_index: i64,
}

impl BlockPosition {
    /// Create a new BlockPosition
    pub fn new(block: i64, tx_index: i64, log_index: i64) -> Self {
        Self {
            block,
            tx_index,
            log_index,
        }
    }

    /// Create a BlockPosition at the start of a block
    pub fn at_block(block: i64) -> Self {
        Self {
            block,
            tx_index: 0,
            log_index: 0,
        }
    }
}

/// Account state change event
#[derive(Debug, Clone)]
pub struct AccountStateChange {
    /// Account ID that changed
    pub account_id: i64,
    /// New state record ID
    pub state_id: i64,
    /// Block number where change occurred
    pub published_block: i64,
    /// Transaction index within block
    pub published_tx_index: i64,
    /// Log index within transaction
    pub published_log_index: i64,
}

impl AccountStateChange {
    pub fn position(&self) -> BlockPosition {
        BlockPosition {
            block: self.published_block,
            tx_index: self.published_tx_index,
            log_index: self.published_log_index,
        }
    }
}

/// Channel state change event
#[derive(Debug, Clone)]
pub struct ChannelStateChange {
    /// Channel ID that changed
    pub channel_id: i64,
    /// New state record ID
    pub state_id: i64,
    /// Block number where change occurred
    pub published_block: i64,
    /// Transaction index within block
    pub published_tx_index: i64,
    /// Log index within transaction
    pub published_log_index: i64,
}

impl ChannelStateChange {
    pub fn position(&self) -> BlockPosition {
        BlockPosition {
            block: self.published_block,
            tx_index: self.published_tx_index,
            log_index: self.published_log_index,
        }
    }
}

/// State change events for database mutations
#[derive(Debug, Clone)]
pub enum StateChange {
    /// Account state was updated
    AccountState(AccountStateChange),
    /// Channel state was updated
    ChannelState(ChannelStateChange),
}

impl StateChange {
    pub fn position(&self) -> BlockPosition {
        match self {
            StateChange::AccountState(change) => change.position(),
            StateChange::ChannelState(change) => change.position(),
        }
    }
}

/// Event bus for broadcasting state changes
///
/// Uses async-broadcast for efficient multi-subscriber distribution.
/// Subscribers can join at any time and will receive all future events.
#[derive(Clone)]
pub struct EventBus {
    sender: Sender<StateChange>,
    // Keep the channel open when no clients are subscribed without retaining its events.
    #[allow(dead_code)]
    _keepalive: InactiveReceiver<StateChange>,
}

impl EventBus {
    /// Create a new event bus with the given channel capacity
    ///
    /// # Arguments
    ///
    /// * `capacity` - Maximum number of events to buffer per subscriber
    ///
    /// # Example
    ///
    /// ```rust
    /// use blokli_db::events::EventBus;
    ///
    /// let event_bus = EventBus::new(1000);
    /// ```
    pub fn new(capacity: usize) -> Self {
        let (mut sender, receiver) = broadcast(capacity);
        sender.set_overflow(true);
        Self {
            sender,
            _keepalive: receiver.deactivate(),
        }
    }

    /// Create event bus from an existing sender
    ///
    /// Useful when you need to share the same channel across multiple components.
    pub fn from_sender(mut sender: Sender<StateChange>) -> Self {
        sender.set_overflow(true);
        let _keepalive = sender.new_receiver().deactivate();
        Self { sender, _keepalive }
    }

    /// Subscribe to state change events
    ///
    /// Returns a receiver that will receive all future state changes.
    /// Multiple subscribers can receive the same events concurrently.
    ///
    /// # Example
    ///
    /// ```rust
    /// use blokli_db::events::EventBus;
    ///
    /// let event_bus = EventBus::new(1000);
    /// let mut subscriber1 = event_bus.subscribe();
    /// let mut subscriber2 = event_bus.subscribe();
    ///
    /// // Both subscribers receive the same events
    /// ```
    pub fn subscribe(&self) -> Receiver<StateChange> {
        self.sender.new_receiver()
    }

    /// Publish a state change event to all subscribers
    ///
    /// This never waits for a subscriber. If a subscriber is behind the configured capacity,
    /// it misses the oldest event rather than blocking indexing.
    ///
    /// # Arguments
    ///
    /// * `event` - The state change event to publish
    ///
    /// # Returns
    ///
    /// Returns `Ok(true)` when an event was delivered to active subscribers and `Ok(false)`
    /// when none are subscribed. The latter is an expected no-op during indexing.
    pub fn publish(&self, event: StateChange) -> Result<bool, TrySendError<StateChange>> {
        match self.sender.try_broadcast(event) {
            Ok(_) => Ok(true),
            Err(TrySendError::Inactive(_)) => Ok(false),
            Err(error) => Err(error),
        }
    }

    /// Get the number of active subscribers
    pub fn subscriber_count(&self) -> usize {
        self.sender.receiver_count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_event_bus_single_subscriber() {
        let event_bus = EventBus::new(10);
        let mut subscriber = event_bus.subscribe();

        let event = StateChange::AccountState(AccountStateChange {
            account_id: 1,
            state_id: 42,
            published_block: 1000,
            published_tx_index: 5,
            published_log_index: 2,
        });

        event_bus.publish(event.clone()).unwrap();

        let received = subscriber.recv().await.unwrap();
        assert!(matches!(received, StateChange::AccountState(_)));
    }

    #[test]
    fn test_event_bus_discards_events_without_active_subscribers() {
        let event_bus = EventBus::new(1);
        let event = StateChange::AccountState(AccountStateChange {
            account_id: 1,
            state_id: 42,
            published_block: 1000,
            published_tx_index: 5,
            published_log_index: 2,
        });

        assert!(!event_bus.publish(event).unwrap());
    }

    #[tokio::test]
    async fn test_event_bus_multiple_subscribers() {
        let event_bus = EventBus::new(10);
        let mut subscriber1 = event_bus.subscribe();
        let mut subscriber2 = event_bus.subscribe();

        assert_eq!(event_bus.subscriber_count(), 2);

        let event = StateChange::ChannelState(ChannelStateChange {
            channel_id: 10,
            state_id: 100,
            published_block: 2000,
            published_tx_index: 3,
            published_log_index: 1,
        });

        event_bus.publish(event.clone()).unwrap();

        let received1 = subscriber1.recv().await.unwrap();
        let received2 = subscriber2.recv().await.unwrap();

        assert!(matches!(received1, StateChange::ChannelState(_)));
        assert!(matches!(received2, StateChange::ChannelState(_)));
    }

    #[tokio::test]
    async fn test_event_bus_drops_old_events_for_slow_subscribers() {
        let event_bus = EventBus::new(1);
        let mut subscriber = event_bus.subscribe();

        let first = StateChange::ChannelState(ChannelStateChange {
            channel_id: 10,
            state_id: 100,
            published_block: 2000,
            published_tx_index: 3,
            published_log_index: 1,
        });
        let second = StateChange::ChannelState(ChannelStateChange {
            channel_id: 10,
            state_id: 101,
            published_block: 2001,
            published_tx_index: 0,
            published_log_index: 0,
        });

        event_bus.publish(first).unwrap();
        event_bus.publish(second).unwrap();

        assert!(subscriber.recv().await.is_err());
        let received = subscriber.recv().await.unwrap();
        let StateChange::ChannelState(change) = received else {
            panic!("expected a channel state change");
        };
        assert_eq!(change.state_id, 101);
    }

    #[test]
    fn test_block_position_ordering() {
        let pos1 = BlockPosition {
            block: 100,
            tx_index: 5,
            log_index: 2,
        };
        let pos2 = BlockPosition {
            block: 100,
            tx_index: 5,
            log_index: 3,
        };
        let pos3 = BlockPosition {
            block: 100,
            tx_index: 6,
            log_index: 1,
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
}
