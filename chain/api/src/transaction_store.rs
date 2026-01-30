//! In-memory transaction store for tracking submitted transactions
//!
//! This module provides a thread-safe in-memory store for tracking raw transactions
//! submitted through the GraphQL API. Transactions are stored with their submission
//! status and can be queried by UUID.

use std::sync::Arc;

use async_graphql::ID;
use blokli_api_types::{Hex32, SafeExecution, Transaction, TransactionStatus as GqlTransactionStatus};
use chrono::{DateTime, Utc};
use dashmap::{DashMap, mapref::entry::Entry};
use hopr_crypto_types::types::Hash;
use thiserror::Error;
use uuid::Uuid;

/// Errors that can occur when working with the transaction store
#[derive(Error, Debug, Clone, PartialEq)]
pub enum TransactionStoreError {
    #[error("Transaction not found: {0}")]
    NotFound(Uuid),
    #[error("Transaction already exists: {0}")]
    AlreadyExists(Uuid),
}

/// Result of internal Safe contract execution
///
/// Populated after a transaction targeting a Safe contract is confirmed on-chain.
/// Extracted from ExecutionSuccess/ExecutionFailure events in the receipt logs.
#[derive(Debug, Clone)]
pub struct SafeExecutionResult {
    /// Whether the internal Safe transaction succeeded
    pub success: bool,
    /// Safe internal transaction hash (bytes32 from event)
    pub safe_tx_hash: Hash,
    /// Revert reason string (if execution failed and reason is decodable)
    pub revert_reason: Option<String>,
}

/// Status of a submitted transaction
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransactionStatus {
    /// Transaction is pending submission to the chain
    Pending,
    /// Transaction has been submitted and is awaiting confirmation
    Submitted,
    /// Transaction has been confirmed on-chain with success
    Confirmed,
    /// Transaction was included on-chain but reverted (receipt.status = 0)
    Reverted,
    /// Transaction was not mined within timeout window
    Timeout,
    /// Transaction validation failed
    ValidationFailed,
    /// Transaction submission failed
    SubmissionFailed,
}

/// Record of a submitted transaction
#[derive(Debug, Clone)]
pub struct TransactionRecord {
    /// Unique identifier for the transaction
    pub id: Uuid,
    /// Raw signed transaction data
    pub raw_transaction: Vec<u8>,
    /// Transaction hash from successful blockchain submission
    pub transaction_hash: Hash,
    /// Current status of the transaction
    pub status: TransactionStatus,
    /// Timestamp when transaction was submitted
    pub submitted_at: DateTime<Utc>,
    /// Timestamp when transaction was confirmed (if applicable)
    pub confirmed_at: Option<DateTime<Utc>>,
    /// Error message (if submission or confirmation failed)
    pub error_message: Option<String>,
    /// Internal Safe execution result (populated after confirmation for Safe transactions)
    pub safe_execution: Option<SafeExecutionResult>,
}

impl From<TransactionStatus> for GqlTransactionStatus {
    fn from(status: TransactionStatus) -> Self {
        match status {
            TransactionStatus::Pending => GqlTransactionStatus::Pending,
            TransactionStatus::Submitted => GqlTransactionStatus::Submitted,
            TransactionStatus::Confirmed => GqlTransactionStatus::Confirmed,
            TransactionStatus::Reverted => GqlTransactionStatus::Reverted,
            TransactionStatus::Timeout => GqlTransactionStatus::Timeout,
            TransactionStatus::ValidationFailed => GqlTransactionStatus::ValidationFailed,
            TransactionStatus::SubmissionFailed => GqlTransactionStatus::SubmissionFailed,
        }
    }
}

impl From<TransactionRecord> for Transaction {
    fn from(record: TransactionRecord) -> Self {
        let safe_execution = record.safe_execution.map(|se| SafeExecution {
            success: se.success,
            safe_tx_hash: Hex32::from(se.safe_tx_hash),
            revert_reason: se.revert_reason,
        });

        Transaction {
            id: ID::from(record.id.to_string()),
            status: record.status.into(),
            submitted_at: record.submitted_at,
            transaction_hash: record.transaction_hash.into(),
            safe_execution,
        }
    }
}

/// Thread-safe in-memory store for transaction records
#[derive(Debug, Clone)]
pub struct TransactionStore {
    transactions: Arc<DashMap<Uuid, TransactionRecord>>,
}

impl TransactionStore {
    /// Create a new empty transaction store
    pub fn new() -> Self {
        Self {
            transactions: Arc::new(DashMap::new()),
        }
    }

    /// Insert a new transaction record into the store
    ///
    /// # Errors
    /// Returns `TransactionStoreError::AlreadyExists` if a transaction with the same ID already exists
    pub fn insert(&self, record: TransactionRecord) -> Result<(), TransactionStoreError> {
        match self.transactions.entry(record.id) {
            Entry::Vacant(entry) => {
                entry.insert(record);
                Ok(())
            }
            Entry::Occupied(entry) => Err(TransactionStoreError::AlreadyExists(*entry.key())),
        }
    }

    /// Get a transaction record by its UUID
    ///
    /// # Errors
    /// Returns `TransactionStoreError::NotFound` if the transaction doesn't exist
    pub fn get(&self, id: Uuid) -> Result<TransactionRecord, TransactionStoreError> {
        self.transactions
            .get(&id)
            .map(|entry| entry.value().clone())
            .ok_or(TransactionStoreError::NotFound(id))
    }

    /// Update an existing transaction record
    ///
    /// # Errors
    /// Returns `TransactionStoreError::NotFound` if the transaction doesn't exist
    pub fn update(&self, record: TransactionRecord) -> Result<(), TransactionStoreError> {
        match self.transactions.entry(record.id) {
            Entry::Occupied(mut entry) => {
                entry.insert(record);
                Ok(())
            }
            Entry::Vacant(_) => Err(TransactionStoreError::NotFound(record.id)),
        }
    }

    /// Update the status of a transaction
    ///
    /// # Errors
    /// Returns `TransactionStoreError::NotFound` if the transaction doesn't exist
    pub fn update_status(
        &self,
        id: Uuid,
        status: TransactionStatus,
        error_message: Option<String>,
    ) -> Result<(), TransactionStoreError> {
        self.transactions
            .get_mut(&id)
            .map(|mut entry| {
                let record = entry.value_mut();
                record.status = status;
                record.error_message = error_message;

                // Set confirmed_at timestamp if status is Confirmed
                if status == TransactionStatus::Confirmed && record.confirmed_at.is_none() {
                    record.confirmed_at = Some(Utc::now());
                }
            })
            .ok_or(TransactionStoreError::NotFound(id))
    }

    /// Update the Safe execution result for a transaction
    ///
    /// # Errors
    /// Returns `TransactionStoreError::NotFound` if the transaction doesn't exist
    pub fn update_safe_execution(
        &self,
        id: Uuid,
        safe_execution: SafeExecutionResult,
    ) -> Result<(), TransactionStoreError> {
        self.transactions
            .get_mut(&id)
            .map(|mut entry| {
                entry.value_mut().safe_execution = Some(safe_execution);
            })
            .ok_or(TransactionStoreError::NotFound(id))
    }

    /// List all transactions with a specific status
    pub fn list_by_status(&self, status: TransactionStatus) -> Vec<TransactionRecord> {
        self.transactions
            .iter()
            .filter(|entry| entry.value().status == status)
            .map(|entry| entry.value().clone())
            .collect()
    }

    /// Get the total count of transactions in the store
    pub fn count(&self) -> usize {
        self.transactions.len()
    }
}

impl Default for TransactionStore {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use std::thread;

    use super::*;

    #[test]
    fn test_create_store_and_insert_transaction() {
        let store = TransactionStore::new();

        let record = TransactionRecord {
            id: Uuid::new_v4(),
            raw_transaction: vec![0x01, 0x02, 0x03],
            transaction_hash: Hash::default(),
            status: TransactionStatus::Submitted,
            submitted_at: Utc::now(),
            confirmed_at: None,
            error_message: None,
            safe_execution: None,
        };

        let id = record.id;
        assert!(store.insert(record).is_ok());
        assert_eq!(store.count(), 1);

        // Verify we can retrieve it
        let retrieved = store.get(id).unwrap();
        assert_eq!(retrieved.id, id);
        assert_eq!(retrieved.status, TransactionStatus::Submitted);
    }

    #[test]
    fn test_insert_duplicate_transaction_fails() {
        let store = TransactionStore::new();

        let id = Uuid::new_v4();
        let record = TransactionRecord {
            id,
            raw_transaction: vec![0x01, 0x02, 0x03],
            transaction_hash: Hash::default(),
            status: TransactionStatus::Submitted,
            submitted_at: Utc::now(),
            confirmed_at: None,
            error_message: None,
            safe_execution: None,
        };

        assert!(store.insert(record.clone()).is_ok());

        // Try to insert again with same ID
        let result = store.insert(record);
        assert!(matches!(result, Err(TransactionStoreError::AlreadyExists(_))));
    }

    #[test]
    fn test_retrieve_nonexistent_transaction() {
        let store = TransactionStore::new();
        let id = Uuid::new_v4();

        let result = store.get(id);
        assert!(matches!(result, Err(TransactionStoreError::NotFound(_))));
    }

    #[test]
    fn test_update_transaction_status() {
        let store = TransactionStore::new();

        let record = TransactionRecord {
            id: Uuid::new_v4(),
            raw_transaction: vec![0x01, 0x02, 0x03],
            transaction_hash: Hash::default(),
            status: TransactionStatus::Submitted,
            submitted_at: Utc::now(),
            confirmed_at: None,
            error_message: None,
            safe_execution: None,
        };

        let id = record.id;
        store.insert(record).unwrap();

        // Verify initial status
        let retrieved = store.get(id).unwrap();
        assert_eq!(retrieved.status, TransactionStatus::Submitted);
        assert!(retrieved.confirmed_at.is_none());

        // Update status to Confirmed
        store.update_status(id, TransactionStatus::Confirmed, None).unwrap();

        let retrieved = store.get(id).unwrap();
        assert_eq!(retrieved.status, TransactionStatus::Confirmed);
        assert!(retrieved.confirmed_at.is_some());
    }

    #[test]
    fn test_update_nonexistent_transaction_fails() {
        let store = TransactionStore::new();
        let id = Uuid::new_v4();

        let result = store.update_status(id, TransactionStatus::Confirmed, None);
        assert!(matches!(result, Err(TransactionStoreError::NotFound(_))));
    }

    #[test]
    fn test_list_by_status() {
        let store = TransactionStore::new();

        // Insert transactions with different statuses
        for i in 0..5 {
            let status = if i % 2 == 0 {
                TransactionStatus::Submitted
            } else {
                TransactionStatus::Confirmed
            };

            let record = TransactionRecord {
                id: Uuid::new_v4(),
                raw_transaction: vec![i as u8],
                transaction_hash: Hash::default(),
                status,
                submitted_at: Utc::now(),
                confirmed_at: None,
                error_message: None,
                safe_execution: None,
            };

            store.insert(record).unwrap();
        }

        let submitted = store.list_by_status(TransactionStatus::Submitted);
        assert_eq!(submitted.len(), 3); // 0, 2, 4

        let confirmed = store.list_by_status(TransactionStatus::Confirmed);
        assert_eq!(confirmed.len(), 2); // 1, 3
    }

    #[test]
    fn test_concurrent_operations() {
        let store = TransactionStore::new();
        let store_clone1 = store.clone();
        let store_clone2 = store.clone();

        // Thread 1: Insert 5 transactions
        let handle1 = thread::spawn(move || {
            for i in 0..5 {
                let record = TransactionRecord {
                    id: Uuid::new_v4(),
                    raw_transaction: vec![i],
                    transaction_hash: Hash::default(),
                    status: TransactionStatus::Submitted,
                    submitted_at: Utc::now(),
                    confirmed_at: None,
                    error_message: None,
                    safe_execution: None,
                };
                store_clone1.insert(record).unwrap();
            }
        });

        // Thread 2: Insert 5 more transactions
        let handle2 = thread::spawn(move || {
            for i in 5..10 {
                let record = TransactionRecord {
                    id: Uuid::new_v4(),
                    raw_transaction: vec![i],
                    transaction_hash: Hash::default(),
                    status: TransactionStatus::Submitted,
                    submitted_at: Utc::now(),
                    confirmed_at: None,
                    error_message: None,
                    safe_execution: None,
                };
                store_clone2.insert(record).unwrap();
            }
        });

        handle1.join().unwrap();
        handle2.join().unwrap();

        // Verify all 10 transactions were inserted
        assert_eq!(store.count(), 10);
    }

    #[test]
    fn test_update_full_record() {
        let store = TransactionStore::new();

        let record = TransactionRecord {
            id: Uuid::new_v4(),
            raw_transaction: vec![0x01, 0x02, 0x03],
            transaction_hash: Hash::default(),
            status: TransactionStatus::Submitted,
            submitted_at: Utc::now(),
            confirmed_at: None,
            error_message: None,
            safe_execution: None,
        };

        let id = record.id;
        store.insert(record).unwrap();

        // Update the full record
        let updated_record = TransactionRecord {
            id,
            raw_transaction: vec![0x01, 0x02, 0x03],
            transaction_hash: Hash::default(),
            status: TransactionStatus::Confirmed,
            submitted_at: Utc::now(),
            confirmed_at: Some(Utc::now()),
            error_message: None,
            safe_execution: None,
        };

        store.update(updated_record.clone()).unwrap();

        let retrieved = store.get(id).unwrap();
        assert_eq!(retrieved.status, TransactionStatus::Confirmed);
        assert_eq!(retrieved.transaction_hash, Hash::default());
        assert!(retrieved.confirmed_at.is_some());
    }
}
