//! Background monitor for tracking transaction confirmations
//!
//! This module provides a background task that monitors submitted transactions
//! and updates their status based on on-chain confirmation.

use std::{sync::Arc, time::Duration};

use async_trait::async_trait;
use hopr_crypto_types::types::Hash;
use tokio::time::sleep;
use tracing::{debug, error, info, warn};

use crate::{
    safe_execution::{decode_transaction_to_address, inspect_safe_execution_logs},
    transaction_store::{SafeExecutionResult, TransactionStatus, TransactionStore},
};

/// A single log entry from a transaction receipt
#[derive(Debug, Clone)]
pub struct ReceiptLog {
    /// Address of the contract that emitted the log
    pub address: [u8; 20],
    /// Log topics (indexed parameters, first topic is event signature)
    pub topics: Vec<[u8; 32]>,
    /// Non-indexed log data
    pub data: Vec<u8>,
}

/// Trait for querying transaction receipts from the RPC
#[async_trait]
pub trait ReceiptProvider: Send + Sync {
    /// Get the status of a transaction by its hash
    /// Returns Some(true) if confirmed, Some(false) if reverted, None if still pending
    async fn get_transaction_status(&self, tx_hash: Hash) -> Result<Option<bool>, String>;

    /// Fetch receipt logs for a confirmed transaction.
    ///
    /// Returns `Ok(Some(logs))` when the receipt exists (even if logs are empty),
    /// `Ok(None)` when the receipt is not found (transaction still pending),
    /// or `Err` on RPC failure.
    async fn get_transaction_receipt_logs(&self, tx_hash: Hash) -> Result<Option<Vec<ReceiptLog>>, String>;
}

/// Trait for checking if an address is a known Safe contract
#[async_trait]
pub trait SafeAddressChecker: Send + Sync + std::fmt::Debug {
    /// Check if the given address is a known Safe contract in the database
    async fn is_known_safe(&self, address: &[u8; 20]) -> bool;
}

/// Configuration for the transaction monitor
#[derive(Debug, Clone)]
pub struct TransactionMonitorConfig {
    /// Interval between monitor polls
    pub poll_interval: Duration,
    /// Maximum time to wait before marking a transaction as timed out
    pub timeout: Duration,
    /// Delay between RPC calls when checking multiple transactions
    /// This prevents overwhelming the RPC endpoint with burst requests
    pub per_transaction_delay: Duration,
}

impl Default for TransactionMonitorConfig {
    fn default() -> Self {
        Self {
            poll_interval: Duration::from_secs(5),
            timeout: Duration::from_secs(300), // 5 minutes
            per_transaction_delay: Duration::from_millis(100),
        }
    }
}

/// Background monitor for tracking transaction confirmations
pub struct TransactionMonitor<R: ReceiptProvider> {
    transaction_store: Arc<TransactionStore>,
    receipt_provider: Arc<R>,
    safe_checker: Option<Arc<dyn SafeAddressChecker>>,
    config: TransactionMonitorConfig,
}

impl<R: ReceiptProvider> std::fmt::Debug for TransactionMonitor<R> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TransactionMonitor")
            .field("config", &self.config)
            .field("safe_checker", &self.safe_checker)
            .finish_non_exhaustive()
    }
}

impl<R: ReceiptProvider> TransactionMonitor<R> {
    /// Create a new transaction monitor
    pub fn new(
        transaction_store: Arc<TransactionStore>,
        receipt_provider: R,
        config: TransactionMonitorConfig,
        safe_checker: Option<Arc<dyn SafeAddressChecker>>,
    ) -> Self {
        Self {
            transaction_store,
            receipt_provider: Arc::new(receipt_provider),
            safe_checker,
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
            let tx_hash = record.transaction_hash;

            // Check if transaction has timed out
            let elapsed = chrono::Utc::now().signed_duration_since(record.submitted_at);
            if elapsed.to_std().ok() > Some(self.config.timeout) {
                info!("Transaction {} timed out", record.id);
                if let Err(e) = self.transaction_store.update_status(
                    record.id,
                    TransactionStatus::Timeout,
                    Some("Transaction timed out waiting for confirmation".to_string()),
                ) {
                    error!("Failed to update transaction {} status to Timeout: {}", record.id, e);
                }
                continue;
            }

            // Check confirmation status
            match self.receipt_provider.get_transaction_status(tx_hash).await {
                Ok(Some(true)) => {
                    info!("Transaction {} confirmed", record.id);

                    // Collect Safe enrichment data before confirming, so that
                    // status and safe_execution are set atomically.
                    let safe_execution = self.try_enrich_safe_execution(&record).await;

                    if let Err(e) = self
                        .transaction_store
                        .confirm_with_safe_execution(record.id, safe_execution)
                    {
                        error!("Failed to confirm transaction {}: {}", record.id, e);
                    }
                }
                Ok(Some(false)) => {
                    info!("Transaction {} reverted", record.id);
                    if let Err(e) = self.transaction_store.update_status(
                        record.id,
                        TransactionStatus::Reverted,
                        Some("Transaction reverted on-chain".to_string()),
                    ) {
                        error!("Failed to update transaction {} status to Reverted: {}", record.id, e);
                    }
                }
                Ok(None) => {
                    // Still pending, continue monitoring
                    debug!("Transaction {} still pending", record.id);
                }
                Err(e) => {
                    error!("Error checking transaction {}: {}", record.id, e);
                }
            }

            // Rate limit RPC calls to prevent overwhelming the endpoint
            sleep(self.config.per_transaction_delay).await;
        }

        Ok(())
    }

    /// Attempt to extract Safe execution results for a confirmed transaction.
    ///
    /// Decodes the raw transaction to extract the `to` address, checks if it is a
    /// known Safe contract, and if so, fetches receipt logs to find
    /// ExecutionSuccess/ExecutionFailure events.
    ///
    /// Returns `Some(result)` if the transaction targets a known Safe and an
    /// execution event was found, `None` otherwise.
    async fn try_enrich_safe_execution(
        &self,
        record: &crate::transaction_store::TransactionRecord,
    ) -> Option<SafeExecutionResult> {
        let safe_checker = self.safe_checker.as_ref()?;

        // Decode the raw transaction to extract the `to` address
        let to_addr = decode_transaction_to_address(&record.raw_transaction)?;

        // Check if the `to` address is a known Safe contract
        if !safe_checker.is_known_safe(&to_addr).await {
            return None;
        }

        debug!(
            "Transaction {} targets a known Safe contract, fetching receipt logs",
            record.id
        );

        // Fetch receipt logs and look for Safe execution events
        match self
            .receipt_provider
            .get_transaction_receipt_logs(record.transaction_hash)
            .await
        {
            Ok(Some(logs)) => {
                let result = inspect_safe_execution_logs(&to_addr, &logs);
                if let Some(ref r) = result {
                    info!(
                        "Transaction {} Safe execution: success={}, safe_tx_hash={:?}",
                        record.id, r.success, r.safe_tx_hash
                    );
                }
                result
            }
            Ok(None) => {
                debug!("No receipt found for Safe tx {} (still pending)", record.id);
                None
            }
            Err(e) => {
                warn!("Failed to fetch receipt logs for Safe tx {}: {}", record.id, e);
                None
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use chrono::Utc;
    use dashmap::DashMap;
    use hopr_bindings::exports::alloy::{
        consensus::{SignableTransaction, TxLegacy},
        eips::eip2718::Encodable2718,
        primitives::{Address as AlloyAddress, TxKind, U256},
        signers::{Signer, local::PrivateKeySigner},
    };
    use uuid::Uuid;

    use super::*;
    use crate::transaction_store::TransactionRecord;

    // Mock receipt provider for testing with configurable statuses and logs
    struct MockReceiptProvider {
        // Map from tx_hash to status (Some(true) = confirmed, Some(false) = reverted, None = pending)
        statuses: Arc<DashMap<Hash, Option<bool>>>,
        // Map from tx_hash to receipt logs (or error).
        // Some(Ok(logs)) = receipt found with logs, Some(Err(..)) = RPC error, None = pending
        receipt_logs: Arc<DashMap<Hash, Result<Option<Vec<ReceiptLog>>, String>>>,
    }

    impl MockReceiptProvider {
        fn new() -> Self {
            Self {
                statuses: Arc::new(DashMap::new()),
                receipt_logs: Arc::new(DashMap::new()),
            }
        }

        fn set_status(&self, tx_hash: Hash, status: Option<bool>) {
            self.statuses.insert(tx_hash, status);
        }

        fn set_receipt_logs(&self, tx_hash: Hash, logs: Vec<ReceiptLog>) {
            self.receipt_logs.insert(tx_hash, Ok(Some(logs)));
        }

        fn set_receipt_logs_error(&self, tx_hash: Hash, error: String) {
            self.receipt_logs.insert(tx_hash, Err(error));
        }
    }

    #[async_trait]
    impl ReceiptProvider for MockReceiptProvider {
        async fn get_transaction_status(&self, tx_hash: Hash) -> Result<Option<bool>, String> {
            Ok(self.statuses.get(&tx_hash).map(|entry| *entry.value()).unwrap_or(None))
        }

        async fn get_transaction_receipt_logs(&self, tx_hash: Hash) -> Result<Option<Vec<ReceiptLog>>, String> {
            match self.receipt_logs.get(&tx_hash) {
                Some(entry) => entry.value().clone(),
                // No entry configured: receipt not found (pending)
                None => Ok(None),
            }
        }
    }

    // Mock Safe address checker for testing
    #[derive(Debug)]
    struct MockSafeAddressChecker {
        known_safes: Arc<DashMap<[u8; 20], ()>>,
    }

    impl MockSafeAddressChecker {
        fn new() -> Self {
            Self {
                known_safes: Arc::new(DashMap::new()),
            }
        }

        fn add_safe(&self, address: [u8; 20]) {
            self.known_safes.insert(address, ());
        }
    }

    #[async_trait]
    impl SafeAddressChecker for MockSafeAddressChecker {
        async fn is_known_safe(&self, address: &[u8; 20]) -> bool {
            self.known_safes.contains_key(address)
        }
    }

    /// Create a raw signed transaction targeting a specific address
    async fn create_raw_tx_to(target: &[u8; 20]) -> Vec<u8> {
        let signer = PrivateKeySigner::random();
        let tx = TxLegacy {
            chain_id: Some(1),
            nonce: 0,
            gas_price: 1_000_000_000,
            gas_limit: 21_000,
            to: TxKind::Call(AlloyAddress::from_slice(target)),
            value: U256::ZERO,
            input: Default::default(),
        };

        let tx_hash = tx.signature_hash();
        let signature = signer.sign_hash(&tx_hash).await.expect("signing failed");
        let signed_tx = tx.into_signed(signature);

        let mut encoded = Vec::new();
        signed_tx.encode_2718(&mut encoded);
        encoded
    }

    fn create_monitor(
        store: Arc<TransactionStore>,
        provider: MockReceiptProvider,
    ) -> TransactionMonitor<MockReceiptProvider> {
        TransactionMonitor::new(store, provider, TransactionMonitorConfig::default(), None)
    }

    fn create_monitor_with_safe_checker(
        store: Arc<TransactionStore>,
        provider: MockReceiptProvider,
        safe_checker: MockSafeAddressChecker,
    ) -> TransactionMonitor<MockReceiptProvider> {
        TransactionMonitor::new(
            store,
            provider,
            TransactionMonitorConfig::default(),
            Some(Arc::new(safe_checker)),
        )
    }

    #[tokio::test]
    async fn test_monitor_updates_confirmed_transaction() {
        let store = Arc::new(TransactionStore::new());
        let provider = MockReceiptProvider::new();

        let tx_hash = Hash::default();
        let record = TransactionRecord {
            id: Uuid::new_v4(),
            raw_transaction: vec![0x01],
            transaction_hash: tx_hash,
            status: TransactionStatus::Submitted,
            submitted_at: Utc::now(),
            confirmed_at: None,
            error_message: None,
            safe_execution: None,
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
            transaction_hash: tx_hash,
            status: TransactionStatus::Submitted,
            submitted_at: Utc::now(),
            confirmed_at: None,
            error_message: None,
            safe_execution: None,
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
            transaction_hash: tx_hash,
            status: TransactionStatus::Submitted,
            submitted_at: Utc::now(),
            confirmed_at: None,
            error_message: None,
            safe_execution: None,
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
            transaction_hash: tx_hash,
            status: TransactionStatus::Submitted,
            submitted_at: Utc::now() - chrono::Duration::try_seconds(400).unwrap(), // 400 seconds ago
            confirmed_at: None,
            error_message: None,
            safe_execution: None,
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
            transaction_hash: tx_hash1,
            status: TransactionStatus::Submitted,
            submitted_at: Utc::now(),
            confirmed_at: None,
            error_message: None,
            safe_execution: None,
        };

        let record2 = TransactionRecord {
            id: Uuid::new_v4(),
            raw_transaction: vec![0x02],
            transaction_hash: tx_hash2,
            status: TransactionStatus::Submitted,
            submitted_at: Utc::now(),
            confirmed_at: None,
            error_message: None,
            safe_execution: None,
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

    // =========================================================================
    // try_enrich_safe_execution tests
    // =========================================================================

    #[tokio::test]
    async fn test_enrich_safe_execution_succeeds_for_known_safe() {
        let safe_address = [0xAA; 20];
        let raw_tx = create_raw_tx_to(&safe_address).await;
        let tx_hash = Hash::from([0x11u8; 32]);

        let store = Arc::new(TransactionStore::new());
        let provider = MockReceiptProvider::new();

        // Set up receipt logs with ExecutionSuccess event
        let safe_tx_hash_bytes = [0x42u8; 32];
        let mut log_data = vec![0u8; 64];
        log_data[..32].copy_from_slice(&safe_tx_hash_bytes);

        provider.set_receipt_logs(
            tx_hash,
            vec![ReceiptLog {
                address: safe_address,
                topics: vec![crate::safe_execution::EXECUTION_SUCCESS_TOPIC],
                data: log_data,
            }],
        );

        let safe_checker = MockSafeAddressChecker::new();
        safe_checker.add_safe(safe_address);

        let record = TransactionRecord {
            id: Uuid::new_v4(),
            raw_transaction: raw_tx,
            transaction_hash: tx_hash,
            status: TransactionStatus::Submitted,
            submitted_at: Utc::now(),
            confirmed_at: None,
            error_message: None,
            safe_execution: None,
        };

        store.insert(record.clone()).unwrap();

        let monitor = create_monitor_with_safe_checker(store, provider, safe_checker);
        let result = monitor.try_enrich_safe_execution(&record).await;

        let exec = result.expect("enrichment should return a result");
        assert!(exec.success);
        assert!(exec.safe_tx_hash.is_some());
    }

    #[tokio::test]
    async fn test_enrich_safe_execution_receipt_fetch_fails() {
        let safe_address = [0xBB; 20];
        let raw_tx = create_raw_tx_to(&safe_address).await;
        let tx_hash = Hash::from([0x22u8; 32]);

        let store = Arc::new(TransactionStore::new());
        let provider = MockReceiptProvider::new();

        // Set receipt logs to return an error
        provider.set_receipt_logs_error(tx_hash, "RPC timeout".to_string());

        let safe_checker = MockSafeAddressChecker::new();
        safe_checker.add_safe(safe_address);

        let record = TransactionRecord {
            id: Uuid::new_v4(),
            raw_transaction: raw_tx,
            transaction_hash: tx_hash,
            status: TransactionStatus::Submitted,
            submitted_at: Utc::now(),
            confirmed_at: None,
            error_message: None,
            safe_execution: None,
        };

        store.insert(record.clone()).unwrap();

        let monitor = create_monitor_with_safe_checker(store, provider, safe_checker);
        let result = monitor.try_enrich_safe_execution(&record).await;

        assert!(result.is_none(), "should return None when receipt fetch fails");
    }

    #[tokio::test]
    async fn test_enrich_safe_execution_unknown_address() {
        let target_address = [0xCC; 20];
        let raw_tx = create_raw_tx_to(&target_address).await;
        let tx_hash = Hash::from([0x33u8; 32]);

        let store = Arc::new(TransactionStore::new());
        let provider = MockReceiptProvider::new();

        // Safe checker does NOT recognize this address
        let safe_checker = MockSafeAddressChecker::new();

        let record = TransactionRecord {
            id: Uuid::new_v4(),
            raw_transaction: raw_tx,
            transaction_hash: tx_hash,
            status: TransactionStatus::Submitted,
            submitted_at: Utc::now(),
            confirmed_at: None,
            error_message: None,
            safe_execution: None,
        };

        store.insert(record.clone()).unwrap();

        let monitor = create_monitor_with_safe_checker(store, provider, safe_checker);
        let result = monitor.try_enrich_safe_execution(&record).await;

        assert!(result.is_none(), "should return None for unknown addresses");
    }

    #[tokio::test]
    async fn test_enrich_safe_execution_no_safe_checker() {
        let store = Arc::new(TransactionStore::new());
        let provider = MockReceiptProvider::new();

        let record = TransactionRecord {
            id: Uuid::new_v4(),
            raw_transaction: vec![0x01],
            transaction_hash: Hash::default(),
            status: TransactionStatus::Submitted,
            submitted_at: Utc::now(),
            confirmed_at: None,
            error_message: None,
            safe_execution: None,
        };

        store.insert(record.clone()).unwrap();

        // Monitor without safe_checker (None)
        let monitor = create_monitor(store, provider);
        let result = monitor.try_enrich_safe_execution(&record).await;

        assert!(result.is_none(), "should return None when safe_checker is None");
    }

    #[tokio::test]
    async fn test_poll_once_confirms_with_safe_execution_atomically() {
        let safe_address = [0xDD; 20];
        let raw_tx = create_raw_tx_to(&safe_address).await;
        let tx_hash = Hash::from([0x44u8; 32]);

        let store = Arc::new(TransactionStore::new());
        let provider = MockReceiptProvider::new();

        // Transaction is confirmed
        provider.set_status(tx_hash, Some(true));

        // Set up receipt logs with ExecutionSuccess event
        let mut log_data = vec![0u8; 64];
        log_data[0] = 0xEE;

        provider.set_receipt_logs(
            tx_hash,
            vec![ReceiptLog {
                address: safe_address,
                topics: vec![crate::safe_execution::EXECUTION_SUCCESS_TOPIC],
                data: log_data,
            }],
        );

        let safe_checker = MockSafeAddressChecker::new();
        safe_checker.add_safe(safe_address);

        let record = TransactionRecord {
            id: Uuid::new_v4(),
            raw_transaction: raw_tx,
            transaction_hash: tx_hash,
            status: TransactionStatus::Submitted,
            submitted_at: Utc::now(),
            confirmed_at: None,
            error_message: None,
            safe_execution: None,
        };

        let id = record.id;
        store.insert(record).unwrap();

        let monitor = create_monitor_with_safe_checker(store.clone(), provider, safe_checker);
        monitor.poll_once().await.unwrap();

        // Both status and safe_execution should be set atomically
        let updated = store.get(id).unwrap();
        assert_eq!(updated.status, TransactionStatus::Confirmed);
        assert!(updated.confirmed_at.is_some());

        let exec = updated
            .safe_execution
            .expect("safe_execution should be set alongside confirmed status");
        assert!(exec.success);
    }
}
