//! Background monitor for tracking transaction confirmations
//!
//! This module provides a background task that monitors submitted transactions
//! and updates their status based on on-chain confirmation.

use std::{sync::Arc, time::Duration};

use async_trait::async_trait;
use hopr_crypto_types::types::Hash;
use tokio::time::sleep;
use tracing::{debug, error, info};

use crate::transaction_store::{TransactionStatus, TransactionStore};

/// Trait for querying transaction receipts from the RPC
#[async_trait]
pub trait ReceiptProvider: Send + Sync {
    /// Get the status of a transaction by its hash
    /// Returns Some(true) if confirmed, Some(false) if reverted, None if still pending
    async fn get_transaction_status(&self, tx_hash: Hash) -> Result<Option<bool>, String>;
}

/// Configuration for the transaction monitor
#[derive(Debug, Clone)]
pub struct TransactionMonitorConfig {
    /// Interval between monitor polls
    pub poll_interval: Duration,
    /// Maximum time to wait before marking a transaction as timed out
    pub timeout: Duration,
}

impl Default for TransactionMonitorConfig {
    fn default() -> Self {
        Self {
            poll_interval: Duration::from_secs(5),
            timeout: Duration::from_secs(300), // 5 minutes
        }
    }
}

/// Background monitor for tracking transaction confirmations
#[derive(Debug)]
pub struct TransactionMonitor<R: ReceiptProvider> {
    transaction_store: Arc<TransactionStore>,
    receipt_provider: Arc<R>,
    config: TransactionMonitorConfig,
}

impl<R: ReceiptProvider> TransactionMonitor<R> {
    /// Create a new transaction monitor
    pub fn new(
        transaction_store: Arc<TransactionStore>,
        receipt_provider: R,
        config: TransactionMonitorConfig,
    ) -> Self {
        Self {
            transaction_store,
            receipt_provider: Arc::new(receipt_provider),
            config,
        }
    }

    /// Start the background monitoring loop
    ///
    /// This runs continuously until cancelled, checking submitted transactions
    /// for confirmation status.
    pub async fn start(self: Arc<Self>) {
        info!("Starting transaction monitor");

        loop {
            if let Err(e) = self.poll_once().await {
                error!("Error in transaction monitor poll: {}", e);
            }

            sleep(self.config.poll_interval).await;
        }
    }

    /// Perform a single poll of all submitted transactions
    async fn poll_once(&self) -> Result<(), String> {
        let submitted = self.transaction_store.list_by_status(TransactionStatus::Submitted);

        debug!("Polling {} submitted transactions", submitted.len());

        for record in submitted {
            if let Some(tx_hash) = record.transaction_hash {
                // Check if transaction has timed out
                let elapsed = chrono::Utc::now().signed_duration_since(record.submitted_at);
                if elapsed.to_std().ok() > Some(self.config.timeout) {
                    info!("Transaction {} timed out", record.id);
                    let _ = self.transaction_store.update_status(
                        record.id,
                        TransactionStatus::Timeout,
                        Some("Transaction timed out waiting for confirmation".to_string()),
                    );
                    continue;
                }

                // Check confirmation status
                match self.receipt_provider.get_transaction_status(tx_hash).await {
                    Ok(Some(true)) => {
                        info!("Transaction {} confirmed", record.id);
                        let _ = self
                            .transaction_store
                            .update_status(record.id, TransactionStatus::Confirmed, None);
                    }
                    Ok(Some(false)) => {
                        info!("Transaction {} reverted", record.id);
                        let _ = self.transaction_store.update_status(
                            record.id,
                            TransactionStatus::Reverted,
                            Some("Transaction reverted on-chain".to_string()),
                        );
                    }
                    Ok(None) => {
                        // Still pending, continue monitoring
                        debug!("Transaction {} still pending", record.id);
                    }
                    Err(e) => {
                        error!("Error checking transaction {}: {}", record.id, e);
                    }
                }
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transaction_store::TransactionRecord;
    use chrono::Utc;
    use std::collections::HashMap;
    use std::sync::Mutex;
    use uuid::Uuid;

    // Mock receipt provider for testing
    struct MockReceiptProvider {
        // Map from tx_hash to status (Some(true) = confirmed, Some(false) = reverted, None = pending)
        statuses: Arc<Mutex<HashMap<Hash, Option<bool>>>>,
    }

    impl MockReceiptProvider {
        fn new() -> Self {
            Self {
                statuses: Arc::new(Mutex::new(HashMap::new())),
            }
        }

        fn set_status(&self, tx_hash: Hash, status: Option<bool>) {
            self.statuses.lock().unwrap().insert(tx_hash, status);
        }
    }

    #[async_trait]
    impl ReceiptProvider for MockReceiptProvider {
        async fn get_transaction_status(&self, tx_hash: Hash) -> Result<Option<bool>, String> {
            Ok(self.statuses.lock().unwrap().get(&tx_hash).copied().unwrap_or(None))
        }
    }

    fn create_monitor(
        store: Arc<TransactionStore>,
        provider: MockReceiptProvider,
    ) -> TransactionMonitor<MockReceiptProvider> {
        TransactionMonitor::new(store, provider, TransactionMonitorConfig::default())
    }

    #[tokio::test]
    async fn test_monitor_updates_confirmed_transaction() {
        let store = Arc::new(TransactionStore::new());
        let provider = MockReceiptProvider::new();

        let tx_hash = Hash::default();
        let record = TransactionRecord {
            id: Uuid::new_v4(),
            raw_transaction: vec![0x01],
            transaction_hash: Some(tx_hash),
            status: TransactionStatus::Submitted,
            submitted_at: Utc::now(),
            confirmed_at: None,
            error_message: None,
        };

        let id = record.id;
        store.insert(record).unwrap();

        // Set transaction as confirmed
        provider.set_status(tx_hash, Some(true));

        let monitor = create_monitor(store.clone(), provider);
        monitor.poll_once().await.unwrap();

        // Verify status was updated
        let updated = store.get(id).unwrap();
        assert_eq!(updated.status, TransactionStatus::Confirmed);
        assert!(updated.confirmed_at.is_some());
    }

    #[tokio::test]
    async fn test_monitor_updates_reverted_transaction() {
        let store = Arc::new(TransactionStore::new());
        let provider = MockReceiptProvider::new();

        let tx_hash = Hash::default();
        let record = TransactionRecord {
            id: Uuid::new_v4(),
            raw_transaction: vec![0x01],
            transaction_hash: Some(tx_hash),
            status: TransactionStatus::Submitted,
            submitted_at: Utc::now(),
            confirmed_at: None,
            error_message: None,
        };

        let id = record.id;
        store.insert(record).unwrap();

        // Set transaction as reverted
        provider.set_status(tx_hash, Some(false));

        let monitor = create_monitor(store.clone(), provider);
        monitor.poll_once().await.unwrap();

        // Verify status was updated
        let updated = store.get(id).unwrap();
        assert_eq!(updated.status, TransactionStatus::Reverted);
        assert!(updated.error_message.is_some());
    }

    #[tokio::test]
    async fn test_monitor_handles_pending_transaction() {
        let store = Arc::new(TransactionStore::new());
        let provider = MockReceiptProvider::new();

        let tx_hash = Hash::default();
        let record = TransactionRecord {
            id: Uuid::new_v4(),
            raw_transaction: vec![0x01],
            transaction_hash: Some(tx_hash),
            status: TransactionStatus::Submitted,
            submitted_at: Utc::now(),
            confirmed_at: None,
            error_message: None,
        };

        let id = record.id;
        store.insert(record).unwrap();

        // Transaction still pending (no status set)
        let monitor = create_monitor(store.clone(), provider);
        monitor.poll_once().await.unwrap();

        // Verify status unchanged
        let updated = store.get(id).unwrap();
        assert_eq!(updated.status, TransactionStatus::Submitted);
    }

    #[tokio::test]
    async fn test_monitor_handles_timeout() {
        let store = Arc::new(TransactionStore::new());
        let provider = MockReceiptProvider::new();

        let tx_hash = Hash::default();
        // Create record with old timestamp (will be timed out)
        let record = TransactionRecord {
            id: Uuid::new_v4(),
            raw_transaction: vec![0x01],
            transaction_hash: Some(tx_hash),
            status: TransactionStatus::Submitted,
            submitted_at: Utc::now() - chrono::Duration::try_seconds(400).unwrap(), // 400 seconds ago
            confirmed_at: None,
            error_message: None,
        };

        let id = record.id;
        store.insert(record).unwrap();

        let monitor = create_monitor(store.clone(), provider);
        monitor.poll_once().await.unwrap();

        // Verify timed out
        let updated = store.get(id).unwrap();
        assert_eq!(updated.status, TransactionStatus::Timeout);
        assert!(updated.error_message.is_some());
    }

    #[tokio::test]
    async fn test_monitor_handles_multiple_transactions() {
        let store = Arc::new(TransactionStore::new());
        let provider = MockReceiptProvider::new();

        // Create multiple transactions with different statuses
        let tx_hash1 = Hash::from([1u8; 32]);
        let tx_hash2 = Hash::from([2u8; 32]);

        let record1 = TransactionRecord {
            id: Uuid::new_v4(),
            raw_transaction: vec![0x01],
            transaction_hash: Some(tx_hash1),
            status: TransactionStatus::Submitted,
            submitted_at: Utc::now(),
            confirmed_at: None,
            error_message: None,
        };

        let record2 = TransactionRecord {
            id: Uuid::new_v4(),
            raw_transaction: vec![0x02],
            transaction_hash: Some(tx_hash2),
            status: TransactionStatus::Submitted,
            submitted_at: Utc::now(),
            confirmed_at: None,
            error_message: None,
        };

        let id1 = record1.id;
        let id2 = record2.id;

        store.insert(record1).unwrap();
        store.insert(record2).unwrap();

        // Set different statuses
        provider.set_status(tx_hash1, Some(true)); // Confirmed
        provider.set_status(tx_hash2, Some(false)); // Reverted

        let monitor = create_monitor(store.clone(), provider);
        monitor.poll_once().await.unwrap();

        // Verify both were updated correctly
        let updated1 = store.get(id1).unwrap();
        assert_eq!(updated1.status, TransactionStatus::Confirmed);

        let updated2 = store.get(id2).unwrap();
        assert_eq!(updated2.status, TransactionStatus::Reverted);
    }
}
