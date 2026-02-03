//! In-memory transaction store for tracking submitted transactions
//!
//! This module provides a thread-safe in-memory store for tracking raw transactions
//! submitted through the GraphQL API. Transactions are stored with their submission
//! status and can be queried by UUID.

use std::sync::Arc;

use async_broadcast::{Receiver, Sender, broadcast};
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

/// Event type for transaction status updates
///
/// Represents transaction status changes that should be broadcast to subscribers.
#[derive(Clone, Debug)]
pub enum TransactionEvent {
    /// Transaction status was updated
    ///
    /// Contains the transaction ID and the complete updated record
    StatusUpdated { id: Uuid, record: TransactionRecord },
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
}

/// Thread-safe in-memory store for transaction records
#[derive(Clone)]
pub struct TransactionStore {
    transactions: Arc<DashMap<Uuid, TransactionRecord>>,
    /// Event bus sender for broadcasting transaction status updates
    event_bus: Sender<TransactionEvent>,
    /// Initial receiver kept alive to maintain channel state
    ///
    /// This receiver is never used but must be kept alive to prevent the
    /// async_broadcast channel from entering a closed state.
    _event_bus_rx: Arc<Receiver<TransactionEvent>>,
}

impl std::fmt::Debug for TransactionStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TransactionStore")
            .field("transactions", &self.transactions)
            .field("event_bus", &"Sender<TransactionEvent>")
            .field("_event_bus_rx", &"Arc<Receiver<TransactionEvent>>")
            .finish()
    }
}

impl TransactionStore {
    /// Create a new empty transaction store
    ///
    /// Creates a transaction store with an event bus capacity of 100 events.
    pub fn new() -> Self {
        Self::with_capacity(100)
    }

    /// Create a new transaction store with specified event bus capacity
    ///
    /// # Arguments
    ///
    /// * `event_bus_capacity` - Capacity of the event bus channel
    ///
    /// # Returns
    ///
    /// A new TransactionStore instance ready for use
    pub fn with_capacity(event_bus_capacity: usize) -> Self {
        // Create broadcast channel, keeping the initial receiver alive
        let (mut event_bus, event_bus_rx) = broadcast(event_bus_capacity);

        // Set overflow behavior to allow new receivers to miss old messages if they can't keep up
        event_bus.set_overflow(true);

        Self {
            transactions: Arc::new(DashMap::new()),
            event_bus,
            _event_bus_rx: Arc::new(event_bus_rx),
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
    /// Publishes a `TransactionEvent::StatusUpdated` event to all subscribers
    /// after the status is successfully updated.
    ///
    /// # Errors
    /// Returns `TransactionStoreError::NotFound` if the transaction doesn't exist
    pub fn update_status(
        &self,
        id: Uuid,
        status: TransactionStatus,
        error_message: Option<String>,
    ) -> Result<(), TransactionStoreError> {
        // Update the transaction and get the updated record
        let updated_record = self
            .transactions
            .get_mut(&id)
            .map(|mut entry| {
                let record = entry.value_mut();
                record.status = status;
                record.error_message = error_message;

                // Set confirmed_at timestamp if status is Confirmed
                if status == TransactionStatus::Confirmed && record.confirmed_at.is_none() {
                    record.confirmed_at = Some(Utc::now());
                }

                // Clone the updated record for event broadcasting
                record.clone()
            })
            .ok_or(TransactionStoreError::NotFound(id))?;

        // Publish event to subscribers
        let _ = self.event_bus.try_broadcast(TransactionEvent::StatusUpdated {
            id,
            record: updated_record,
        });

        Ok(())
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

    /// Subscribe to transaction status update events
    ///
    /// Creates a new receiver that will receive all future transaction status updates.
    /// Each status update will broadcast a `TransactionEvent::StatusUpdated` event
    /// containing the transaction ID and the complete updated record.
    ///
    /// # Returns
    ///
    /// A receiver for transaction events
    pub fn subscribe(&self) -> Receiver<TransactionEvent> {
        // Get a fresh receiver from the sender to avoid inheriting backlog
        self.event_bus.new_receiver()
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
        };

        store.update(updated_record.clone()).unwrap();

        let retrieved = store.get(id).unwrap();
        assert_eq!(retrieved.status, TransactionStatus::Confirmed);
        assert_eq!(retrieved.transaction_hash, Hash::default());
        assert!(retrieved.confirmed_at.is_some());
    }

    #[tokio::test]
    async fn test_event_publishing_on_status_update() {
        let store = TransactionStore::new();

        // Subscribe before inserting transaction
        let mut receiver = store.subscribe();

        // Insert a transaction
        let record = TransactionRecord {
            id: Uuid::new_v4(),
            raw_transaction: vec![0x01, 0x02, 0x03],
            transaction_hash: Hash::default(),
            status: TransactionStatus::Pending,
            submitted_at: Utc::now(),
            confirmed_at: None,
            error_message: None,
        };

        let id = record.id;
        store.insert(record).unwrap();

        // Update status to Submitted
        store.update_status(id, TransactionStatus::Submitted, None).unwrap();

        // Verify event was published
        let event = receiver.recv().await.unwrap();
        match event {
            TransactionEvent::StatusUpdated {
                id: event_id,
                record: event_record,
            } => {
                assert_eq!(event_id, id);
                assert_eq!(event_record.status, TransactionStatus::Submitted);
                assert_eq!(event_record.id, id);
            }
        }

        // Update status to Confirmed
        store.update_status(id, TransactionStatus::Confirmed, None).unwrap();

        // Verify second event was published
        let event = receiver.recv().await.unwrap();
        match event {
            TransactionEvent::StatusUpdated {
                id: event_id,
                record: event_record,
            } => {
                assert_eq!(event_id, id);
                assert_eq!(event_record.status, TransactionStatus::Confirmed);
                assert!(event_record.confirmed_at.is_some());
            }
        }
    }

    #[tokio::test]
    async fn test_multiple_subscribers_receive_events() {
        let store = TransactionStore::new();

        // Create multiple subscribers
        let mut receiver1 = store.subscribe();
        let mut receiver2 = store.subscribe();

        // Insert a transaction
        let record = TransactionRecord {
            id: Uuid::new_v4(),
            raw_transaction: vec![0x01, 0x02, 0x03],
            transaction_hash: Hash::default(),
            status: TransactionStatus::Pending,
            submitted_at: Utc::now(),
            confirmed_at: None,
            error_message: None,
        };

        let id = record.id;
        store.insert(record).unwrap();

        // Update status
        store.update_status(id, TransactionStatus::Confirmed, None).unwrap();

        // Verify both receivers got the event
        let event1 = receiver1.recv().await.unwrap();
        let event2 = receiver2.recv().await.unwrap();

        match (event1, event2) {
            (
                TransactionEvent::StatusUpdated {
                    id: id1,
                    record: record1,
                },
                TransactionEvent::StatusUpdated {
                    id: id2,
                    record: record2,
                },
            ) => {
                assert_eq!(id1, id);
                assert_eq!(id2, id);
                assert_eq!(record1.status, TransactionStatus::Confirmed);
                assert_eq!(record2.status, TransactionStatus::Confirmed);
            }
        }
    }

    #[tokio::test]
    async fn test_subscribe_after_updates_only_receives_future_events() {
        let store = TransactionStore::new();

        // Insert and update a transaction before subscribing
        let record = TransactionRecord {
            id: Uuid::new_v4(),
            raw_transaction: vec![0x01, 0x02, 0x03],
            transaction_hash: Hash::default(),
            status: TransactionStatus::Pending,
            submitted_at: Utc::now(),
            confirmed_at: None,
            error_message: None,
        };

        let id = record.id;
        store.insert(record).unwrap();
        store.update_status(id, TransactionStatus::Submitted, None).unwrap();

        // Subscribe after the update
        let mut receiver = store.subscribe();

        // Update status again
        store.update_status(id, TransactionStatus::Confirmed, None).unwrap();

        // Should only receive the Confirmed event, not the Submitted one
        let event = receiver.recv().await.unwrap();
        match event {
            TransactionEvent::StatusUpdated {
                id: event_id,
                record: event_record,
            } => {
                assert_eq!(event_id, id);
                assert_eq!(event_record.status, TransactionStatus::Confirmed);
            }
        }
    }
}
