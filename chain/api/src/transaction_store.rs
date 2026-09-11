//! In-memory transaction store for tracking submitted transactions
//!
//! This module provides a thread-safe in-memory store for tracking raw transactions
//! submitted through the GraphQL API. Transactions are stored with their submission
//! status and can be queried by UUID.

use std::{
    collections::{HashMap, VecDeque},
    sync::Arc,
};

use async_broadcast::{InactiveReceiver, Receiver, Sender, TrySendError, broadcast};
use chrono::{DateTime, Utc};
use dashmap::{DashMap, mapref::entry::Entry};
use hopr_types::{crypto::types::Hash, primitive::traits::ToHex};
use thiserror::Error;
use tracing::{debug, error, warn};
use uuid::Uuid;

use crate::{
    metrics::{
        STATUS_CONFIRMED, STATUS_REVERTED, STATUS_SUBMISSION_FAILED, STATUS_TIMEOUT, STATUS_VALIDATION_FAILED,
        record_transaction_status,
    },
    safe_execution::decode_transaction_signer,
};

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
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct SafeExecutionResult {
    /// Whether the internal Safe transaction succeeded
    pub success: bool,
    /// Safe internal transaction hash (bytes32 from event).
    /// `None` for module-executed transactions (the standard HOPR path) since
    /// module events do not carry a txHash. For direct `execTransaction` calls,
    /// `None` only if the event data was malformed.
    #[serde(serialize_with = "serialize_optional_hash")]
    pub safe_tx_hash: Option<Hash>,
    /// Revert reason string (if execution failed and reason is decodable)
    pub revert_reason: Option<String>,
}

fn serialize_optional_hash<S: serde::Serializer>(hash: &Option<Hash>, s: S) -> Result<S::Ok, S::Error> {
    match hash {
        Some(h) => s.serialize_some(&h.to_hex()),
        None => s.serialize_none(),
    }
}

fn serialize_hash<S: serde::Serializer>(hash: &Hash, s: S) -> Result<S::Ok, S::Error> {
    s.serialize_str(&hash.to_hex())
}

/// Status of a submitted transaction
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub enum TransactionStatus {
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

impl TransactionStatus {
    /// The `blokli_transaction_status_total` metric label for this status, or `None` for the
    /// non-terminal `Submitted` state.
    fn metric_label(self) -> Option<&'static str> {
        match self {
            TransactionStatus::Submitted => None,
            TransactionStatus::Confirmed => Some(STATUS_CONFIRMED),
            TransactionStatus::Reverted => Some(STATUS_REVERTED),
            TransactionStatus::Timeout => Some(STATUS_TIMEOUT),
            TransactionStatus::ValidationFailed => Some(STATUS_VALIDATION_FAILED),
            TransactionStatus::SubmissionFailed => Some(STATUS_SUBMISSION_FAILED),
        }
    }
}

/// Event type for transaction status updates
///
/// Represents transaction status changes that should be broadcast to subscribers.
/// Uses delta fields to keep copied data minimal.
#[derive(Clone, Debug, serde::Serialize)]
pub enum TransactionEvent {
    /// Transaction status was updated
    ///
    /// Contains only the changed fields (delta) instead of the full record
    StatusUpdated {
        id: Uuid,
        status: TransactionStatus,
        error_message: Option<String>,
        confirmed_at: Option<DateTime<Utc>>,
    },
}

/// Record of a submitted transaction
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct TransactionRecord {
    /// Unique identifier for the transaction
    pub id: Uuid,
    /// Raw signed transaction data
    pub raw_transaction: Vec<u8>,
    /// Transaction hash from successful blockchain submission
    #[serde(serialize_with = "serialize_hash")]
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

/// Identity used for per-client submission fairness.
///
/// This is the recovered signer of the raw transaction. `None` groups all
/// transactions whose signer could not be recovered into a single bucket, so a
/// stream of undecodable envelopes cannot spread across unlimited buckets.
type SubmissionIdentity = Option<[u8; 20]>;

/// Thread-safe in-memory store for transaction records
#[derive(Clone)]
pub struct TransactionStore {
    transactions: Arc<DashMap<Uuid, TransactionRecord>>,
    /// Identity of every transaction currently in `Submitted` status.
    ///
    /// Maintained alongside `transactions` so admission control never has to
    /// scan the store or re-recover a signer.
    submitted_identities: Arc<DashMap<Uuid, SubmissionIdentity>>,
    /// Number of `Submitted` transactions per identity, for O(1) admission checks.
    submitted_per_identity: Arc<DashMap<SubmissionIdentity, usize>>,
    /// Event bus sender for broadcasting transaction status updates
    event_bus: Sender<TransactionEvent>,
    /// Inactive receiver kept alive to maintain channel state
    ///
    /// It prevents the channel from closing without retaining events when no
    /// transaction subscriptions are active.
    _inactive_event_bus_rx: Arc<InactiveReceiver<TransactionEvent>>,
}

impl std::fmt::Debug for TransactionStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TransactionStore")
            .field("transactions", &self.transactions)
            .field("submitted", &self.submitted_identities.len())
            .field("event_bus", &"Sender<TransactionEvent>")
            .field("_inactive_event_bus_rx", &"Arc<InactiveReceiver<TransactionEvent>>")
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
        // Keep an inactive receiver alive so the channel remains open without
        // retaining events when there are no real subscribers.
        let (mut event_bus, event_bus_rx) = broadcast(event_bus_capacity);
        let inactive_event_bus_rx = event_bus_rx.deactivate();

        // Set overflow behavior to allow new receivers to miss old messages if they can't keep up
        event_bus.set_overflow(true);

        Self {
            transactions: Arc::new(DashMap::new()),
            submitted_identities: Arc::new(DashMap::new()),
            submitted_per_identity: Arc::new(DashMap::new()),
            event_bus,
            _inactive_event_bus_rx: Arc::new(inactive_event_bus_rx),
        }
    }

    /// Insert a new transaction record into the store
    ///
    /// # Errors
    /// Returns `TransactionStoreError::AlreadyExists` if a transaction with the same ID already exists
    pub fn insert(&self, record: TransactionRecord) -> Result<(), TransactionStoreError> {
        match self.transactions.entry(record.id) {
            Entry::Vacant(entry) => {
                let submitted = record.status == TransactionStatus::Submitted;
                let id = record.id;
                let identity = submitted.then(|| transaction_identity(&record.raw_transaction));
                entry.insert(record);
                if let Some(identity) = identity {
                    self.track_submitted(id, identity);
                }
                Ok(())
            }
            Entry::Occupied(entry) => Err(TransactionStoreError::AlreadyExists(*entry.key())),
        }
    }

    /// Record a transaction as occupying receipt-monitoring capacity.
    fn track_submitted(&self, id: Uuid, identity: SubmissionIdentity) {
        if self.submitted_identities.insert(id, identity).is_none() {
            *self.submitted_per_identity.entry(identity).or_insert(0) += 1;
        }
    }

    /// Release the receipt-monitoring capacity held by a transaction.
    fn untrack_submitted(&self, id: Uuid) {
        if let Some((_, identity)) = self.submitted_identities.remove(&id) {
            if let Entry::Occupied(mut entry) = self.submitted_per_identity.entry(identity) {
                let remaining = entry.get().saturating_sub(1);
                if remaining == 0 {
                    entry.remove();
                } else {
                    *entry.get_mut() = remaining;
                }
            }
        }
    }

    /// Keep the submitted index in step with a status transition.
    ///
    /// The signer is only recovered when a transaction enters `Submitted`
    /// without already being indexed, so terminal transitions stay cheap.
    fn sync_submitted_status(&self, id: Uuid, status: TransactionStatus) {
        if status != TransactionStatus::Submitted {
            self.untrack_submitted(id);
            return;
        }
        if self.submitted_identities.contains_key(&id) {
            return;
        }
        if let Some(entry) = self.transactions.get(&id) {
            let identity = transaction_identity(&entry.value().raw_transaction);
            self.track_submitted(id, identity);
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
                let id = record.id;
                let status = record.status;
                entry.insert(record);
                self.sync_submitted_status(id, status);
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
        // Update the transaction and extract delta fields for event
        let (confirmed_at, error_msg) = self
            .transactions
            .get_mut(&id)
            .map(|mut entry| {
                let record = entry.value_mut();
                record.status = status;
                record.error_message = error_message.clone();

                // Set confirmed_at timestamp if status is Confirmed
                if status == TransactionStatus::Confirmed && record.confirmed_at.is_none() {
                    record.confirmed_at = Some(Utc::now());
                }

                // Extract only fields needed for event (no cloning raw_transaction)
                (record.confirmed_at, record.error_message.clone())
            })
            .ok_or(TransactionStoreError::NotFound(id))?;

        self.sync_submitted_status(id, status);

        if let Some(label) = status.metric_label() {
            record_transaction_status(label);
        }

        // Publish event to subscribers with delta fields only
        self.broadcast_status_update(TransactionEvent::StatusUpdated {
            id,
            status,
            error_message: error_msg,
            confirmed_at,
        });

        Ok(())
    }

    /// Atomically confirm a transaction and set its Safe execution result.
    ///
    /// Sets `status = Confirmed`, `confirmed_at = Some(Utc::now())`, and
    /// `safe_execution` in a single operation, preventing a window where
    /// subscribers see `Confirmed` with `safeExecution: null`.
    ///
    /// # Errors
    /// Returns `TransactionStoreError::NotFound` if the transaction doesn't exist
    pub fn confirm_with_safe_execution(
        &self,
        id: Uuid,
        safe_execution: Option<SafeExecutionResult>,
    ) -> Result<(), TransactionStoreError> {
        let confirmed_at = self
            .transactions
            .get_mut(&id)
            .map(|mut entry| {
                let record = entry.value_mut();
                record.status = TransactionStatus::Confirmed;
                if record.confirmed_at.is_none() {
                    record.confirmed_at = Some(Utc::now());
                }
                record.safe_execution = safe_execution;
                record.confirmed_at
            })
            .ok_or(TransactionStoreError::NotFound(id))?;

        self.untrack_submitted(id);

        record_transaction_status(STATUS_CONFIRMED);

        // Broadcast event so subscribers are notified of the confirmation
        self.broadcast_status_update(TransactionEvent::StatusUpdated {
            id,
            status: TransactionStatus::Confirmed,
            error_message: None,
            confirmed_at,
        });

        Ok(())
    }

    /// Add an optional revert reason after a Safe failure was already published.
    /// This deliberately does not emit a status transition: clients have already
    /// received the authoritative Safe failure outcome.
    pub fn update_safe_revert_reason(&self, id: Uuid, revert_reason: String) -> Result<(), TransactionStoreError> {
        self.transactions
            .get_mut(&id)
            .map(|mut entry| {
                if let Some(safe_execution) = entry.value_mut().safe_execution.as_mut() {
                    if !safe_execution.success {
                        safe_execution.revert_reason = Some(revert_reason);
                    }
                }
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

    /// Return submitted work in round-robin signer order. The signer is
    /// recovered from the signed envelope, so it is not supplied by a client
    /// header or IP.
    pub fn list_submitted_fair(&self) -> Vec<TransactionRecord> {
        let mut by_identity: HashMap<SubmissionIdentity, VecDeque<TransactionRecord>> = HashMap::new();
        for entry in self.submitted_identities.iter() {
            let Some(record) = self.transactions.get(entry.key()).map(|record| record.value().clone()) else {
                continue;
            };
            if record.status != TransactionStatus::Submitted {
                continue;
            }
            by_identity.entry(*entry.value()).or_default().push_back(record);
        }
        let mut queues: Vec<VecDeque<TransactionRecord>> = by_identity.into_values().collect();
        let mut fair = Vec::new();
        loop {
            let mut progressed = false;
            for queue in &mut queues {
                if let Some(record) = queue.pop_front() {
                    fair.push(record);
                    progressed = true;
                }
            }
            if !progressed {
                break;
            }
        }
        fair
    }

    /// Check capacity before a raw transaction is broadcast.
    ///
    /// The identity is the recovered transaction signer, so it is bound to the
    /// signature rather than chosen by the caller, and deliberately never a
    /// client IP address. Both counts are read from the submitted index, so the
    /// cost is independent of how many transactions the store holds.
    ///
    /// A limit of `0` means unbounded. Note that the limits are advisory rather
    /// than hard: this check and the subsequent insert are not a single atomic
    /// operation, so concurrent submissions can transiently exceed a limit by
    /// the number of requests in flight.
    pub fn can_admit_submission(&self, raw_transaction: &[u8], max_submitted: usize, max_per_identity: usize) -> bool {
        if max_submitted > 0 && self.submitted_identities.len() >= max_submitted {
            return false;
        }
        if max_per_identity == 0 {
            return true;
        }
        let identity = transaction_identity(raw_transaction);
        let submitted_for_identity = self
            .submitted_per_identity
            .get(&identity)
            .map(|count| *count.value())
            .unwrap_or(0);
        submitted_for_identity < max_per_identity
    }

    /// Number of transactions currently awaiting receipt monitoring.
    pub fn submitted_count(&self) -> usize {
        self.submitted_identities.len()
    }

    /// Get the total count of transactions in the store
    pub fn count(&self) -> usize {
        self.transactions.len()
    }

    /// Subscribe to transaction status update events
    ///
    /// Creates a new receiver that will receive all future transaction status updates.
    /// Each status update broadcasts a `TransactionEvent::StatusUpdated` delta.
    ///
    /// # Returns
    ///
    /// A receiver for transaction events
    pub fn subscribe(&self) -> Receiver<TransactionEvent> {
        // Get a fresh receiver from the sender to avoid inheriting backlog
        self.event_bus.new_receiver()
    }

    fn broadcast_status_update(&self, event: TransactionEvent) {
        let TransactionEvent::StatusUpdated { id, status, .. } = &event;
        let id = *id;
        let status = *status;

        match self.event_bus.try_broadcast(event) {
            Ok(Some(_)) => {
                warn!(
                    transaction_id = %id,
                    ?status,
                    "Transaction event bus overflowed and dropped its oldest event"
                );
            }
            Ok(None) => {}
            Err(TrySendError::Inactive(_)) => {
                debug!(
                    transaction_id = %id,
                    ?status,
                    "No active transaction subscribers; status remains available in the store"
                );
            }
            Err(error) => {
                error!(
                    transaction_id = %id,
                    ?status,
                    %error,
                    "Failed to broadcast transaction status update"
                );
            }
        }
    }
}

/// Identity used to account for submission capacity: the recovered signer of
/// the transaction. All envelopes whose signer cannot be recovered share the
/// `None` bucket.
fn transaction_identity(raw_transaction: &[u8]) -> SubmissionIdentity {
    decode_transaction_signer(raw_transaction)
}

impl Default for TransactionStore {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use std::thread;

    use hopr_bindings::exports::alloy::{
        consensus::{SignableTransaction, TxLegacy},
        eips::eip2718::Encodable2718,
        primitives::{Address as AlloyAddress, TxKind, U256},
        signers::{Signer, local::PrivateKeySigner},
    };

    use super::*;

    const TEST_UUID: Uuid = Uuid::from_bytes([
        0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0A, 0x0B, 0x0C, 0x0D, 0x0E, 0x0F, 0x10,
    ]);

    fn test_tx_hash() -> Hash {
        Hash::from([0xABu8; 32])
    }

    fn test_timestamp() -> DateTime<Utc> {
        DateTime::from_timestamp(1_700_000_000, 0).unwrap()
    }

    #[test]
    fn test_create_store_and_insert_transaction() {
        let store = TransactionStore::new();

        let record = TransactionRecord {
            id: TEST_UUID,
            raw_transaction: vec![0x01, 0x02, 0x03],
            transaction_hash: test_tx_hash(),
            status: TransactionStatus::Submitted,
            submitted_at: test_timestamp(),
            confirmed_at: None,
            error_message: None,
            safe_execution: None,
        };

        let id = record.id;
        assert!(store.insert(record).is_ok());
        assert_eq!(store.count(), 1);

        // Verify we can retrieve it
        let retrieved = store.get(id).unwrap();
        insta::assert_yaml_snapshot!(retrieved);
    }

    #[test]
    fn test_insert_duplicate_transaction_fails() {
        let store = TransactionStore::new();

        let id = TEST_UUID;
        let record = TransactionRecord {
            id,
            raw_transaction: vec![0x01, 0x02, 0x03],
            transaction_hash: test_tx_hash(),
            status: TransactionStatus::Submitted,
            submitted_at: test_timestamp(),
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
        let id = TEST_UUID;

        let result = store.get(id);
        assert!(matches!(result, Err(TransactionStoreError::NotFound(_))));
    }

    #[test]
    fn test_update_transaction_status() {
        let store = TransactionStore::new();

        let record = TransactionRecord {
            id: TEST_UUID,
            raw_transaction: vec![0x01, 0x02, 0x03],
            transaction_hash: test_tx_hash(),
            status: TransactionStatus::Submitted,
            submitted_at: test_timestamp(),
            confirmed_at: None,
            error_message: None,
            safe_execution: None,
        };

        let id = record.id;
        store.insert(record).unwrap();

        // Verify initial status
        let retrieved = store.get(id).unwrap();
        insta::assert_yaml_snapshot!(retrieved);

        // Update status to Confirmed
        store.update_status(id, TransactionStatus::Confirmed, None).unwrap();

        let retrieved = store.get(id).unwrap();
        insta::assert_yaml_snapshot!(retrieved, {
            ".confirmed_at" => "[timestamp]",
        });
    }

    #[test]
    fn test_update_nonexistent_transaction_fails() {
        let store = TransactionStore::new();
        let id = TEST_UUID;

        let result = store.update_status(id, TransactionStatus::Confirmed, None);
        assert!(matches!(result, Err(TransactionStoreError::NotFound(_))));
    }

    #[test]
    fn test_list_by_status() {
        let store = TransactionStore::new();

        // Insert transactions with different statuses
        for i in 0..5u128 {
            let status = if i % 2 == 0 {
                TransactionStatus::Submitted
            } else {
                TransactionStatus::Confirmed
            };

            let record = TransactionRecord {
                id: Uuid::from_u128(i + 1),
                raw_transaction: vec![i as u8],
                transaction_hash: test_tx_hash(),
                status,
                submitted_at: test_timestamp(),
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
            for i in 0..5u128 {
                let record = TransactionRecord {
                    id: Uuid::from_u128(i + 1),
                    raw_transaction: vec![i as u8],
                    transaction_hash: test_tx_hash(),
                    status: TransactionStatus::Submitted,
                    submitted_at: test_timestamp(),
                    confirmed_at: None,
                    error_message: None,
                    safe_execution: None,
                };
                store_clone1.insert(record).unwrap();
            }
        });

        // Thread 2: Insert 5 more transactions
        let handle2 = thread::spawn(move || {
            for i in 5..10u128 {
                let record = TransactionRecord {
                    id: Uuid::from_u128(i + 1),
                    raw_transaction: vec![i as u8],
                    transaction_hash: test_tx_hash(),
                    status: TransactionStatus::Submitted,
                    submitted_at: test_timestamp(),
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
            id: TEST_UUID,
            raw_transaction: vec![0x01, 0x02, 0x03],
            transaction_hash: test_tx_hash(),
            status: TransactionStatus::Submitted,
            submitted_at: test_timestamp(),
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
            transaction_hash: test_tx_hash(),
            status: TransactionStatus::Confirmed,
            submitted_at: test_timestamp(),
            confirmed_at: Some(test_timestamp()),
            error_message: None,
            safe_execution: None,
        };

        store.update(updated_record.clone()).unwrap();

        let retrieved = store.get(id).unwrap();
        insta::assert_yaml_snapshot!(retrieved);
    }

    #[tokio::test]
    async fn test_event_publishing_on_status_update() {
        let store = TransactionStore::new();

        // Subscribe before inserting transaction
        let mut receiver = store.subscribe();

        // Insert a transaction
        let record = TransactionRecord {
            id: TEST_UUID,
            raw_transaction: vec![0x01, 0x02, 0x03],
            transaction_hash: test_tx_hash(),
            status: TransactionStatus::Submitted,
            submitted_at: test_timestamp(),
            confirmed_at: None,
            error_message: None,
            safe_execution: None,
        };

        let id = record.id;
        store.insert(record).unwrap();

        // Update status to Submitted
        store.update_status(id, TransactionStatus::Submitted, None).unwrap();

        // Verify event was published
        let event = receiver.recv().await.unwrap();
        let TransactionEvent::StatusUpdated {
            id: event_id,
            status,
            error_message,
            confirmed_at,
        } = event;
        assert_eq!(event_id, id);
        assert_eq!(status, TransactionStatus::Submitted);
        assert!(error_message.is_none());
        assert!(confirmed_at.is_none());

        // Update status to Confirmed
        store.update_status(id, TransactionStatus::Confirmed, None).unwrap();

        // Verify second event was published
        let event = receiver.recv().await.unwrap();
        let TransactionEvent::StatusUpdated {
            id: event_id,
            status,
            error_message,
            confirmed_at,
        } = event;
        assert_eq!(event_id, id);
        assert_eq!(status, TransactionStatus::Confirmed);
        assert!(error_message.is_none());
        assert!(confirmed_at.is_some());
    }

    #[tokio::test]
    async fn test_multiple_subscribers_receive_events() {
        let store = TransactionStore::new();

        // Create multiple subscribers
        let mut receiver1 = store.subscribe();
        let mut receiver2 = store.subscribe();

        // Insert a transaction
        let record = TransactionRecord {
            id: TEST_UUID,
            raw_transaction: vec![0x01, 0x02, 0x03],
            transaction_hash: test_tx_hash(),
            status: TransactionStatus::Submitted,
            submitted_at: test_timestamp(),
            confirmed_at: None,
            error_message: None,
            safe_execution: None,
        };

        let id = record.id;
        store.insert(record).unwrap();

        // Update status
        store.update_status(id, TransactionStatus::Confirmed, None).unwrap();

        // Verify both receivers got the event
        let event1 = receiver1.recv().await.unwrap();
        let event2 = receiver2.recv().await.unwrap();

        let TransactionEvent::StatusUpdated {
            id: event_id, status, ..
        } = event1;
        assert_eq!(event_id, id);
        assert_eq!(status, TransactionStatus::Confirmed);

        let TransactionEvent::StatusUpdated {
            id: event_id, status, ..
        } = event2;
        assert_eq!(event_id, id);
        assert_eq!(status, TransactionStatus::Confirmed);
    }

    #[tokio::test]
    async fn test_subscribe_after_updates_only_receives_future_events() {
        let store = TransactionStore::new();

        // Insert and update a transaction before subscribing
        let record = TransactionRecord {
            id: TEST_UUID,
            raw_transaction: vec![0x01, 0x02, 0x03],
            transaction_hash: test_tx_hash(),
            status: TransactionStatus::Submitted,
            submitted_at: test_timestamp(),
            confirmed_at: None,
            error_message: None,
            safe_execution: None,
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
        let TransactionEvent::StatusUpdated {
            id: event_id, status, ..
        } = event;
        assert_eq!(event_id, id);
        assert_eq!(status, TransactionStatus::Confirmed);
    }

    #[test]
    fn test_confirm_with_safe_execution() {
        let store = TransactionStore::new();

        let record = TransactionRecord {
            id: TEST_UUID,
            raw_transaction: vec![0x01],
            transaction_hash: test_tx_hash(),
            status: TransactionStatus::Submitted,
            submitted_at: test_timestamp(),
            confirmed_at: None,
            error_message: None,
            safe_execution: None,
        };

        let id = record.id;
        store.insert(record).unwrap();

        // Atomically confirm with safe execution
        let safe_exec = SafeExecutionResult {
            success: false,
            safe_tx_hash: Some(Hash::from([0xAB; 32])),
            revert_reason: Some("revert reason".to_string()),
        };

        store.confirm_with_safe_execution(id, Some(safe_exec)).unwrap();

        let retrieved = store.get(id).unwrap();
        insta::assert_yaml_snapshot!(retrieved, {
            ".confirmed_at" => "[timestamp]",
        });
    }

    #[test]
    fn test_confirm_with_safe_execution_none() {
        let store = TransactionStore::new();

        let record = TransactionRecord {
            id: TEST_UUID,
            raw_transaction: vec![0x01],
            transaction_hash: test_tx_hash(),
            status: TransactionStatus::Submitted,
            submitted_at: test_timestamp(),
            confirmed_at: None,
            error_message: None,
            safe_execution: None,
        };

        let id = record.id;
        store.insert(record).unwrap();

        // Confirm without safe execution (non-Safe transaction)
        store.confirm_with_safe_execution(id, None).unwrap();

        let retrieved = store.get(id).unwrap();
        insta::assert_yaml_snapshot!(retrieved, {
            ".confirmed_at" => "[timestamp]",
        });
    }

    #[test]
    fn test_confirm_with_safe_execution_not_found() {
        let store = TransactionStore::new();
        let id = TEST_UUID;

        let result = store.confirm_with_safe_execution(id, None);
        assert!(matches!(result, Err(TransactionStoreError::NotFound(_))));
    }

    /// Build a raw signed legacy transaction from `signer` to `target`.
    async fn signed_raw_tx(signer: &PrivateKeySigner, target: [u8; 20], nonce: u64) -> Vec<u8> {
        let tx = TxLegacy {
            chain_id: Some(1),
            nonce,
            gas_price: 1_000_000_000,
            gas_limit: 21_000,
            to: TxKind::Call(AlloyAddress::from_slice(&target)),
            value: U256::ZERO,
            input: Default::default(),
        };
        let signature = signer.sign_hash(&tx.signature_hash()).await.expect("signing failed");
        let mut encoded = Vec::new();
        tx.into_signed(signature).encode_2718(&mut encoded);
        encoded
    }

    fn submitted_record(id: Uuid, raw_transaction: Vec<u8>) -> TransactionRecord {
        TransactionRecord {
            id,
            raw_transaction,
            transaction_hash: test_tx_hash(),
            status: TransactionStatus::Submitted,
            submitted_at: test_timestamp(),
            confirmed_at: None,
            error_message: None,
            safe_execution: None,
        }
    }

    #[tokio::test]
    async fn test_identity_is_the_signer_not_the_target() {
        let signer = PrivateKeySigner::random();

        // Same signer, two different contract targets.
        let first = signed_raw_tx(&signer, [0x11; 20], 0).await;
        let second = signed_raw_tx(&signer, [0x22; 20], 1).await;
        assert_eq!(transaction_identity(&first), transaction_identity(&second));

        // Different signers, same contract target.
        let other = signed_raw_tx(&PrivateKeySigner::random(), [0x11; 20], 0).await;
        assert_ne!(transaction_identity(&first), transaction_identity(&other));
    }

    #[tokio::test]
    async fn test_per_identity_limit_counts_signers_not_targets() {
        let store = TransactionStore::new();
        let signer = PrivateKeySigner::random();

        let first = signed_raw_tx(&signer, [0x11; 20], 0).await;
        store.insert(submitted_record(Uuid::new_v4(), first)).unwrap();

        // A second transaction from the same signer to a different contract
        // still counts against that signer's quota.
        let second = signed_raw_tx(&signer, [0x22; 20], 1).await;
        assert!(!store.can_admit_submission(&second, 0, 1));

        // A different signer is unaffected by the first signer's usage.
        let other = signed_raw_tx(&PrivateKeySigner::random(), [0x11; 20], 0).await;
        assert!(store.can_admit_submission(&other, 0, 1));
    }

    #[tokio::test]
    async fn test_zero_limits_are_unbounded() {
        let store = TransactionStore::new();
        let signer = PrivateKeySigner::random();

        for nonce in 0..4 {
            let raw = signed_raw_tx(&signer, [0x11; 20], nonce).await;
            store.insert(submitted_record(Uuid::new_v4(), raw)).unwrap();
        }

        let next = signed_raw_tx(&signer, [0x11; 20], 4).await;
        assert!(store.can_admit_submission(&next, 0, 0));
    }

    #[tokio::test]
    async fn test_terminal_status_releases_capacity() {
        let store = TransactionStore::new();
        let signer = PrivateKeySigner::random();
        let raw = signed_raw_tx(&signer, [0x11; 20], 0).await;
        let id = Uuid::new_v4();

        store.insert(submitted_record(id, raw.clone())).unwrap();
        assert_eq!(store.submitted_count(), 1);
        assert!(!store.can_admit_submission(&raw, 1, 1));

        store.update_status(id, TransactionStatus::Confirmed, None).unwrap();
        assert_eq!(store.submitted_count(), 0);
        assert!(store.can_admit_submission(&raw, 1, 1));

        // The per-identity bucket is dropped once it reaches zero.
        assert!(store.submitted_per_identity.is_empty());
    }

    #[tokio::test]
    async fn test_confirm_with_safe_execution_releases_capacity() {
        let store = TransactionStore::new();
        let raw = signed_raw_tx(&PrivateKeySigner::random(), [0x11; 20], 0).await;
        let id = Uuid::new_v4();

        store.insert(submitted_record(id, raw)).unwrap();
        store.confirm_with_safe_execution(id, None).unwrap();

        assert_eq!(store.submitted_count(), 0);
        assert!(store.submitted_per_identity.is_empty());
    }

    #[test]
    fn test_non_submitted_insert_holds_no_capacity() {
        let store = TransactionStore::new();
        let mut record = submitted_record(TEST_UUID, vec![0x01]);
        record.status = TransactionStatus::Confirmed;

        store.insert(record).unwrap();
        assert_eq!(store.submitted_count(), 0);
    }

    #[test]
    fn test_undecodable_transactions_share_one_bucket() {
        let store = TransactionStore::new();

        store.insert(submitted_record(Uuid::new_v4(), vec![0xff])).unwrap();

        // A second undecodable envelope lands in the same `None` bucket rather
        // than creating an unbounded number of identities.
        assert!(!store.can_admit_submission(&[0xfe], 0, 1));
        assert_eq!(store.submitted_per_identity.len(), 1);
    }

    #[tokio::test]
    async fn test_fair_listing_round_robins_signers() {
        let store = TransactionStore::new();
        let busy = PrivateKeySigner::random();
        let quiet = PrivateKeySigner::random();

        for nonce in 0..3 {
            let raw = signed_raw_tx(&busy, [0x11; 20], nonce).await;
            store.insert(submitted_record(Uuid::new_v4(), raw)).unwrap();
        }
        let quiet_raw = signed_raw_tx(&quiet, [0x11; 20], 0).await;
        let quiet_id = Uuid::new_v4();
        store.insert(submitted_record(quiet_id, quiet_raw)).unwrap();

        let fair = store.list_submitted_fair();
        assert_eq!(fair.len(), 4);

        // The single transaction of the quiet signer is served in the first
        // round, ahead of the busy signer's backlog.
        let quiet_position = fair.iter().position(|record| record.id == quiet_id).unwrap();
        assert!(quiet_position < 2);
    }
}
