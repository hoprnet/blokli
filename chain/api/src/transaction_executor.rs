//! Raw transaction executor for submitting pre-signed transactions to the chain
//!
//! This module provides the core functionality for submitting raw transactions to the blockchain
//! in three different modes:
//! - Fire-and-forget: Submit and return hash immediately
//! - Async: Submit, track in store, return UUID for later querying
//! - Sync: Submit and wait for confirmations before returning

use std::{sync::Arc, time::Duration};

use async_trait::async_trait;
use chrono::Utc;
use hopr_crypto_types::types::Hash;
use thiserror::Error;
use uuid::Uuid;

use crate::{
    transaction_store::{TransactionRecord, TransactionStatus, TransactionStore, TransactionStoreError},
    transaction_validator::{TransactionValidator, ValidationError},
};

/// Errors that can occur during transaction execution
#[derive(Error, Debug)]
pub enum TransactionExecutorError {
    #[error("Validation failed: {0}")]
    ValidationFailed(#[from] ValidationError),
    #[error("RPC error: {0}")]
    RpcError(String),
    #[error("Transaction store error: {0}")]
    StoreError(#[from] TransactionStoreError),
    #[error("Transaction timed out")]
    Timeout,
    #[error("Transaction execution failed: {0}")]
    ExecutionFailed(String),
}

/// Trait for RPC operations needed by the transaction executor
///
/// This trait abstracts the RPC client so we can mock it in tests
#[async_trait]
pub trait RpcClient: Send + Sync {
    /// Send a raw transaction and return its hash
    async fn send_raw_transaction(&self, raw_tx: Vec<u8>) -> Result<Hash, String>;

    /// Send a raw transaction and wait for confirmations
    async fn send_raw_transaction_with_confirm(
        &self,
        raw_tx: Vec<u8>,
        confirmations: u64,
        timeout: Option<Duration>,
    ) -> Result<Hash, String>;
}

/// Configuration for the raw transaction executor
#[derive(Debug, Clone)]
pub struct RawTransactionExecutorConfig {
    /// Default number of confirmations to wait for in sync mode
    pub default_confirmations: u64,
    /// Maximum time to wait for confirmations
    pub confirmation_timeout: Duration,
}

impl Default for RawTransactionExecutorConfig {
    fn default() -> Self {
        Self {
            default_confirmations: 8,
            confirmation_timeout: Duration::from_secs(150),
        }
    }
}

/// Executor for submitting raw transactions to the blockchain
///
/// Supports three submission modes:
/// - Fire-and-forget: Returns tx hash immediately, no tracking
/// - Async: Returns UUID, tracks in store, background monitoring
/// - Sync: Waits for confirmations, returns complete transaction record
#[derive(Debug)]
pub struct RawTransactionExecutor<R: RpcClient> {
    rpc_client: Arc<R>,
    transaction_store: Arc<TransactionStore>,
    validator: Arc<TransactionValidator>,
    config: RawTransactionExecutorConfig,
}

impl<R: RpcClient> RawTransactionExecutor<R> {
    /// Create a new raw transaction executor
    pub fn new(
        rpc_client: R,
        transaction_store: TransactionStore,
        validator: TransactionValidator,
        config: RawTransactionExecutorConfig,
    ) -> Self {
        Self {
            rpc_client: Arc::new(rpc_client),
            transaction_store: Arc::new(transaction_store),
            validator: Arc::new(validator),
            config,
        }
    }

    /// Create a new raw transaction executor with shared Arc-wrapped dependencies
    ///
    /// This constructor is useful when you need multiple components to share the same
    /// transaction store instance (e.g., executor and monitor).
    pub fn with_shared_dependencies(
        rpc_client: Arc<R>,
        transaction_store: Arc<TransactionStore>,
        validator: Arc<TransactionValidator>,
        config: RawTransactionExecutorConfig,
    ) -> Self {
        Self {
            rpc_client,
            transaction_store,
            validator,
            config,
        }
    }

    /// Fire-and-forget mode: Submit transaction and return hash immediately
    ///
    /// This mode:
    /// - Validates the transaction
    /// - Submits to RPC
    /// - Returns transaction hash
    /// - Does NOT track in database
    /// - Does NOT wait for confirmation
    pub async fn send_raw_transaction(&self, raw_tx: Vec<u8>) -> Result<Hash, TransactionExecutorError> {
        // Validate transaction
        self.validator.validate_raw_transaction(&raw_tx)?;

        // Submit to RPC
        let tx_hash = self
            .rpc_client
            .send_raw_transaction(raw_tx)
            .await
            .map_err(TransactionExecutorError::RpcError)?;

        Ok(tx_hash)
    }

    /// Async mode: Submit transaction, track in store, return UUID
    ///
    /// This mode:
    /// - Validates the transaction
    /// - Submits to RPC and obtains transaction hash
    /// - Creates transaction record in store with hash
    /// - Returns UUID for later querying
    /// - Background monitor handles confirmation tracking
    pub async fn send_raw_transaction_async(&self, raw_tx: Vec<u8>) -> Result<Uuid, TransactionExecutorError> {
        // Validate transaction
        self.validator.validate_raw_transaction(&raw_tx)?;

        // Submit to RPC first to get transaction hash
        let tx_hash = self
            .rpc_client
            .send_raw_transaction(raw_tx.clone())
            .await
            .map_err(TransactionExecutorError::RpcError)?;

        // Create transaction record with hash and Submitted status
        let id = Uuid::new_v4();
        let record = TransactionRecord {
            id,
            raw_transaction: raw_tx,
            transaction_hash: tx_hash,
            status: TransactionStatus::Submitted,
            submitted_at: Utc::now(),
            confirmed_at: None,
            error_message: None,
        };

        self.transaction_store.insert(record)?;
        Ok(id)
    }

    /// Sync mode: Submit transaction and wait for confirmations
    ///
    /// This mode:
    /// - Validates the transaction
    /// - Submits to RPC and waits for confirmations
    /// - Creates transaction record in store with result
    /// - Returns complete transaction record
    pub async fn send_raw_transaction_sync(
        &self,
        raw_tx: Vec<u8>,
        confirmations: Option<u64>,
    ) -> Result<TransactionRecord, TransactionExecutorError> {
        // Validate transaction
        self.validator.validate_raw_transaction(&raw_tx)?;

        let confirmations = confirmations.unwrap_or(self.config.default_confirmations);

        // Submit to RPC with confirmation wait
        let tx_hash = self
            .rpc_client
            .send_raw_transaction_with_confirm(raw_tx.clone(), confirmations, Some(self.config.confirmation_timeout))
            .await
            .map_err(TransactionExecutorError::RpcError)?;

        // Create transaction record with confirmed status
        let id = Uuid::new_v4();
        let record = TransactionRecord {
            id,
            raw_transaction: raw_tx,
            transaction_hash: tx_hash,
            status: TransactionStatus::Confirmed,
            submitted_at: Utc::now(),
            confirmed_at: Some(Utc::now()),
            error_message: None,
        };

        self.transaction_store.insert(record.clone())?;
        Ok(record)
    }

    /// Get transaction store for querying
    pub fn transaction_store(&self) -> Arc<TransactionStore> {
        self.transaction_store.clone()
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;

    // Mock RPC client for testing
    struct MockRpcClient {
        should_fail: bool,
        should_timeout: bool,
        call_count: Arc<Mutex<usize>>,
    }

    impl MockRpcClient {
        fn new() -> Self {
            Self {
                should_fail: false,
                should_timeout: false,
                call_count: Arc::new(Mutex::new(0)),
            }
        }

        fn with_failure() -> Self {
            Self {
                should_fail: true,
                should_timeout: false,
                call_count: Arc::new(Mutex::new(0)),
            }
        }

        fn with_timeout() -> Self {
            Self {
                should_fail: false,
                should_timeout: true,
                call_count: Arc::new(Mutex::new(0)),
            }
        }

        #[allow(dead_code)]
        fn call_count(&self) -> usize {
            *self.call_count.lock().unwrap()
        }
    }

    #[async_trait]
    impl RpcClient for MockRpcClient {
        async fn send_raw_transaction(&self, _raw_tx: Vec<u8>) -> Result<Hash, String> {
            *self.call_count.lock().unwrap() += 1;

            if self.should_fail {
                return Err("RPC error: transaction failed".to_string());
            }

            Ok(Hash::default())
        }

        async fn send_raw_transaction_with_confirm(
            &self,
            _raw_tx: Vec<u8>,
            _confirmations: u64,
            _timeout: Option<Duration>,
        ) -> Result<Hash, String> {
            *self.call_count.lock().unwrap() += 1;

            if self.should_fail {
                return Err("RPC error: transaction failed".to_string());
            }

            if self.should_timeout {
                return Err("RPC error: timeout waiting for confirmation".to_string());
            }

            Ok(Hash::default())
        }
    }

    fn create_executor(rpc_client: MockRpcClient) -> RawTransactionExecutor<MockRpcClient> {
        RawTransactionExecutor::new(
            rpc_client,
            TransactionStore::new(),
            TransactionValidator::new(),
            RawTransactionExecutorConfig::default(),
        )
    }

    #[tokio::test]
    async fn test_fire_and_forget_returns_hash() {
        let executor = create_executor(MockRpcClient::new());
        let raw_tx = vec![0x01, 0x02, 0x03];

        let result = executor.send_raw_transaction(raw_tx).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), Hash::default());
    }

    #[tokio::test]
    async fn test_fire_and_forget_does_not_store() {
        let executor = create_executor(MockRpcClient::new());
        let raw_tx = vec![0x01, 0x02, 0x03];

        executor.send_raw_transaction(raw_tx).await.unwrap();

        // Verify nothing was stored
        assert_eq!(executor.transaction_store().count(), 0);
    }

    #[tokio::test]
    async fn test_fire_and_forget_validates() {
        let executor = create_executor(MockRpcClient::new());
        let empty_tx = vec![];

        let result = executor.send_raw_transaction(empty_tx).await;
        assert!(matches!(result, Err(TransactionExecutorError::ValidationFailed(_))));
    }

    #[tokio::test]
    async fn test_async_mode_returns_uuid() {
        let executor = create_executor(MockRpcClient::new());
        let raw_tx = vec![0x01, 0x02, 0x03];

        let result = executor.send_raw_transaction_async(raw_tx).await;
        assert!(result.is_ok());

        let uuid = result.unwrap();
        assert_ne!(uuid, Uuid::nil());
    }

    #[tokio::test]
    async fn test_async_mode_stores_transaction() {
        let executor = create_executor(MockRpcClient::new());
        let raw_tx = vec![0x01, 0x02, 0x03];

        let uuid = executor.send_raw_transaction_async(raw_tx).await.unwrap();

        // Verify it was stored
        let record = executor.transaction_store().get(uuid).unwrap();
        assert_eq!(record.id, uuid);
        assert_eq!(record.status, TransactionStatus::Submitted);
        assert_eq!(record.transaction_hash, Hash::default());
    }

    #[tokio::test]
    async fn test_async_mode_handles_rpc_error() {
        let executor = create_executor(MockRpcClient::with_failure());
        let raw_tx = vec![0x01, 0x02, 0x03];

        let result = executor.send_raw_transaction_async(raw_tx).await;
        assert!(matches!(result, Err(TransactionExecutorError::RpcError(_))));

        // Verify no record was created on RPC failure
        assert_eq!(executor.transaction_store().count(), 0);
    }

    #[tokio::test]
    async fn test_sync_mode_waits_for_confirmations() {
        let executor = create_executor(MockRpcClient::new());
        let raw_tx = vec![0x01, 0x02, 0x03];

        let result = executor.send_raw_transaction_sync(raw_tx, None).await;
        assert!(result.is_ok());

        let record = result.unwrap();
        assert_eq!(record.status, TransactionStatus::Confirmed);
        assert_eq!(record.transaction_hash, Hash::default());
        assert!(record.confirmed_at.is_some());
    }

    #[tokio::test]
    async fn test_sync_mode_handles_timeout() {
        let executor = create_executor(MockRpcClient::with_timeout());
        let raw_tx = vec![0x01, 0x02, 0x03];

        let result = executor.send_raw_transaction_sync(raw_tx, None).await;
        assert!(matches!(result, Err(TransactionExecutorError::RpcError(_))));

        // Verify no record was created on RPC failure
        assert_eq!(executor.transaction_store().count(), 0);
    }

    #[tokio::test]
    async fn test_sync_mode_uses_custom_confirmations() {
        let rpc_client = MockRpcClient::new();
        let executor = create_executor(rpc_client);
        let raw_tx = vec![0x01, 0x02, 0x03];

        let result = executor.send_raw_transaction_sync(raw_tx, Some(16)).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_concurrent_async_submissions() {
        let executor = Arc::new(create_executor(MockRpcClient::new()));

        let mut handles = vec![];
        for i in 0..10 {
            let executor = executor.clone();
            let handle = tokio::spawn(async move {
                let raw_tx = vec![i as u8];
                executor.send_raw_transaction_async(raw_tx).await
            });
            handles.push(handle);
        }

        for handle in handles {
            assert!(handle.await.unwrap().is_ok());
        }

        // Verify all 10 were stored
        assert_eq!(executor.transaction_store().count(), 10);
    }

    #[tokio::test]
    async fn test_transaction_status_transitions() {
        let executor = create_executor(MockRpcClient::new());
        let raw_tx = vec![0x01, 0x02, 0x03];

        // Submit async
        let uuid = executor.send_raw_transaction_async(raw_tx).await.unwrap();

        // Check initial status
        let record = executor.transaction_store().get(uuid).unwrap();
        assert_eq!(record.status, TransactionStatus::Submitted);

        // Verify can retrieve by UUID
        let retrieved = executor.transaction_store().get(uuid).unwrap();
        assert_eq!(retrieved.id, uuid);
    }
}
