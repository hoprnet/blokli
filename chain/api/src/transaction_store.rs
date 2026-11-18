//! In-memory transaction store for tracking submitted transactions
//!
//! This module provides a thread-safe in-memory store for tracking raw transactions
//! submitted through the GraphQL API. Transactions are stored with their submission
//! status and can be queried by UUID.

use std::{
    collections::{HashMap, HashSet, VecDeque},
    sync::{Arc, Mutex, MutexGuard, PoisonError},
    time::Duration,
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
        record_transaction_status, set_transaction_store_size,
    },
    safe_execution::{decode_transaction_signer, decode_transaction_signer_and_nonce},
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

/// Why a submitted transaction was not mined before it timed out.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub enum UnminedReason {
    /// The node holds the transaction, but it was signed below the current recommended fee.
    Underpriced,
    /// The node holds the transaction, signed at or above the recommended fee, and it waits
    /// behind earlier pending transactions of the signer.
    Queued,
    /// An earlier nonce of the signer is neither mined nor pending.
    NonceGap,
    /// The node no longer knows the transaction and its nonce is still unused.
    Dropped,
    /// The node no longer knows the transaction and its nonce was consumed by another one.
    Replaced,
    /// The reason could not be established.
    Unknown,
}

/// EIP-1559 fee pair in wei.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub struct FeePair {
    pub max_fee_per_gas: u128,
    pub max_priority_fee_per_gas: u128,
}

/// Structured explanation of a transaction that timed out unmined.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub struct TransactionDiagnosis {
    /// Why the transaction was not mined
    pub reason: UnminedReason,
    /// Fees the transaction was signed with
    pub signed_fees: Option<FeePair>,
    /// Fees a transaction should currently be signed with, when the fee estimate was available
    pub recommended_fees: Option<FeePair>,
    /// Lowest fees at which a replacement would be accepted, when the fee estimate was available
    pub min_replacement_fees: Option<FeePair>,
    /// How long to wait before checking or retrying
    pub retry_after: Duration,
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
    /// Whether this status is a terminal outcome that no longer needs receipt monitoring.
    pub fn is_terminal(self) -> bool {
        !matches!(self, TransactionStatus::Submitted)
    }

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
    /// Why the transaction was not mined (populated when it times out)
    pub diagnosis: Option<TransactionDiagnosis>,
    /// Number of times a resubmission resumed tracking of this record
    #[serde(skip)]
    pub resume_generation: u64,
    /// When tracking was last resumed; restarts the timeout clock without touching `submitted_at`
    #[serde(skip)]
    pub last_resumed_at: Option<DateTime<Utc>>,
}

impl TransactionRecord {
    /// Instant the timeout clock counts from: the last resumption, else the submission.
    pub fn timeout_clock_start(&self) -> DateTime<Utc> {
        self.last_resumed_at.unwrap_or(self.submitted_at)
    }
}

/// Identity used for per-client submission fairness.
///
/// This is the recovered signer of the raw transaction. `None` groups all
/// transactions whose signer could not be recovered into a single bucket, so a
/// stream of undecodable envelopes cannot spread across unlimited buckets.
type SubmissionIdentity = Option<[u8; 20]>;

/// Signer and nonce of a transaction: the slot a fee-bumped replacement competes for.
type NonceKey = ([u8; 20], u64);

/// The `Submitted` transactions that share one signer and nonce, and the replacements made.
///
/// Only the node can mine one of them; the others are replacements of it. Entries exist
/// exactly while at least one `Submitted` transaction occupies the slot, so a client cannot
/// grow the index beyond the monitoring capacity.
#[derive(Debug, Default)]
struct NonceSlot {
    /// `Submitted` transactions in the slot, oldest first. The last one is the newest, the one
    /// a replacement targets.
    ids: Vec<Uuid>,
    /// Replacements committed in this slot. It lives in the slot and not on a record, so
    /// evicting a terminal record never resets it.
    replacements: usize,
    /// A replacement holds a [`ReplacementTicket`] and is being broadcast.
    replacing: bool,
}

/// Why [`TransactionStore::begin_replacement`] refused a replacement.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReplacementRefused {
    /// The transaction the replacement was priced against is no longer the newest one in its
    /// slot: it concluded, or another replacement was committed.
    TargetMoved,
    /// Another replacement of this slot is being broadcast.
    InProgress,
    /// The slot already holds the maximum number of replacements.
    LimitReached { max: usize },
}

/// Exclusive right to broadcast one replacement in a (signer, nonce) slot.
///
/// Held from before the broadcast until [`TransactionStore::commit_replacement`] consumes it.
/// Dropping it unused (a rejected or failed broadcast) frees the slot for another attempt.
#[must_use = "dropping a ticket releases the replacement slot"]
pub struct ReplacementTicket {
    nonce_slots: Arc<DashMap<NonceKey, NonceSlot>>,
    key: NonceKey,
    /// Newest transaction of the slot when the ticket was granted: the one being replaced.
    target: Uuid,
    held: bool,
}

impl ReplacementTicket {
    /// The transaction this ticket replaces.
    pub fn target(&self) -> Uuid {
        self.target
    }
}

impl Drop for ReplacementTicket {
    fn drop(&mut self) {
        if self.held {
            if let Some(mut slot) = self.nonce_slots.get_mut(&self.key) {
                slot.replacing = false;
            }
        }
    }
}

/// The newest `Submitted` transaction of a slot, as a replacement sees it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReplacementTarget {
    pub id: Uuid,
    pub transaction_hash: Hash,
    pub raw_transaction: Vec<u8>,
}

/// Receipt-monitoring capacity in use: tracked `Submitted` records plus
/// reservations whose transaction is still being broadcast.
///
/// Both counts change under one lock, so an admission check and the
/// reservation it grants are a single step.
#[derive(Debug, Default)]
struct SubmissionCounts {
    total: usize,
    per_identity: HashMap<SubmissionIdentity, usize>,
}

impl SubmissionCounts {
    fn acquire(&mut self, identity: SubmissionIdentity) {
        self.total += 1;
        *self.per_identity.entry(identity).or_insert(0) += 1;
    }

    fn release(&mut self, identity: SubmissionIdentity) {
        self.total = self.total.saturating_sub(1);
        if let Some(count) = self.per_identity.get_mut(&identity) {
            *count = count.saturating_sub(1);
            if *count == 0 {
                self.per_identity.remove(&identity);
            }
        }
    }
}

/// Lock the capacity counts.
///
/// They are plain integers updated in one step, so a panic while the lock was
/// held cannot leave them half-written and poisoning is safe to ignore.
fn lock_counts(counts: &Mutex<SubmissionCounts>) -> MutexGuard<'_, SubmissionCounts> {
    counts.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Capacity held for an async submission between admission and insertion.
///
/// Dropping it gives the capacity back, so a failed broadcast frees its slot.
/// [`TransactionStore::insert_reserved`] hands the capacity over to the stored
/// record instead.
#[must_use = "dropping a reservation immediately releases the capacity it holds"]
pub struct SubmissionReservation {
    counts: Arc<Mutex<SubmissionCounts>>,
    identity: SubmissionIdentity,
    held: bool,
}

impl Drop for SubmissionReservation {
    fn drop(&mut self) {
        if self.held {
            lock_counts(&self.counts).release(self.identity);
        }
    }
}

/// Submission limit that refused a reservation, with its configured maximum.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubmissionLimit {
    /// `max_submitted_transactions` across all signers.
    Global(usize),
    /// `max_submitted_transactions_per_identity` for the transaction signer.
    PerIdentity(usize),
}

/// Why [`TransactionStore::try_reserve_submission`] refused a reservation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SubmissionCapacityExhausted {
    /// The limit that was reached.
    pub limit: SubmissionLimit,
    /// Recovered signer of the refused transaction, `None` if it could not be recovered.
    pub identity: Option<[u8; 20]>,
    /// Capacity in use across all signers.
    pub submitted_total: usize,
    /// Capacity in use by the refused transaction's signer.
    pub submitted_for_identity: usize,
}

/// Outcome of [`TransactionStore::insert_or_resume`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrackOutcome {
    /// No tracked transaction with this hash was resumable, so the record was inserted.
    Inserted,
    /// A tracked transaction with this hash was resumed instead; the record was not inserted.
    Resumed {
        /// Identity of the transaction that already tracks this hash.
        existing: Uuid,
        /// The diagnosis it carried before resuming cleared it.
        diagnosis: Option<TransactionDiagnosis>,
    },
}

/// Thread-safe in-memory store for transaction records
///
/// Lock order, outermost first: `hash_index` entry, `transactions` record, `submitted_nonces` and
/// `nonce_slots` (never held together), `submitted_identities`, the capacity counts. Code holding
/// an inner lock never takes an outer one. In particular, no code reads or writes `transactions`
/// while holding a `nonce_slots` guard, and no code holds one `transactions` guard while
/// inserting another record: two ids may share a shard, which would deadlock.
#[derive(Clone)]
pub struct TransactionStore {
    transactions: Arc<DashMap<Uuid, TransactionRecord>>,
    /// Latest record id per transaction hash.
    ///
    /// Lets a submission find the record that already tracks its hash without scanning the
    /// store, and serialises concurrent submissions of the same bytes: the entry stays locked
    /// from lookup to insertion. An entry can outlive its record (it is then ignored and
    /// replaced), so the record it points to is always re-checked.
    hash_index: Arc<DashMap<Hash, Uuid>>,
    /// Time at which a transaction entered a terminal status.
    ///
    /// This metadata is kept separately to avoid exposing an implementation-only timestamp
    /// in the public transaction record and GraphQL schema.
    terminal_timestamps: Arc<DashMap<Uuid, DateTime<Utc>>>,
    /// Identity of every transaction currently in `Submitted` status.
    ///
    /// Maintained alongside `transactions` so admission control never has to
    /// scan the store or re-recover a signer.
    submitted_identities: Arc<DashMap<Uuid, SubmissionIdentity>>,
    /// Signer and nonce of every `Submitted` transaction whose signer could be recovered.
    ///
    /// Lets a transaction leave its `nonce_slots` entry without re-reading the record, which
    /// the caller may still hold. Lock order: after `transactions`, before `submitted_identities`.
    submitted_nonces: Arc<DashMap<Uuid, NonceKey>>,
    /// `Submitted` transactions per (signer, nonce), with the replacement count of that nonce.
    ///
    /// Lock order: after `transactions`, before `submitted_identities`. A guard on one entry is
    /// never held across any other lock, so slot operations are single-step.
    nonce_slots: Arc<DashMap<NonceKey, NonceSlot>>,
    /// Capacity in use, globally and per identity, for O(1) admission checks.
    submission_counts: Arc<Mutex<SubmissionCounts>>,
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

        let store = Self {
            transactions: Arc::new(DashMap::new()),
            hash_index: Arc::new(DashMap::new()),
            terminal_timestamps: Arc::new(DashMap::new()),
            submitted_identities: Arc::new(DashMap::new()),
            submitted_nonces: Arc::new(DashMap::new()),
            nonce_slots: Arc::new(DashMap::new()),
            submission_counts: Arc::new(Mutex::new(SubmissionCounts::default())),
            event_bus,
            _inactive_event_bus_rx: Arc::new(inactive_event_bus_rx),
        };
        store.update_size_metric();
        store
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
                let tracked = submitted.then(|| signer_and_nonce(&record.raw_transaction));
                let hash = record.transaction_hash;
                entry.insert(record);
                if submitted {
                    self.clear_terminal_timestamp(id);
                } else {
                    self.mark_terminal(id);
                }
                if let Some((identity, nonce_key)) = tracked {
                    self.track_submitted(id, identity, nonce_key);
                }
                self.index_hash(hash, id);
                self.update_size_metric();
                Ok(())
            }
            Entry::Occupied(entry) => Err(TransactionStoreError::AlreadyExists(*entry.key())),
        }
    }

    /// Insert an async submission, handing its reserved capacity over to the record.
    ///
    /// `reservation` must come from [`Self::try_reserve_submission`] on this store for the
    /// same raw transaction. If the record is not `Submitted` or cannot be inserted, the
    /// reservation is dropped and its capacity released.
    ///
    /// # Errors
    /// Returns `TransactionStoreError::AlreadyExists` if a transaction with the same ID already exists
    pub fn insert_reserved(
        &self,
        record: TransactionRecord,
        reservation: SubmissionReservation,
    ) -> Result<(), TransactionStoreError> {
        let (hash, id) = (record.transaction_hash, record.id);
        self.insert_reserved_unindexed(record, reservation)?;
        self.index_hash(hash, id);
        Ok(())
    }

    /// Insert an async submission, or resume the tracked transaction that already carries its
    /// hash, as one step per hash.
    ///
    /// The hash entry is held from the lookup to the insertion, so concurrent submissions of
    /// the same bytes cannot both insert: the second finds the first and resumes it. A tracked
    /// transaction counts as carrying the hash while the node can still hold it (see
    /// [`Self::resume_tracking`]); a dropped, replaced or concluded one is not resumed and the
    /// record is inserted next to it. When a transaction is resumed the reservation is dropped.
    ///
    /// `verified_known` is set when the node reported the transaction as already known and
    /// returned it for this hash. That is current evidence it holds the transaction, so a
    /// transaction that timed out with an unknown cause is resumed too, where a fresh
    /// acceptance of the same hash leaves it alone and inserts the record.
    ///
    /// # Errors
    /// Returns `TransactionStoreError::AlreadyExists` if a transaction with the same ID already exists
    pub fn insert_or_resume(
        &self,
        record: TransactionRecord,
        reservation: SubmissionReservation,
        verified_known: bool,
    ) -> Result<TrackOutcome, TransactionStoreError> {
        let id = record.id;
        match self.hash_index.entry(record.transaction_hash) {
            Entry::Occupied(mut index) => {
                let existing = *index.get();
                if let Some(diagnosis) = self.resume_held(existing, |held| {
                    held.transaction_hash == record.transaction_hash && is_held_by_node(held, verified_known)
                }) {
                    return Ok(TrackOutcome::Resumed { existing, diagnosis });
                }
                self.insert_reserved_unindexed(record, reservation)?;
                index.insert(id);
            }
            Entry::Vacant(index) => {
                self.insert_reserved_unindexed(record, reservation)?;
                index.insert(id);
            }
        }
        Ok(TrackOutcome::Inserted)
    }

    fn insert_reserved_unindexed(
        &self,
        record: TransactionRecord,
        mut reservation: SubmissionReservation,
    ) -> Result<(), TransactionStoreError> {
        match self.transactions.entry(record.id) {
            Entry::Vacant(entry) => {
                let id = record.id;
                let submitted = record.status == TransactionStatus::Submitted;
                let nonce_key = submitted.then(|| signer_and_nonce(&record.raw_transaction).1).flatten();
                entry.insert(record);
                if submitted {
                    self.clear_terminal_timestamp(id);
                } else {
                    self.mark_terminal(id);
                }
                // The reservation already counted this record, so the index entry takes over
                // that count instead of adding a second one.
                if submitted {
                    self.track_nonce(id, nonce_key);
                    if self.submitted_identities.insert(id, reservation.identity).is_none() {
                        reservation.held = false;
                    }
                }
                self.update_size_metric();
                Ok(())
            }
            Entry::Occupied(entry) => Err(TransactionStoreError::AlreadyExists(*entry.key())),
        }
    }

    fn counts(&self) -> MutexGuard<'_, SubmissionCounts> {
        lock_counts(&self.submission_counts)
    }

    /// Record a transaction as occupying receipt-monitoring capacity.
    fn track_submitted(&self, id: Uuid, identity: SubmissionIdentity, nonce_key: Option<NonceKey>) {
        self.track_nonce(id, nonce_key);
        if self.submitted_identities.insert(id, identity).is_none() {
            self.counts().acquire(identity);
        }
    }

    /// Release the receipt-monitoring capacity held by a transaction.
    fn untrack_submitted(&self, id: Uuid) {
        self.untrack_nonce(id);
        if let Some((_, identity)) = self.submitted_identities.remove(&id) {
            self.counts().release(identity);
        }
    }

    /// Place a `Submitted` transaction in its (signer, nonce) slot, moving it if its key changed.
    ///
    /// `None` (signer not recoverable) leaves the transaction out of every slot.
    fn track_nonce(&self, id: Uuid, nonce_key: Option<NonceKey>) {
        let Some(key) = nonce_key else {
            self.untrack_nonce(id);
            return;
        };
        match self.submitted_nonces.insert(id, key) {
            Some(previous) if previous == key => return,
            Some(previous) => self.leave_nonce_slot(id, previous),
            None => {}
        }
        let mut slot = self.nonce_slots.entry(key).or_default();
        if !slot.ids.contains(&id) {
            slot.ids.push(id);
        }
    }

    /// Take a transaction out of its (signer, nonce) slot, dropping the slot once it is empty.
    fn untrack_nonce(&self, id: Uuid) {
        if let Some((_, key)) = self.submitted_nonces.remove(&id) {
            self.leave_nonce_slot(id, key);
        }
    }

    fn leave_nonce_slot(&self, id: Uuid, key: NonceKey) {
        if let Entry::Occupied(mut slot) = self.nonce_slots.entry(key) {
            slot.get_mut().ids.retain(|member| *member != id);
            if slot.get().ids.is_empty() {
                slot.remove();
            }
        }
    }

    /// Record when a transaction first reached a terminal status.
    fn mark_terminal(&self, id: Uuid) {
        // Retain the record guard while inserting retention metadata. This keeps
        // concurrent eviction from removing the record and timestamp between the
        // two operations, which would otherwise leave an orphan timestamp.
        if let Some(record) = self.transactions.get(&id) {
            if record.status.is_terminal() {
                self.terminal_timestamps.entry(id).or_insert_with(Utc::now);
            }
        }
    }

    /// Remove terminal-retention metadata when a record becomes pending again.
    fn clear_terminal_timestamp(&self, id: Uuid) {
        self.terminal_timestamps.remove(&id);
    }

    fn update_size_metric(&self) {
        set_transaction_store_size(self.transactions.len());
    }

    /// Reconcile the submitted-transaction indexes with a record's new state.
    ///
    /// `identity` is supplied by the caller rather than read back from `transactions`: a
    /// caller may still hold that entry's guard, and re-reading it from here would deadlock
    /// on the same shard. It is only meaningful when `status` is `Submitted`.
    fn sync_submitted_status(
        &self,
        id: Uuid,
        status: TransactionStatus,
        identity: SubmissionIdentity,
        nonce_key: Option<NonceKey>,
    ) {
        if status != TransactionStatus::Submitted {
            self.untrack_submitted(id);
            return;
        }
        self.track_nonce(id, nonce_key);

        // Bind before matching so the read guard is released before the branches below
        // mutate the same map.
        let cached = self.submitted_identities.get(&id).map(|entry| *entry.value());
        match cached {
            // Already counted against this identity.
            Some(existing) if existing == identity => {}
            // A submitted record was replaced by one with a different signer. Without moving
            // the count the old signer would stay charged and the new one would go free.
            Some(_) => {
                self.untrack_submitted(id);
                self.track_submitted(id, identity, nonce_key);
            }
            None => self.track_submitted(id, identity, nonce_key),
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

    /// Read a transaction's status without cloning its raw signed transaction bytes.
    ///
    /// This is intended for subscription reconciliation, where status is usually unchanged.
    pub fn status(&self, id: Uuid) -> Result<TransactionStatus, TransactionStoreError> {
        self.transactions
            .get(&id)
            .map(|entry| entry.value().status)
            .ok_or(TransactionStoreError::NotFound(id))
    }

    /// Get a transaction record suitable for GraphQL output without copying the raw transaction.
    pub fn get_public(&self, id: Uuid) -> Result<TransactionRecord, TransactionStoreError> {
        self.transactions
            .get(&id)
            .map(|entry| {
                let record = entry.value();
                TransactionRecord {
                    id: record.id,
                    raw_transaction: Vec::new(),
                    transaction_hash: record.transaction_hash,
                    status: record.status,
                    submitted_at: record.submitted_at,
                    confirmed_at: record.confirmed_at,
                    error_message: record.error_message.clone(),
                    safe_execution: record.safe_execution.clone(),
                    diagnosis: record.diagnosis,
                    resume_generation: record.resume_generation,
                    last_resumed_at: record.last_resumed_at,
                }
            })
            .ok_or(TransactionStoreError::NotFound(id))
    }

    /// Point the hash index at `id`, unless it already points at a record the node can hold.
    fn index_hash(&self, hash: Hash, id: Uuid) {
        match self.hash_index.entry(hash) {
            Entry::Vacant(index) => {
                index.insert(id);
            }
            Entry::Occupied(mut index) => {
                let current = *index.get();
                if current != id
                    && !self
                        .transactions
                        .get(&current)
                        .is_some_and(|r| is_held_by_node(r.value(), false))
                {
                    index.insert(id);
                }
            }
        }
    }

    /// Id of the record with this hash that is still `Submitted`, i.e. occupies a monitoring slot.
    fn in_flight_by_hash(&self, transaction_hash: Hash) -> Option<Uuid> {
        let id = *self.hash_index.get(&transaction_hash)?;
        self.transactions
            .get(&id)
            .is_some_and(|record| {
                record.transaction_hash == transaction_hash && record.status == TransactionStatus::Submitted
            })
            .then_some(id)
    }

    /// Whether a record with this hash is `Submitted` and so already occupies a monitoring slot.
    ///
    /// A resubmission of it needs no new capacity; one of a timed-out record does.
    pub fn has_in_flight_hash(&self, transaction_hash: Hash) -> bool {
        self.in_flight_by_hash(transaction_hash).is_some()
    }

    /// Resume the `Submitted` record with this hash, returning its id.
    ///
    /// Takes no capacity, because the record already holds its slot. Returns `None` when no
    /// record with this hash is `Submitted` any more, e.g. it concluded in the meantime.
    pub fn resume_in_flight_hash(&self, transaction_hash: Hash) -> Option<Uuid> {
        let id = self.in_flight_by_hash(transaction_hash)?;
        self.resume_held(id, |record| {
            record.transaction_hash == transaction_hash && record.status == TransactionStatus::Submitted
        })
        .map(|_| id)
    }

    /// Resume tracking a transaction the node still holds after an identical resubmission.
    ///
    /// A `Timeout` record moves back to `Submitted` with its diagnosis cleared, bumps the
    /// resume generation and restarts the timeout clock (`submitted_at` keeps the original
    /// submission time), so the monitor polls it again. A record that is already `Submitted`
    /// keeps its generation and its timeout clock, so retrying an in-flight transaction cannot
    /// postpone its diagnosis. Records in any other status are untouched. The check window is
    /// extended only while the node still holds the transaction: each timeout re-diagnoses it,
    /// and a dropped or replaced transaction is no longer resumable.
    ///
    /// Returns the diagnosis the record carried before it was cleared, so the caller can still
    /// report it for the resubmission that resumed tracking; `None` when nothing was resumed.
    ///
    /// # Errors
    /// Returns `TransactionStoreError::NotFound` if the transaction doesn't exist
    pub fn resume_tracking(&self, id: Uuid) -> Result<Option<TransactionDiagnosis>, TransactionStoreError> {
        self.resume_held(id, |_| true)
            .ok_or(TransactionStoreError::NotFound(id))
    }

    /// Resume the record `id` if it exists and `held` accepts it, as [`Self::resume_tracking`] does.
    ///
    /// Returns `None` when the record is missing or not accepted, else the diagnosis it carried.
    fn resume_held(&self, id: Uuid, held: impl Fn(&TransactionRecord) -> bool) -> Option<Option<TransactionDiagnosis>> {
        // The submitted index is updated while the record guard is held, so a concurrent
        // status change cannot interleave its own index update with this one.
        let diagnosis = {
            let mut entry = self.transactions.get_mut(&id)?;
            let record = entry.value_mut();
            if !held(record) {
                return None;
            }
            if record.status != TransactionStatus::Timeout {
                return Some(None);
            }
            record.resume_generation += 1;
            record.last_resumed_at = Some(Utc::now());
            record.status = TransactionStatus::Submitted;
            record.error_message = None;
            let previous = record.diagnosis.take();
            let (identity, nonce_key) = signer_and_nonce(&record.raw_transaction);
            self.sync_submitted_status(id, TransactionStatus::Submitted, identity, nonce_key);
            previous
        };
        self.clear_terminal_timestamp(id);
        self.broadcast_status_update(TransactionEvent::StatusUpdated {
            id,
            status: TransactionStatus::Submitted,
            error_message: None,
            confirmed_at: None,
        });
        Some(diagnosis)
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
                // The replacement may carry a different raw transaction, so the identity is
                // taken from it rather than from whatever was indexed before.
                let (identity, nonce_key) = if status == TransactionStatus::Submitted {
                    signer_and_nonce(&record.raw_transaction)
                } else {
                    (None, None)
                };
                let hash = record.transaction_hash;
                let previous_hash = entry.get().transaction_hash;
                entry.insert(record);
                // Lock order is `transactions` then `submitted_identities`, with the index
                // updated under the record guard.
                self.sync_submitted_status(id, status, identity, nonce_key);
                drop(entry);
                if previous_hash != hash {
                    self.hash_index.remove_if(&previous_hash, |_, indexed| *indexed == id);
                }
                self.index_hash(hash, id);
                if status.is_terminal() {
                    self.mark_terminal(id);
                } else {
                    self.clear_terminal_timestamp(id);
                }
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
        self.update_status_with_diagnosis(id, status, error_message, None)
    }

    /// Update the status of a transaction together with its unmined diagnosis.
    ///
    /// # Errors
    /// Returns `TransactionStoreError::NotFound` if the transaction doesn't exist
    pub fn update_status_with_diagnosis(
        &self,
        id: Uuid,
        status: TransactionStatus,
        error_message: Option<String>,
        diagnosis: Option<TransactionDiagnosis>,
    ) -> Result<(), TransactionStoreError> {
        self.apply_status(id, status, error_message, diagnosis, None)
            .map(|_| ())
    }

    /// Mark a transaction as timed out, but only if it is still `Submitted` with the
    /// `resume_generation` the caller read before diagnosing it.
    ///
    /// A resubmission bumps the generation, so a changed one means the record was resumed
    /// while the diagnosis ran and the write is skipped. Returns whether it applied.
    ///
    /// # Errors
    /// Returns `TransactionStoreError::NotFound` if the transaction doesn't exist
    pub fn mark_timeout_if_unchanged(
        &self,
        id: Uuid,
        expected_generation: u64,
        error_message: Option<String>,
        diagnosis: Option<TransactionDiagnosis>,
    ) -> Result<bool, TransactionStoreError> {
        self.apply_status(
            id,
            TransactionStatus::Timeout,
            error_message,
            diagnosis,
            Some(expected_generation),
        )
    }

    /// Apply a status change, optionally only while the record is `Submitted` at
    /// `expected_generation`. Returns whether the change was applied.
    fn apply_status(
        &self,
        id: Uuid,
        status: TransactionStatus,
        error_message: Option<String>,
        diagnosis: Option<TransactionDiagnosis>,
        expected_generation: Option<u64>,
    ) -> Result<bool, TransactionStoreError> {
        // Update the transaction and extract delta fields for event
        let Some((confirmed_at, error_msg)) = self
            .transactions
            .get_mut(&id)
            .map(|mut entry| {
                let record = entry.value_mut();
                if let Some(generation) = expected_generation {
                    if record.status != TransactionStatus::Submitted || record.resume_generation != generation {
                        return None;
                    }
                }
                record.status = status;
                record.error_message = error_message.clone();
                record.diagnosis = diagnosis;

                // Set confirmed_at timestamp if status is Confirmed
                if status == TransactionStatus::Confirmed && record.confirmed_at.is_none() {
                    record.confirmed_at = Some(Utc::now());
                }

                // Extract only fields needed for event (no cloning raw_transaction). The
                // signer is recovered only when the record is entering `Submitted`, so
                // terminal transitions stay free.
                let (identity, nonce_key) = if status == TransactionStatus::Submitted {
                    signer_and_nonce(&record.raw_transaction)
                } else {
                    (None, None)
                };
                // Reconcile the submitted index while the record guard is held, so a concurrent
                // resume cannot interleave its own index update with this one.
                self.sync_submitted_status(id, status, identity, nonce_key);
                Some((record.confirmed_at, record.error_message.clone()))
            })
            .ok_or(TransactionStoreError::NotFound(id))?
        else {
            return Ok(false);
        };

        if status.is_terminal() {
            self.mark_terminal(id);
        } else {
            self.clear_terminal_timestamp(id);
        }

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

        Ok(true)
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
                record.diagnosis = None;
                self.untrack_submitted(id);
                record.confirmed_at
            })
            .ok_or(TransactionStoreError::NotFound(id))?;

        self.mark_terminal(id);

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
        // Snapshot the index first so no index guard is held while `transactions` is read: the
        // lock order everywhere is `transactions` then `submitted_identities`.
        let indexed: Vec<(Uuid, SubmissionIdentity)> = self
            .submitted_identities
            .iter()
            .map(|entry| (*entry.key(), *entry.value()))
            .collect();
        for (id, identity) in indexed {
            let Some(record) = self.transactions.get(&id).map(|record| record.value().clone()) else {
                continue;
            };
            if record.status != TransactionStatus::Submitted {
                continue;
            }
            by_identity.entry(identity).or_default().push_back(record);
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

    /// Reserve capacity before a raw transaction is broadcast.
    ///
    /// The identity is the recovered transaction signer, so it is bound to the
    /// signature rather than chosen by the caller, and deliberately never a
    /// client IP address. Counts cover both stored `Submitted` records and
    /// reservations still in flight, so concurrent submissions cannot overshoot
    /// a limit while they wait on the RPC. The cost is independent of how many
    /// transactions the store holds.
    ///
    /// A limit of `0` means unbounded. Returns [`SubmissionCapacityExhausted`]
    /// describing the limit that was reached; otherwise the capacity is held
    /// until the returned reservation is dropped or passed to
    /// [`Self::insert_reserved`].
    pub fn try_reserve_submission(
        &self,
        raw_transaction: &[u8],
        max_submitted: usize,
        max_per_identity: usize,
    ) -> Result<SubmissionReservation, SubmissionCapacityExhausted> {
        // Recover the signer before locking so the critical section stays short.
        let identity = transaction_identity(raw_transaction);
        let mut counts = self.counts();
        let submitted_for_identity = counts.per_identity.get(&identity).copied().unwrap_or(0);
        let limit = if max_submitted > 0 && counts.total >= max_submitted {
            Some(SubmissionLimit::Global(max_submitted))
        } else if max_per_identity > 0 && submitted_for_identity >= max_per_identity {
            Some(SubmissionLimit::PerIdentity(max_per_identity))
        } else {
            None
        };
        if let Some(limit) = limit {
            return Err(SubmissionCapacityExhausted {
                limit,
                identity,
                submitted_total: counts.total,
                submitted_for_identity,
            });
        }
        counts.acquire(identity);
        Ok(SubmissionReservation {
            counts: self.submission_counts.clone(),
            identity,
            held: true,
        })
    }

    /// The newest `Submitted` transaction of a (signer, nonce) slot: the one a replacement
    /// would replace. `None` when nothing is pending at that nonce.
    pub fn replacement_target(&self, signer: [u8; 20], nonce: u64) -> Option<ReplacementTarget> {
        // The slot guard is released before `transactions` is read (see the lock order).
        let id = self.nonce_slots.get(&(signer, nonce))?.ids.last().copied()?;
        let record = self.transactions.get(&id)?;
        (record.status == TransactionStatus::Submitted).then(|| ReplacementTarget {
            id,
            transaction_hash: record.transaction_hash,
            raw_transaction: record.raw_transaction.clone(),
        })
    }

    /// Claim the right to broadcast one replacement of `target`, the newest transaction of its
    /// (signer, nonce) slot, as one step on the slot.
    ///
    /// Of several replacements racing for the same slot exactly one gets a ticket; the others are
    /// refused before anything is broadcast. At most `max_replacements` replacements are made per
    /// slot, the original not counted.
    ///
    /// # Errors
    /// Returns the [`ReplacementRefused`] reason when no ticket can be granted.
    pub fn begin_replacement(
        &self,
        signer: [u8; 20],
        nonce: u64,
        target: Uuid,
        max_replacements: usize,
    ) -> Result<ReplacementTicket, ReplacementRefused> {
        let key = (signer, nonce);
        let mut slot = self.nonce_slots.get_mut(&key).ok_or(ReplacementRefused::TargetMoved)?;
        if slot.ids.last() != Some(&target) {
            return Err(ReplacementRefused::TargetMoved);
        }
        if slot.replacing {
            return Err(ReplacementRefused::InProgress);
        }
        if slot.replacements >= max_replacements {
            return Err(ReplacementRefused::LimitReached { max: max_replacements });
        }
        slot.replacing = true;
        Ok(ReplacementTicket {
            nonce_slots: self.nonce_slots.clone(),
            key,
            target,
            held: true,
        })
    }

    /// Track a broadcast replacement, handing the reserved capacity over to its record.
    ///
    /// The replaced transaction stays `Submitted` and monitored like any other record: either
    /// one may still be mined. The ticket keeps every other replacement of the slot out until
    /// this one is counted.
    ///
    /// # Errors
    /// Returns `TransactionStoreError::AlreadyExists` if a transaction with the same ID already exists
    pub fn commit_replacement(
        &self,
        mut ticket: ReplacementTicket,
        record: TransactionRecord,
        reservation: SubmissionReservation,
    ) -> Result<(), TransactionStoreError> {
        let (hash, id) = (record.transaction_hash, record.id);
        self.insert_reserved_unindexed(record, reservation)?;
        if let Some(mut slot) = self.nonce_slots.get_mut(&ticket.key) {
            slot.replacements += 1;
            slot.replacing = false;
        }
        ticket.held = false;
        self.index_hash(hash, id);
        Ok(())
    }

    /// Number of transactions currently awaiting receipt monitoring.
    pub fn submitted_count(&self) -> usize {
        self.submitted_identities.len()
    }

    /// Get the total count of transactions in the store
    pub fn count(&self) -> usize {
        self.transactions.len()
    }

    /// Evict terminal records past `terminal_ttl` and, independently, the oldest terminal
    /// records above `max_terminal_records`. A zero TTL or cap disables that limit.
    ///
    /// Submitted records are never evicted, so receipt monitoring and its admission counters
    /// remain intact. Returns the number of records removed.
    pub fn evict_terminal_records(&self, terminal_ttl: Duration, max_terminal_records: usize) -> usize {
        let now = Utc::now();
        let mut terminal_records = self
            .transactions
            .iter()
            .filter_map(|entry| {
                let record = entry.value();
                if !record.status.is_terminal() {
                    return None;
                }
                let id = *entry.key();
                let terminal_at = match self.terminal_timestamps.get(&id) {
                    Some(timestamp) => timestamp.value().to_owned(),
                    None => {
                        self.terminal_timestamps.insert(id, now);
                        now
                    }
                };
                Some((id, terminal_at))
            })
            .collect::<Vec<_>>();

        terminal_records.sort_unstable_by_key(|(_, terminal_at)| terminal_at.to_owned());

        let mut eviction_ids = HashSet::new();
        if !terminal_ttl.is_zero() {
            eviction_ids.extend(terminal_records.iter().filter_map(|(id, terminal_at)| {
                now.signed_duration_since(terminal_at.to_owned())
                    .to_std()
                    .ok()
                    .filter(|age| *age >= terminal_ttl)
                    .map(|_| *id)
            }));
        }

        if max_terminal_records != 0 {
            let remaining = terminal_records
                .iter()
                .filter(|(id, _)| !eviction_ids.contains(id))
                .collect::<Vec<_>>();
            let overflow = remaining.len().saturating_sub(max_terminal_records);
            eviction_ids.extend(remaining.into_iter().take(overflow).map(|(id, _)| *id));
        }

        let removed = eviction_ids
            .into_iter()
            .filter_map(|id| {
                self.transactions
                    .remove_if(&id, |_, record| record.status.is_terminal())
                    .map(|(id, record)| (id, record.transaction_hash))
            })
            .collect::<Vec<_>>();
        for (id, hash) in &removed {
            self.terminal_timestamps.remove(id);
            self.hash_index.remove_if(hash, |_, indexed| indexed == id);
        }
        if !removed.is_empty() {
            self.update_size_metric();
        }
        removed.len()
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

/// Whether the RPC node can still be holding this tracked transaction.
///
/// That is a transaction still awaiting confirmation, or one that timed out because it was
/// underpriced, queued or waiting behind a nonce gap. A dropped or replaced transaction is no
/// longer held, so a resubmission of it is a fresh one.
///
/// A transaction that timed out with an unknown cause (or none recorded) is held only when
/// `verified_known` says the node just returned it for its hash.
fn is_held_by_node(record: &TransactionRecord, verified_known: bool) -> bool {
    match record.status {
        TransactionStatus::Submitted => true,
        TransactionStatus::Timeout => match record.diagnosis {
            Some(diagnosis) => match diagnosis.reason {
                UnminedReason::Underpriced | UnminedReason::Queued | UnminedReason::NonceGap => true,
                UnminedReason::Unknown => verified_known,
                UnminedReason::Dropped | UnminedReason::Replaced => false,
            },
            None => verified_known,
        },
        _ => false,
    }
}

/// Identity used to account for submission capacity: the recovered signer of
/// the transaction. All envelopes whose signer cannot be recovered share the
/// `None` bucket.
fn transaction_identity(raw_transaction: &[u8]) -> SubmissionIdentity {
    decode_transaction_signer(raw_transaction)
}

/// Capacity identity and (signer, nonce) slot key of a raw transaction, from one signer recovery.
fn signer_and_nonce(raw_transaction: &[u8]) -> (SubmissionIdentity, Option<NonceKey>) {
    let signer_and_nonce = decode_transaction_signer_and_nonce(raw_transaction);
    (signer_and_nonce.map(|(signer, _)| signer), signer_and_nonce)
}

impl Default for TransactionStore {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use std::thread;

    use chrono::Duration as ChronoDuration;
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
            diagnosis: None,
            resume_generation: 0,
            last_resumed_at: None,
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
            diagnosis: None,
            resume_generation: 0,
            last_resumed_at: None,
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

    fn diagnosis_with(reason: UnminedReason) -> TransactionDiagnosis {
        TransactionDiagnosis {
            reason,
            signed_fees: None,
            recommended_fees: None,
            min_replacement_fees: None,
            retry_after: Duration::from_secs(5),
        }
    }

    #[test]
    fn test_resume_tracking_on_a_timed_out_record_keeps_it_listed_and_counted() {
        let store = TransactionStore::new();
        store
            .insert(submitted_record(TEST_UUID, vec![0x01, 0x02, 0x03]))
            .unwrap();
        let submitted_at = store.get(TEST_UUID).unwrap().submitted_at;

        assert!(
            store
                .mark_timeout_if_unchanged(TEST_UUID, 0, None, Some(diagnosis_with(UnminedReason::Queued)))
                .unwrap()
        );
        assert_eq!(store.submitted_count(), 0);

        let previous = store.resume_tracking(TEST_UUID).unwrap();
        assert_eq!(previous, Some(diagnosis_with(UnminedReason::Queued)));

        let record = store.get(TEST_UUID).unwrap();
        assert_eq!(record.status, TransactionStatus::Submitted);
        assert_eq!(record.diagnosis, None);
        assert_eq!(record.submitted_at, submitted_at);
        assert_eq!(record.resume_generation, 1);
        assert!(record.last_resumed_at.is_some());
        assert_eq!(store.submitted_count(), 1);
        assert_eq!(store.list_submitted_fair().len(), 1);
    }

    #[test]
    fn test_resume_tracking_on_a_submitted_record_keeps_its_timeout_clock() {
        let store = TransactionStore::new();
        store
            .insert(submitted_record(TEST_UUID, vec![0x01, 0x02, 0x03]))
            .unwrap();
        let submitted_at = store.get(TEST_UUID).unwrap().submitted_at;

        assert_eq!(store.resume_tracking(TEST_UUID).unwrap(), None);

        let record = store.get(TEST_UUID).unwrap();
        assert_eq!(record.status, TransactionStatus::Submitted);
        assert_eq!(record.submitted_at, submitted_at);
        assert_eq!(record.resume_generation, 0);
        assert_eq!(record.last_resumed_at, None);
        assert_eq!(record.timeout_clock_start(), submitted_at);
        assert_eq!(store.submitted_count(), 1);
        assert_eq!(store.list_submitted_fair().len(), 1);
    }

    #[test]
    fn test_repeated_resume_keeps_the_clock_set_by_the_timeout_resumption() {
        let store = TransactionStore::new();
        store
            .insert(submitted_record(TEST_UUID, vec![0x01, 0x02, 0x03]))
            .unwrap();
        assert!(store.mark_timeout_if_unchanged(TEST_UUID, 0, None, None).unwrap());

        store.resume_tracking(TEST_UUID).unwrap();
        let resumed = store.get(TEST_UUID).unwrap();
        assert_eq!(resumed.resume_generation, 1);
        assert!(resumed.last_resumed_at.is_some());

        // Identical retries of the now in-flight record leave the clock and generation alone.
        for _ in 0..3 {
            store.resume_tracking(TEST_UUID).unwrap();
        }
        let retried = store.get(TEST_UUID).unwrap();
        assert_eq!(retried.resume_generation, 1);
        assert_eq!(retried.last_resumed_at, resumed.last_resumed_at);
    }

    #[test]
    fn test_mark_timeout_if_unchanged_skips_stale_and_non_submitted_records() {
        let store = TransactionStore::new();
        store
            .insert(submitted_record(TEST_UUID, vec![0x01, 0x02, 0x03]))
            .unwrap();
        let submitted_at = store.get(TEST_UUID).unwrap().submitted_at;

        assert!(store.mark_timeout_if_unchanged(TEST_UUID, 0, None, None).unwrap());
        store.resume_tracking(TEST_UUID).unwrap();
        assert!(!store.mark_timeout_if_unchanged(TEST_UUID, 0, None, None).unwrap());
        assert_eq!(store.get(TEST_UUID).unwrap().status, TransactionStatus::Submitted);

        store
            .update_status(TEST_UUID, TransactionStatus::Confirmed, None)
            .unwrap();
        assert!(!store.mark_timeout_if_unchanged(TEST_UUID, 1, None, None).unwrap());
        assert_eq!(store.get(TEST_UUID).unwrap().status, TransactionStatus::Confirmed);
    }

    /// Assert every `Submitted` record is both listed for polling and counted.
    fn assert_submitted_listed_and_counted(store: &TransactionStore, ids: &[Uuid]) {
        let submitted: HashSet<Uuid> = ids
            .iter()
            .copied()
            .filter(|id| store.get(*id).unwrap().status == TransactionStatus::Submitted)
            .collect();
        let listed: HashSet<Uuid> = store.list_submitted_fair().iter().map(|record| record.id).collect();
        assert_eq!(listed, submitted);
        assert_eq!(store.submitted_count(), submitted.len());
        assert_nonce_slots_match(store, ids);
    }

    /// Assert the (signer, nonce) slots hold exactly the `Submitted` records among `ids` that
    /// have a recoverable signer, and that the reverse index agrees.
    fn assert_nonce_slots_match(store: &TransactionStore, ids: &[Uuid]) {
        let mut expected: HashMap<NonceKey, HashSet<Uuid>> = HashMap::new();
        for id in ids {
            let record = store.get(*id).unwrap();
            let key = decode_transaction_signer_and_nonce(&record.raw_transaction);
            match (record.status, key) {
                (TransactionStatus::Submitted, Some(key)) => {
                    expected.entry(key).or_default().insert(*id);
                    assert_eq!(store.submitted_nonces.get(id).map(|entry| *entry.value()), Some(key));
                }
                _ => assert!(store.submitted_nonces.get(id).is_none(), "{id} must not be in a slot"),
            }
        }
        let actual: HashMap<NonceKey, HashSet<Uuid>> = store
            .nonce_slots
            .iter()
            .map(|slot| (*slot.key(), slot.value().ids.iter().copied().collect()))
            .collect();
        assert_eq!(actual, expected);
        for slot in store.nonce_slots.iter() {
            assert_eq!(
                slot.value().ids.len(),
                slot.value().ids.iter().collect::<HashSet<_>>().len()
            );
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn test_submitted_index_stays_consistent_under_concurrent_writers() {
        let store = Arc::new(TransactionStore::new());
        let ids: Vec<Uuid> = (0..8u128).map(Uuid::from_u128).collect();
        for id in &ids {
            store.insert(submitted_record(*id, vec![id.as_u128() as u8])).unwrap();
        }

        let mut tasks = Vec::new();
        {
            let store = store.clone();
            tasks.push(tokio::task::spawn_blocking(move || {
                for _ in 0..300 {
                    store.list_submitted_fair();
                    store.submitted_count();
                }
            }));
        }
        for worker in 0..3usize {
            let store = store.clone();
            let ids = ids.clone();
            tasks.push(tokio::task::spawn_blocking(move || {
                for round in 0..300usize {
                    for id in &ids {
                        match worker {
                            0 => {
                                store.resume_tracking(*id).unwrap();
                            }
                            1 => {
                                let status = if round % 2 == 0 {
                                    TransactionStatus::Timeout
                                } else {
                                    TransactionStatus::Submitted
                                };
                                store.update_status(*id, status, None).unwrap();
                            }
                            _ => {
                                let generation = store.get(*id).unwrap().resume_generation;
                                store.mark_timeout_if_unchanged(*id, generation, None, None).unwrap();
                            }
                        }
                    }
                }
            }));
        }

        tokio::time::timeout(Duration::from_secs(60), async {
            for task in tasks {
                task.await.unwrap();
            }
        })
        .await
        .expect("concurrent store operations hung");

        assert_submitted_listed_and_counted(&store, &ids);
    }

    #[tokio::test]
    async fn test_submitted_index_follows_update_with_a_changed_signer_and_stale_timeouts() {
        let store = TransactionStore::new();
        let first = PrivateKeySigner::random();
        let second = PrivateKeySigner::random();
        let id = Uuid::from_u128(1);
        store
            .insert(submitted_record(id, signed_raw_tx(&first, [0x11; 20], 0).await))
            .unwrap();

        store
            .update(submitted_record(id, signed_raw_tx(&second, [0x11; 20], 0).await))
            .unwrap();
        assert_submitted_listed_and_counted(&store, &[id]);

        assert!(store.mark_timeout_if_unchanged(id, 0, None, None).unwrap());
        store.resume_tracking(id).unwrap();
        assert!(!store.mark_timeout_if_unchanged(id, 0, None, None).unwrap());
        assert_submitted_listed_and_counted(&store, &[id]);
        assert_eq!(store.list_submitted_fair().len(), 1);
    }

    #[tokio::test]
    async fn test_retry_of_old_bytes_does_not_resume_a_record_whose_hash_changed() {
        let store = TransactionStore::new();
        let signer = PrivateKeySigner::random();
        let old_raw = signed_raw_tx(&signer, [0x11; 20], 0).await;
        let old_hash = crate::safe_execution::decode_transaction_summary(&old_raw)
            .unwrap()
            .transaction_hash;
        let id = Uuid::from_u128(1);
        let mut record = submitted_record(id, old_raw.clone());
        record.transaction_hash = old_hash;
        store.insert(record).unwrap();

        // The record is replaced by another transaction; the old hash is then indexed to it
        // only by a stale entry.
        let new_raw = signed_raw_tx(&signer, [0x11; 20], 1).await;
        let mut replacement = submitted_record(id, new_raw);
        replacement.transaction_hash = crate::safe_execution::decode_transaction_summary(&replacement.raw_transaction)
            .unwrap()
            .transaction_hash;
        store.update(replacement).unwrap();
        store.hash_index.insert(old_hash, id);

        assert!(!store.has_in_flight_hash(old_hash));
        assert!(store.resume_in_flight_hash(old_hash).is_none());

        let mut retry = submitted_record(Uuid::from_u128(2), old_raw.clone());
        retry.transaction_hash = old_hash;
        let outcome = store.insert_or_resume(retry, reserve(&store, &old_raw), true).unwrap();
        assert!(matches!(outcome, TrackOutcome::Inserted));
        assert_eq!(store.get(id).unwrap().resume_generation, 0);
    }

    #[test]
    fn test_insert_or_resume_matches_only_transactions_the_node_can_still_hold() {
        let held = [
            (TransactionStatus::Submitted, None),
            (TransactionStatus::Timeout, Some(UnminedReason::Underpriced)),
            (TransactionStatus::Timeout, Some(UnminedReason::Queued)),
            (TransactionStatus::Timeout, Some(UnminedReason::NonceGap)),
        ];
        let not_held = [
            (TransactionStatus::Timeout, Some(UnminedReason::Dropped)),
            (TransactionStatus::Timeout, Some(UnminedReason::Replaced)),
            (TransactionStatus::Timeout, Some(UnminedReason::Unknown)),
            (TransactionStatus::Timeout, None),
            (TransactionStatus::Confirmed, None),
            (TransactionStatus::Reverted, None),
        ];

        for (resumed, cases) in [(true, held.as_slice()), (false, not_held.as_slice())] {
            for (status, reason) in cases {
                let store = TransactionStore::new();
                store
                    .insert(TransactionRecord {
                        id: TEST_UUID,
                        raw_transaction: vec![0x01, 0x02, 0x03],
                        transaction_hash: test_tx_hash(),
                        status: *status,
                        submitted_at: test_timestamp(),
                        confirmed_at: None,
                        error_message: None,
                        safe_execution: None,
                        diagnosis: reason.map(diagnosis_with),
                        resume_generation: 0,
                        last_resumed_at: None,
                    })
                    .unwrap();

                let other = Uuid::from_u128(2);
                let outcome = store
                    .insert_or_resume(
                        submitted_record(other, vec![0x01, 0x02, 0x03]),
                        reserve(&store, &[0x01, 0x02, 0x03]),
                        false,
                    )
                    .unwrap();
                let expected = if resumed {
                    TrackOutcome::Resumed {
                        existing: TEST_UUID,
                        diagnosis: reason.map(diagnosis_with),
                    }
                } else {
                    TrackOutcome::Inserted
                };
                assert_eq!(outcome, expected, "{status:?} {reason:?}");
                assert_eq!(store.get(other).is_ok(), !resumed, "{status:?} {reason:?}");
            }
        }
    }

    #[test]
    fn test_insert_or_resume_resumes_an_unknown_timeout_only_when_the_node_verifiably_holds_it() {
        let cases = [Some(UnminedReason::Unknown), None];
        for reason in cases {
            for (verified_known, resumed) in [(false, false), (true, true)] {
                let store = TransactionStore::new();
                let mut record = submitted_record(TEST_UUID, vec![0x01, 0x02, 0x03]);
                record.status = TransactionStatus::Timeout;
                record.diagnosis = reason.map(diagnosis_with);
                store.insert(record).unwrap();

                let other = Uuid::from_u128(2);
                let outcome = store
                    .insert_or_resume(
                        submitted_record(other, vec![0x01, 0x02, 0x03]),
                        reserve(&store, &[0x01, 0x02, 0x03]),
                        verified_known,
                    )
                    .unwrap();

                assert_eq!(
                    outcome == TrackOutcome::Inserted,
                    !resumed,
                    "{reason:?} {verified_known}"
                );
                let original = store.get(TEST_UUID).unwrap();
                assert_eq!(
                    original.status,
                    if resumed {
                        TransactionStatus::Submitted
                    } else {
                        TransactionStatus::Timeout
                    }
                );
                // A verified-known hash never ends up tracked under two identities.
                assert_eq!(store.count(), if resumed { 1 } else { 2 });
            }
        }

        // A dropped or replaced transaction is not resumed even when the node answers
        // already-known: its diagnosis is the more specific evidence.
        for reason in [UnminedReason::Dropped, UnminedReason::Replaced] {
            let store = TransactionStore::new();
            let mut record = submitted_record(TEST_UUID, vec![0x01, 0x02, 0x03]);
            record.status = TransactionStatus::Timeout;
            record.diagnosis = Some(diagnosis_with(reason));
            store.insert(record).unwrap();
            let outcome = store
                .insert_or_resume(
                    submitted_record(Uuid::from_u128(2), vec![0x01, 0x02, 0x03]),
                    reserve(&store, &[0x01, 0x02, 0x03]),
                    true,
                )
                .unwrap();
            assert_eq!(outcome, TrackOutcome::Inserted);
        }
    }

    #[test]
    fn test_insert_or_resume_ignores_other_hashes() {
        let store = TransactionStore::new();
        store
            .insert(submitted_record(TEST_UUID, vec![0x01, 0x02, 0x03]))
            .unwrap();

        let mut record = submitted_record(Uuid::from_u128(2), vec![0x04]);
        record.transaction_hash = Hash::from([0x01; 32]);
        let outcome = store.insert_or_resume(record, reserve(&store, &[0x04]), false).unwrap();

        assert_eq!(outcome, TrackOutcome::Inserted);
        assert_eq!(store.count(), 2);
    }

    #[tokio::test]
    async fn test_concurrent_insert_or_resume_of_one_hash_yields_one_record() {
        let store = Arc::new(TransactionStore::new());

        let tasks = (0..16u128)
            .map(|i| {
                let store = store.clone();
                tokio::task::spawn_blocking(move || {
                    let raw = vec![0x01, 0x02, 0x03];
                    let reservation = store.try_reserve_submission(&raw, 0, 0).unwrap();
                    store.insert_or_resume(submitted_record(Uuid::from_u128(i + 1), raw), reservation, false)
                })
            })
            .collect::<Vec<_>>();
        let mut inserted = 0;
        for task in tasks {
            if task.await.unwrap().unwrap() == TrackOutcome::Inserted {
                inserted += 1;
            }
        }

        assert_eq!(inserted, 1);
        assert_eq!(store.count(), 1);
        assert_eq!(store.submitted_count(), 1);
        assert_eq!(store.counts().total, 1);
    }

    #[test]
    fn test_hash_index_follows_replaced_and_evicted_records() {
        let store = TransactionStore::new();
        store
            .insert(submitted_record(TEST_UUID, vec![0x01, 0x02, 0x03]))
            .unwrap();

        // A replacement under a new hash moves the index: the old hash is free again.
        let mut replaced = submitted_record(TEST_UUID, vec![0x01, 0x02, 0x03]);
        replaced.transaction_hash = Hash::from([0x02; 32]);
        store.update(replaced).unwrap();
        let outcome = store
            .insert_or_resume(
                submitted_record(Uuid::from_u128(2), vec![0x01, 0x02, 0x03]),
                reserve(&store, &[0x01, 0x02, 0x03]),
                false,
            )
            .unwrap();
        assert_eq!(outcome, TrackOutcome::Inserted);

        // An evicted record leaves nothing behind in the index.
        store
            .update_status(TEST_UUID, TransactionStatus::Confirmed, None)
            .unwrap();
        store
            .update_status(Uuid::from_u128(2), TransactionStatus::Confirmed, None)
            .unwrap();
        thread::sleep(Duration::from_millis(5));
        assert_eq!(store.evict_terminal_records(Duration::from_millis(1), 0), 2);
        assert_eq!(store.hash_index.len(), 0);
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
            diagnosis: None,
            resume_generation: 0,
            last_resumed_at: None,
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
                diagnosis: None,
                resume_generation: 0,
                last_resumed_at: None,
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
                    diagnosis: None,
                    resume_generation: 0,
                    last_resumed_at: None,
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
                    diagnosis: None,
                    resume_generation: 0,
                    last_resumed_at: None,
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
            diagnosis: None,
            resume_generation: 0,
            last_resumed_at: None,
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
            diagnosis: None,
            resume_generation: 0,
            last_resumed_at: None,
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
            diagnosis: None,
            resume_generation: 0,
            last_resumed_at: None,
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
            diagnosis: None,
            resume_generation: 0,
            last_resumed_at: None,
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
            diagnosis: None,
            resume_generation: 0,
            last_resumed_at: None,
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
            diagnosis: None,
            resume_generation: 0,
            last_resumed_at: None,
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
            diagnosis: None,
            resume_generation: 0,
            last_resumed_at: None,
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

    fn reserve(store: &TransactionStore, raw_transaction: &[u8]) -> SubmissionReservation {
        store.try_reserve_submission(raw_transaction, 0, 0).unwrap()
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
            diagnosis: None,
            resume_generation: 0,
            last_resumed_at: None,
        }
    }

    #[tokio::test]
    async fn replacing_a_submitted_record_moves_the_identity_count() {
        let store = TransactionStore::new();
        let first = PrivateKeySigner::random();
        let second = PrivateKeySigner::random();

        let id = Uuid::new_v4();
        store
            .insert(submitted_record(id, signed_raw_tx(&first, [0x11; 20], 0).await))
            .expect("insert failed");
        assert!(
            store
                .try_reserve_submission(&signed_raw_tx(&first, [0x11; 20], 1).await, 0, 1)
                .is_err()
        );

        // Replace the tracked record with one signed by somebody else. The count must follow
        // the new signer, or the old one stays charged for a transaction it no longer owns
        // while the new one submits for free.
        let mut replacement = submitted_record(id, signed_raw_tx(&second, [0x11; 20], 0).await);
        replacement.status = TransactionStatus::Submitted;
        store.update(replacement).expect("update failed");

        assert!(
            store
                .try_reserve_submission(&signed_raw_tx(&first, [0x11; 20], 2).await, 0, 1)
                .is_ok(),
            "the previous signer should no longer be charged"
        );
        assert!(
            store
                .try_reserve_submission(&signed_raw_tx(&second, [0x11; 20], 1).await, 0, 1)
                .is_err(),
            "the replacement signer should now be charged"
        );
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
        assert!(store.try_reserve_submission(&second, 0, 1).is_err());

        // A different signer is unaffected by the first signer's usage.
        let other = signed_raw_tx(&PrivateKeySigner::random(), [0x11; 20], 0).await;
        assert!(store.try_reserve_submission(&other, 0, 1).is_ok());
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
        assert!(store.try_reserve_submission(&next, 0, 0).is_ok());
    }

    #[tokio::test]
    async fn test_terminal_status_releases_capacity() {
        let store = TransactionStore::new();
        let signer = PrivateKeySigner::random();
        let raw = signed_raw_tx(&signer, [0x11; 20], 0).await;
        let id = Uuid::new_v4();

        store.insert(submitted_record(id, raw.clone())).unwrap();
        assert_eq!(store.submitted_count(), 1);
        assert!(store.try_reserve_submission(&raw, 1, 1).is_err());

        store.update_status(id, TransactionStatus::Confirmed, None).unwrap();
        assert_eq!(store.submitted_count(), 0);
        assert!(store.try_reserve_submission(&raw, 1, 1).is_ok());

        // The per-identity bucket is dropped once it reaches zero.
        assert!(store.counts().per_identity.is_empty());
    }

    #[tokio::test]
    async fn test_confirm_with_safe_execution_releases_capacity() {
        let store = TransactionStore::new();
        let raw = signed_raw_tx(&PrivateKeySigner::random(), [0x11; 20], 0).await;
        let id = Uuid::new_v4();

        store.insert(submitted_record(id, raw)).unwrap();
        store.confirm_with_safe_execution(id, None).unwrap();

        assert_eq!(store.submitted_count(), 0);
        assert!(store.counts().per_identity.is_empty());
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
    fn test_public_record_omits_raw_transaction() {
        let store = TransactionStore::new();
        store
            .insert(submitted_record(TEST_UUID, vec![0x01, 0x02, 0x03]))
            .unwrap();

        assert_eq!(store.status(TEST_UUID), Ok(TransactionStatus::Submitted));
        assert!(store.get_public(TEST_UUID).unwrap().raw_transaction.is_empty());
        assert_eq!(store.get(TEST_UUID).unwrap().raw_transaction, vec![0x01, 0x02, 0x03]);
    }

    #[test]
    fn test_terminal_ttl_evicts_only_terminal_records() {
        let store = TransactionStore::new();
        let submitted_id = Uuid::new_v4();
        let expired_id = Uuid::new_v4();
        let recent_id = Uuid::new_v4();

        store.insert(submitted_record(submitted_id, vec![0x01])).unwrap();

        let mut expired = submitted_record(expired_id, vec![0x02]);
        expired.status = TransactionStatus::Timeout;
        store.insert(expired).unwrap();
        store
            .terminal_timestamps
            .insert(expired_id, Utc::now() - ChronoDuration::hours(2));

        let mut recent = submitted_record(recent_id, vec![0x03]);
        recent.status = TransactionStatus::Reverted;
        store.insert(recent).unwrap();

        assert_eq!(store.evict_terminal_records(Duration::from_secs(60 * 60), 0), 1);
        assert!(store.get(submitted_id).is_ok());
        assert!(matches!(store.get(expired_id), Err(TransactionStoreError::NotFound(_))));
        assert!(store.get(recent_id).is_ok());
        assert_eq!(store.submitted_count(), 1);
    }

    #[test]
    fn test_terminal_cap_evicts_oldest_records_first() {
        let store = TransactionStore::new();
        let oldest_id = Uuid::new_v4();
        let middle_id = Uuid::new_v4();
        let newest_id = Uuid::new_v4();

        for id in [oldest_id, middle_id, newest_id] {
            let mut record = submitted_record(id, vec![0x01]);
            record.status = TransactionStatus::Confirmed;
            store.insert(record).unwrap();
        }
        let now = Utc::now();
        store
            .terminal_timestamps
            .insert(oldest_id, now - ChronoDuration::minutes(3));
        store
            .terminal_timestamps
            .insert(middle_id, now - ChronoDuration::minutes(2));
        store
            .terminal_timestamps
            .insert(newest_id, now - ChronoDuration::minutes(1));

        assert_eq!(store.evict_terminal_records(Duration::ZERO, 1), 2);
        assert!(matches!(store.get(oldest_id), Err(TransactionStoreError::NotFound(_))));
        assert!(matches!(store.get(middle_id), Err(TransactionStoreError::NotFound(_))));
        assert!(store.get(newest_id).is_ok());
    }

    #[tokio::test]
    async fn test_reservation_holds_capacity_until_dropped() {
        let store = TransactionStore::new();
        let signer = PrivateKeySigner::random();
        let first = signed_raw_tx(&signer, [0x11; 20], 0).await;
        let second = signed_raw_tx(&signer, [0x11; 20], 1).await;

        // An in-flight reservation counts against both limits, so a concurrent submission
        // cannot slip through while the first one is still being broadcast.
        let reservation = store.try_reserve_submission(&first, 1, 0).expect("capacity available");
        assert!(store.try_reserve_submission(&second, 1, 0).is_err());
        assert!(store.try_reserve_submission(&second, 0, 1).is_err());

        // A failed broadcast drops the reservation and gives the slot back.
        drop(reservation);
        assert!(store.try_reserve_submission(&second, 1, 1).is_ok());
        assert!(store.counts().per_identity.is_empty());
    }

    #[tokio::test]
    async fn test_insert_reserved_transfers_capacity_without_double_counting() {
        let store = TransactionStore::new();
        let signer = PrivateKeySigner::random();
        let raw = signed_raw_tx(&signer, [0x11; 20], 0).await;
        let id = Uuid::new_v4();

        let reservation = store.try_reserve_submission(&raw, 0, 0).expect("capacity available");
        store
            .insert_reserved(submitted_record(id, raw), reservation)
            .expect("insert failed");
        assert_eq!(store.submitted_count(), 1);
        assert_eq!(store.counts().total, 1);

        store.update_status(id, TransactionStatus::Confirmed, None).unwrap();
        assert_eq!(store.counts().total, 0);
        assert!(store.counts().per_identity.is_empty());
    }

    #[tokio::test]
    async fn test_failed_insert_reserved_releases_capacity() {
        let store = TransactionStore::new();
        let signer = PrivateKeySigner::random();
        let raw = signed_raw_tx(&signer, [0x11; 20], 0).await;
        let id = Uuid::new_v4();
        store.insert(submitted_record(id, raw.clone())).unwrap();

        let reservation = store.try_reserve_submission(&raw, 0, 0).expect("capacity available");
        assert_eq!(
            store.insert_reserved(submitted_record(id, raw), reservation),
            Err(TransactionStoreError::AlreadyExists(id))
        );
        assert_eq!(store.counts().total, 1);
    }

    #[test]
    fn test_undecodable_transactions_share_one_bucket() {
        let store = TransactionStore::new();

        store.insert(submitted_record(Uuid::new_v4(), vec![0xff])).unwrap();

        // A second undecodable envelope lands in the same `None` bucket rather
        // than creating an unbounded number of identities.
        assert!(store.try_reserve_submission(&[0xfe], 0, 1).is_err());
        assert_eq!(store.counts().per_identity.len(), 1);
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

    /// Build a raw signed legacy transaction with an explicit gas price, so two of them at the
    /// same nonce are distinct transactions.
    async fn signed_raw_tx_priced(signer: &PrivateKeySigner, nonce: u64, gas_price: u128) -> Vec<u8> {
        let tx = TxLegacy {
            chain_id: Some(1),
            nonce,
            gas_price,
            gas_limit: 21_000,
            to: TxKind::Call(AlloyAddress::from_slice(&[0x11; 20])),
            value: U256::ZERO,
            input: Default::default(),
        };
        let signature = signer.sign_hash(&tx.signature_hash()).await.expect("signing failed");
        let mut encoded = Vec::new();
        tx.into_signed(signature).encode_2718(&mut encoded);
        encoded
    }

    /// A `Submitted` record carrying the real hash of `raw_transaction`.
    fn hashed_record(raw_transaction: Vec<u8>) -> TransactionRecord {
        let hash = crate::safe_execution::decode_transaction_summary(&raw_transaction)
            .expect("decodable")
            .transaction_hash;
        let mut record = submitted_record(Uuid::new_v4(), raw_transaction);
        record.transaction_hash = hash;
        record
    }

    fn signer_of(signer: &PrivateKeySigner) -> [u8; 20] {
        signer.address().into_array()
    }

    /// Track `first` and replace it with `gas_price`, returning both ids.
    async fn track_and_replace(
        store: &TransactionStore,
        signer: &PrivateKeySigner,
        nonce: u64,
        max: usize,
    ) -> (Uuid, Uuid) {
        let original = hashed_record(signed_raw_tx_priced(signer, nonce, 1_000).await);
        let original_id = original.id;
        store.insert(original).unwrap();
        let target = store.replacement_target(signer_of(signer), nonce).expect("tracked");
        assert_eq!(target.id, original_id);
        let replacement = hashed_record(signed_raw_tx_priced(signer, nonce, 2_000).await);
        let replacement_id = replacement.id;
        let reservation = reserve(store, &replacement.raw_transaction);
        let ticket = store
            .begin_replacement(signer_of(signer), nonce, original_id, max)
            .expect("ticket");
        store.commit_replacement(ticket, replacement, reservation).unwrap();
        (original_id, replacement_id)
    }

    #[tokio::test]
    async fn test_replacement_target_is_the_newest_submitted_transaction_of_the_nonce() {
        let store = TransactionStore::new();
        let signer = PrivateKeySigner::random();
        assert!(store.replacement_target(signer_of(&signer), 0).is_none());

        let (original, replacement) = track_and_replace(&store, &signer, 0, 3).await;
        assert_eq!(store.replacement_target(signer_of(&signer), 0).unwrap().id, replacement);
        // Another nonce of the same signer is a different slot.
        assert!(store.replacement_target(signer_of(&signer), 1).is_none());

        // When the newest concludes, the older one that is still pending is the target again.
        store
            .update_status(replacement, TransactionStatus::Timeout, None)
            .unwrap();
        assert_eq!(store.replacement_target(signer_of(&signer), 0).unwrap().id, original);
        store
            .update_status(original, TransactionStatus::Confirmed, None)
            .unwrap();
        assert!(store.replacement_target(signer_of(&signer), 0).is_none());
        assert!(store.nonce_slots.is_empty());
        assert!(store.submitted_nonces.is_empty());
    }

    #[tokio::test]
    async fn test_replacement_is_tracked_next_to_the_original() {
        let store = TransactionStore::new();
        let signer = PrivateKeySigner::random();
        let (original, replacement) = track_and_replace(&store, &signer, 4, 3).await;

        let old = store.get(original).unwrap();
        let new = store.get(replacement).unwrap();
        assert_eq!(old.status, TransactionStatus::Submitted);
        assert_eq!(new.status, TransactionStatus::Submitted);
        // Both are monitored and both hold capacity: the reservation became the new record's slot.
        assert_eq!(store.submitted_count(), 2);
        assert_eq!(store.counts().total, 2);
        assert_submitted_listed_and_counted(&store, &[original, replacement]);
        assert!(store.has_in_flight_hash(old.transaction_hash));
        assert!(store.has_in_flight_hash(new.transaction_hash));
    }

    #[tokio::test]
    async fn test_replacements_are_capped_per_nonce_and_the_cap_survives_eviction() {
        let store = TransactionStore::new();
        let signer = PrivateKeySigner::random();
        let (original, first) = track_and_replace(&store, &signer, 0, 1).await;

        assert_eq!(
            store.begin_replacement(signer_of(&signer), 0, first, 1).err(),
            Some(ReplacementRefused::LimitReached { max: 1 })
        );
        // Zero disables replacements outright.
        assert_eq!(
            store.begin_replacement(signer_of(&signer), 0, first, 0).err(),
            Some(ReplacementRefused::LimitReached { max: 0 })
        );

        // The replacement times out and its terminal record is evicted: the original is the
        // target again, and the count did not go back to zero with the record.
        store.update_status(first, TransactionStatus::Timeout, None).unwrap();
        assert_eq!(store.evict_terminal_records(Duration::ZERO, 1), 0);
        assert_eq!(store.evict_terminal_records(Duration::from_nanos(1), 0), 1);
        assert!(store.get(first).is_err());
        assert_eq!(
            store.begin_replacement(signer_of(&signer), 0, original, 1).err(),
            Some(ReplacementRefused::LimitReached { max: 1 })
        );
        assert!(store.begin_replacement(signer_of(&signer), 0, original, 2).is_ok());
    }

    #[tokio::test]
    async fn test_replacement_ticket_is_exclusive_until_dropped_or_committed() {
        let store = TransactionStore::new();
        let signer = PrivateKeySigner::random();
        let original = hashed_record(signed_raw_tx_priced(&signer, 0, 1_000).await);
        let original_id = original.id;
        store.insert(original).unwrap();

        let ticket = store.begin_replacement(signer_of(&signer), 0, original_id, 3).unwrap();
        assert_eq!(ticket.target(), original_id);
        assert_eq!(
            store.begin_replacement(signer_of(&signer), 0, original_id, 3).err(),
            Some(ReplacementRefused::InProgress)
        );

        // A rejected or failed broadcast drops the ticket, and nothing was counted.
        drop(ticket);
        let ticket = store.begin_replacement(signer_of(&signer), 0, original_id, 1).unwrap();
        drop(ticket);

        // A target that is not the newest of its slot, or not tracked at all, is refused.
        assert_eq!(
            store
                .begin_replacement(signer_of(&signer), 0, Uuid::from_u128(7), 3)
                .err(),
            Some(ReplacementRefused::TargetMoved)
        );
        assert_eq!(
            store.begin_replacement(signer_of(&signer), 5, original_id, 3).err(),
            Some(ReplacementRefused::TargetMoved)
        );
    }

    #[tokio::test]
    async fn test_a_failed_commit_gives_back_the_slot_and_the_capacity() {
        let store = TransactionStore::new();
        let signer = PrivateKeySigner::random();
        let original = hashed_record(signed_raw_tx_priced(&signer, 0, 1_000).await);
        let original_id = original.id;
        store.insert(original).unwrap();

        // A record that already exists makes the insert fail.
        let clash = hashed_record(signed_raw_tx_priced(&signer, 0, 2_000).await);
        let mut duplicate = clash.clone();
        duplicate.id = original_id;
        let reservation = reserve(&store, &duplicate.raw_transaction);
        let ticket = store.begin_replacement(signer_of(&signer), 0, original_id, 3).unwrap();
        assert!(store.commit_replacement(ticket, duplicate, reservation).is_err());

        assert_eq!(store.counts().total, 1);
        assert!(store.begin_replacement(signer_of(&signer), 0, original_id, 3).is_ok());
    }

    #[tokio::test]
    async fn test_replacement_of_an_unrecoverable_signer_is_not_indexed() {
        let store = TransactionStore::new();
        store.insert(submitted_record(Uuid::new_v4(), vec![0xff])).unwrap();
        assert!(store.nonce_slots.is_empty());
        assert!(store.submitted_nonces.is_empty());
    }

    #[tokio::test]
    async fn test_nonce_slot_follows_update_with_a_changed_nonce() {
        let store = TransactionStore::new();
        let signer = PrivateKeySigner::random();
        let id = Uuid::from_u128(1);
        store
            .insert(submitted_record(id, signed_raw_tx(&signer, [0x11; 20], 0).await))
            .unwrap();
        store
            .update(submitted_record(id, signed_raw_tx(&signer, [0x11; 20], 1).await))
            .unwrap();

        assert!(store.replacement_target(signer_of(&signer), 0).is_none());
        assert_eq!(store.replacement_target(signer_of(&signer), 1).unwrap().id, id);
        assert_nonce_slots_match(&store, &[id]);

        store.update_status(id, TransactionStatus::Timeout, None).unwrap();
        store.resume_tracking(id).unwrap();
        assert_nonce_slots_match(&store, &[id]);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn test_racing_replacements_of_one_transaction_grant_exactly_one_ticket() {
        for round in 0..20u64 {
            let store = Arc::new(TransactionStore::new());
            let signer = PrivateKeySigner::random();
            let original = hashed_record(signed_raw_tx_priced(&signer, round, 1_000).await);
            let original_id = original.id;
            store.insert(original).unwrap();

            let mut replacements = Vec::new();
            for attempt in 0..8u128 {
                let raw = signed_raw_tx_priced(&signer, round, 2_000 + attempt).await;
                replacements.push(hashed_record(raw));
            }
            let barrier = Arc::new(std::sync::Barrier::new(replacements.len()));
            let signer_address = signer_of(&signer);
            let handles: Vec<_> = replacements
                .into_iter()
                .map(|record| {
                    let (store, barrier) = (store.clone(), barrier.clone());
                    tokio::task::spawn_blocking(move || {
                        let reservation = store.try_reserve_submission(&record.raw_transaction, 0, 0).unwrap();
                        barrier.wait();
                        match store.begin_replacement(signer_address, round, original_id, 3) {
                            Ok(ticket) => {
                                // Hold the ticket while the others race, as a broadcast would.
                                std::thread::sleep(Duration::from_millis(5));
                                let id = record.id;
                                store.commit_replacement(ticket, record, reservation).unwrap();
                                Some(id)
                            }
                            Err(reason) => {
                                assert_eq!(
                                    reason,
                                    ReplacementRefused::InProgress,
                                    "or TargetMoved after the commit"
                                );
                                None
                            }
                        }
                    })
                })
                .collect();
            let mut winners = Vec::new();
            for handle in handles {
                winners.extend(handle.await.unwrap());
            }

            assert_eq!(winners.len(), 1, "exactly one replacement wins");
            assert_eq!(store.submitted_count(), 2);
            assert_eq!(store.counts().total, 2, "losers released their capacity");
            assert_nonce_slots_match(&store, &[original_id, winners[0]]);
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn test_nonce_slots_stay_consistent_under_concurrent_writers() {
        let store = Arc::new(TransactionStore::new());
        let signer = PrivateKeySigner::random();
        let (first, second) = track_and_replace(&store, &signer, 0, 8).await;
        let third = hashed_record(signed_raw_tx_priced(&signer, 0, 3_000).await);
        let third_id = third.id;
        store.insert(third).unwrap();
        let ids = vec![first, second, third_id];

        let mut tasks = Vec::new();
        {
            let (store, signer_address) = (store.clone(), signer_of(&signer));
            tasks.push(tokio::task::spawn_blocking(move || {
                for _ in 0..300 {
                    store.replacement_target(signer_address, 0);
                    store.list_submitted_fair();
                }
            }));
        }
        for worker in 0..3usize {
            let (store, ids) = (store.clone(), ids.clone());
            tasks.push(tokio::task::spawn_blocking(move || {
                for round in 0..300usize {
                    for id in &ids {
                        match worker {
                            0 => {
                                store.resume_tracking(*id).unwrap();
                            }
                            1 => {
                                let status = if round % 2 == 0 {
                                    TransactionStatus::Timeout
                                } else {
                                    TransactionStatus::Submitted
                                };
                                store.update_status(*id, status, None).unwrap();
                            }
                            _ => {
                                let record = store.get(*id).unwrap();
                                store
                                    .mark_timeout_if_unchanged(*id, record.resume_generation, None, None)
                                    .unwrap();
                            }
                        }
                    }
                }
            }));
        }

        tokio::time::timeout(Duration::from_secs(60), async {
            for task in tasks {
                task.await.unwrap();
            }
        })
        .await
        .expect("concurrent store operations hung");

        assert_submitted_listed_and_counted(&store, &ids);
    }
}
