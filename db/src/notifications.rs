//! Database change notification system for SQLite
//!
//! This module provides SQLite update hook functionality to enable real-time
//! notifications when chain_info table is updated. This works in conjunction with
//! PostgreSQL LISTEN/NOTIFY to provide a unified notification interface.

use std::sync::Arc;

use async_broadcast::{Sender, broadcast};

/// Notification for chain_info table updates
///
/// Sent when ticket_price or min_incoming_ticket_win_prob changes.
/// Subscribers should query the database for the latest values.
#[derive(Debug, Clone)]
pub struct ChainInfoNotification;

/// SQLite update hook manager for chain_info notifications
///
/// This structure manages SQLite update hooks to emit notifications when the
/// chain_info table is modified. It uses a broadcast channel to distribute
/// notifications to multiple subscribers.
pub struct SqliteNotificationManager {
    sender: Sender<ChainInfoNotification>,
    _keep_alive_receiver: Arc<async_broadcast::Receiver<ChainInfoNotification>>,
}

impl SqliteNotificationManager {
    /// Create a new notification manager with the specified channel capacity
    pub fn new(capacity: usize) -> Self {
        let (mut sender, receiver) = broadcast(capacity);
        sender.set_overflow(true);

        Self {
            sender,
            _keep_alive_receiver: Arc::new(receiver),
        }
    }

    /// Get a new subscriber to chain_info notifications
    pub fn subscribe(&self) -> async_broadcast::Receiver<ChainInfoNotification> {
        self.sender.new_receiver()
    }

    /// Manually trigger a notification (for testing or manual updates)
    pub fn notify(&self) {
        let _ = self.sender.try_broadcast(ChainInfoNotification);
    }
}

impl Default for SqliteNotificationManager {
    fn default() -> Self {
        Self::new(100)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_notification_manager_creation() {
        let manager = SqliteNotificationManager::new(10);
        let _rx = manager.subscribe();
    }

    #[test]
    fn test_manual_notification() {
        let manager = SqliteNotificationManager::new(10);
        let mut rx = manager.subscribe();

        manager.notify();

        let notification = rx.try_recv();
        assert!(notification.is_ok());
    }

    #[test]
    fn test_multiple_subscribers() {
        let manager = SqliteNotificationManager::new(10);
        let mut rx1 = manager.subscribe();
        let mut rx2 = manager.subscribe();

        manager.notify();

        assert!(rx1.try_recv().is_ok());
        assert!(rx2.try_recv().is_ok());
    }
}
