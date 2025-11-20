//! SQLite change notification system using update hooks
//!
//! This module provides real-time notifications when SQLite tables are modified.
//! It uses SQLite's native `update_hook` mechanism to detect changes and broadcasts
//! them to subscribers via async channels.
//!
//! For PostgreSQL, use LISTEN/NOTIFY instead (see `api/src/notifications.rs`).

use std::sync::{Arc, mpsc};

use tokio::sync::broadcast;

/// Types of SQLite table notifications
///
/// This enum is extensible for future table notifications.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SqliteNotification {
    /// The `chain_info` table was updated (ticket_price, win_prob, etc.)
    ChainInfoUpdated,
    // Future notifications:
    // AccountUpdated(i64),
    // ChannelUpdated(i64),
}

/// Manager for SQLite update hook notifications
///
/// This structure bridges SQLite's synchronous update hooks to async subscribers.
/// It uses a sync channel (for the hook callback) that feeds into an async broadcast
/// channel (for stream consumers).
///
/// # Architecture
///
/// ```text
/// SQLite Connection
///       │
///       ▼
/// update_hook callback (sync)
///       │
///       ▼
/// mpsc::Sender (sync channel)
///       │
///       ▼
/// Bridge thread
///       │
///       ▼
/// broadcast::Sender (async channel)
///       │
///       ▼
/// Subscribers (async streams)
/// ```
#[derive(Clone)]
pub struct SqliteNotificationManager {
    /// Async broadcast sender for distributing notifications
    async_sender: broadcast::Sender<SqliteNotification>,
}

impl SqliteNotificationManager {
    /// Create a new notification manager and return the sync sender for hooks
    ///
    /// # Arguments
    ///
    /// * `capacity` - Buffer capacity for the async broadcast channel
    ///
    /// # Returns
    ///
    /// A tuple of:
    /// - The manager itself (for subscribing)
    /// - A sync sender to use in update hook callbacks
    pub fn new(capacity: usize) -> (Self, mpsc::Sender<SqliteNotification>) {
        let (sync_tx, sync_rx) = mpsc::channel::<SqliteNotification>();
        let (async_tx, _async_rx) = broadcast::channel::<SqliteNotification>(capacity);

        let async_tx_clone = async_tx.clone();

        // Bridge sync channel to async broadcast in a dedicated thread
        // This thread will exit when the sync sender is dropped
        std::thread::spawn(move || {
            while let Ok(notification) = sync_rx.recv() {
                // Ignore send errors (no subscribers)
                let _ = async_tx_clone.send(notification);
            }
        });

        (Self { async_sender: async_tx }, sync_tx)
    }

    /// Subscribe to notifications
    ///
    /// Returns an async receiver that yields notifications when tables are modified.
    pub fn subscribe(&self) -> broadcast::Receiver<SqliteNotification> {
        self.async_sender.subscribe()
    }

    /// Get the number of active subscribers
    pub fn subscriber_count(&self) -> usize {
        self.async_sender.receiver_count()
    }
}

impl std::fmt::Debug for SqliteNotificationManager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SqliteNotificationManager")
            .field("subscriber_count", &self.subscriber_count())
            .finish()
    }
}

/// Wrapper to share the sync sender across connection hooks
///
/// This is needed because each connection in the pool gets its own hook,
/// but they all need to send to the same channel.
#[derive(Clone)]
pub struct SqliteHookSender {
    inner: Arc<mpsc::Sender<SqliteNotification>>,
}

impl SqliteHookSender {
    /// Create a new hook sender wrapper
    pub fn new(sender: mpsc::Sender<SqliteNotification>) -> Self {
        Self {
            inner: Arc::new(sender),
        }
    }

    /// Send a notification (ignores errors if receiver is gone)
    pub fn send(&self, notification: SqliteNotification) {
        let _ = self.inner.send(notification);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_notification_manager_creation() {
        let (manager, _sender) = SqliteNotificationManager::new(10);
        assert_eq!(manager.subscriber_count(), 0);
    }

    #[test]
    fn test_subscribe_increases_count() {
        let (manager, _sender) = SqliteNotificationManager::new(10);
        let _rx1 = manager.subscribe();
        assert_eq!(manager.subscriber_count(), 1);
        let _rx2 = manager.subscribe();
        assert_eq!(manager.subscriber_count(), 2);
    }

    #[tokio::test]
    async fn test_notification_delivery() {
        let (manager, sender) = SqliteNotificationManager::new(10);
        let mut rx = manager.subscribe();

        // Send notification
        sender.send(SqliteNotification::ChainInfoUpdated).unwrap();

        // Small delay to allow bridge thread to process
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;

        // Should receive notification
        let result = rx.try_recv();
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), SqliteNotification::ChainInfoUpdated);
    }

    #[tokio::test]
    async fn test_multiple_subscribers_receive() {
        let (manager, sender) = SqliteNotificationManager::new(10);
        let mut rx1 = manager.subscribe();
        let mut rx2 = manager.subscribe();

        // Send notification
        sender.send(SqliteNotification::ChainInfoUpdated).unwrap();

        // Small delay
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;

        // Both should receive
        assert!(rx1.try_recv().is_ok());
        assert!(rx2.try_recv().is_ok());
    }

    #[test]
    fn test_hook_sender_clone() {
        let (_manager, sender) = SqliteNotificationManager::new(10);
        let hook_sender = SqliteHookSender::new(sender);

        // Clone should work
        let _cloned = hook_sender.clone();
    }
}
