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
use hopr_types::{crypto::types::Hash, primitive::prelude::Address};
use thiserror::Error;
use tracing::{error, warn};
use uuid::Uuid;

use crate::{
    hopr_policy::{AdmittedAction, HoprPolicy, HoprPolicyConfig, PolicyDecision, SubmissionMode, ValidationReason},
    metrics::{
        STATUS_CONFIRMED, STATUS_REVERTED, STATUS_SUBMISSION_FAILED, STATUS_TIMEOUT, STATUS_VALIDATION_FAILED,
        record_transaction_status,
    },
    safe_execution::decode_transaction_summary,
    transaction_monitor::{ReceiptProvider, SafeAddressChecker, enrich_safe_execution},
    transaction_store::{
        SubmissionLimit, SubmissionReservation, TrackOutcome, TransactionDiagnosis, TransactionRecord,
        TransactionStatus, TransactionStore, TransactionStoreError,
    },
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
    #[error("Transaction timed out: {0}")]
    Timeout(String),
    #[error("Transaction execution failed: {0}")]
    ExecutionFailed(String),
    #[error("Transaction submission is temporarily overloaded")]
    OverloadedError,
    /// A supported HOPR action was refused by the deterministic preflight.
    ///
    /// No reachable chain state would have made the call succeed, so the transaction was
    /// never broadcast.
    #[error("HOPR {operation} rejected before broadcast: {reason}")]
    HoprActionRejected {
        /// Decoded HOPR operation.
        operation: &'static str,
        /// Why the action was refused.
        reason: ValidationReason,
    },
    /// An equivalent logical HOPR action is already being tracked.
    ///
    /// This is a non-failure outcome carried as an error because, like
    /// [`TransactionExecutorError::OverloadedError`], it means nothing was broadcast. Callers
    /// resolve `existing` and report that transaction's identity and status instead.
    #[error("an equivalent HOPR {operation} is already tracked as {existing}")]
    DuplicateHoprAction {
        /// Decoded HOPR operation.
        operation: &'static str,
        /// Identity of the transaction already carrying this logical action.
        existing: Uuid,
    },
    /// The RPC node already holds this exact transaction and it is already tracked.
    ///
    /// Like [`TransactionExecutorError::DuplicateHoprAction`], a non-failure outcome carried as
    /// an error: nothing new is tracked, and callers report the `existing` transaction and its
    /// diagnosis instead.
    #[error("the transaction is already tracked as {existing}")]
    AlreadyTracked {
        /// Identity of the transaction already tracking this raw transaction.
        existing: Uuid,
        /// Why that transaction last timed out unmined, when this resubmission resumed its
        /// tracking. Resuming clears the stored diagnosis, so it is carried here instead.
        diagnosis: Option<TransactionDiagnosis>,
    },
    /// The signer is on cooldown after repeated deterministically invalid submissions.
    #[error("HOPR {operation} suppressed for {}s: {reason}", retry_after.as_secs())]
    HoprActionThrottled {
        /// Decoded HOPR operation.
        operation: &'static str,
        /// Reason of the most recent rejection that led to the cooldown.
        reason: ValidationReason,
        /// How long until this signer may submit this operation again.
        retry_after: Duration,
    },
}

/// Terminal outcome of waiting for a submitted transaction's confirmation.
///
/// Distinguishes failures that occur before the transaction is submitted from
/// failures that occur while waiting for it to be mined, so callers can record
/// the correct `blokli_transaction_status_total` label for each case.
#[derive(Error, Debug, Clone)]
pub enum ConfirmationError {
    /// The transaction was never accepted by the RPC node.
    #[error("submission failed: {0}")]
    SubmissionFailed(String),
    /// The transaction was mined but reverted (receipt status = failure).
    #[error("transaction reverted: {0}")]
    Reverted(String),
    /// The transaction was not mined within the configured timeout.
    #[error("confirmation timed out: {0}")]
    Timeout(String),
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
    ) -> Result<Hash, ConfirmationError>;
}

/// Configuration for the raw transaction executor
#[derive(Debug, Clone)]
pub struct RawTransactionExecutorConfig {
    /// Default number of confirmations to wait for in sync mode
    pub default_confirmations: u64,
    /// Maximum time to wait for confirmations
    pub confirmation_timeout: Duration,
    /// Global number of transactions awaiting receipt monitoring. `0` means unbounded.
    ///
    /// Only the asynchronous submission mode is bounded by this: it is the only
    /// mode that leaves transactions in `Submitted` status for the background
    /// receipt monitor. Sync mode waits inline and stores a terminal record,
    /// and fire-and-forget mode does not track transactions at all.
    pub max_submitted_transactions: usize,
    /// Per-signer limit for transactions awaiting receipt monitoring. `0` means unbounded.
    ///
    /// The signer is recovered from the transaction signature, so one client
    /// cannot consume the global capacity by spreading submissions across
    /// contract targets.
    pub max_submitted_transactions_per_identity: usize,
    /// Enable optional Safe revert-reason tracing in synchronous transaction execution.
    pub enable_revert_reason_tracing: bool,
    /// HOPR-aware policy applied to supported HOPR node-management transactions.
    ///
    /// Transactions the policy does not recognise keep generic Blokli behaviour regardless
    /// of this setting.
    pub hopr_policy: HoprPolicyConfig,
}

impl Default for RawTransactionExecutorConfig {
    fn default() -> Self {
        Self {
            default_confirmations: 3,
            confirmation_timeout: Duration::from_secs(60),
            max_submitted_transactions: 1_024,
            max_submitted_transactions_per_identity: 64,
            enable_revert_reason_tracing: true,
            hopr_policy: HoprPolicyConfig::default(),
        }
    }
}

/// Executor for submitting raw transactions to the blockchain
///
/// Supports three submission modes:
/// - Fire-and-forget: Returns tx hash immediately, no tracking
/// - Async: Returns UUID, tracks in store, background monitoring
/// - Sync: Waits for confirmations, returns complete transaction record
pub struct RawTransactionExecutor<R: RpcClient> {
    rpc_client: Arc<R>,
    transaction_store: Arc<TransactionStore>,
    validator: Arc<TransactionValidator>,
    config: RawTransactionExecutorConfig,
    /// Receipt provider for Safe enrichment in sync mode
    receipt_provider: Option<Arc<dyn ReceiptProvider>>,
    /// Safe address checker for Safe enrichment in sync mode
    safe_checker: Option<Arc<dyn SafeAddressChecker>>,
    /// Optional HOPR-aware policy applied before broadcasting. When unset, every transaction
    /// keeps generic Blokli behaviour.
    hopr_policy: Option<Arc<HoprPolicy>>,
}

impl<R: RpcClient> std::fmt::Debug for RawTransactionExecutor<R> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RawTransactionExecutor")
            .field("config", &self.config)
            .finish_non_exhaustive()
    }
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
            receipt_provider: None,
            safe_checker: None,
            hopr_policy: None,
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
            receipt_provider: None,
            safe_checker: None,
            hopr_policy: None,
        }
    }

    /// Set the Safe enrichment dependencies for sync-mode transactions
    ///
    /// When set, sync-mode transactions targeting Safe contracts will be
    /// enriched with `safeExecution` data (success/failure, revert reason).
    pub fn with_safe_enrichment(
        mut self,
        receipt_provider: Arc<dyn ReceiptProvider>,
        safe_checker: Arc<dyn SafeAddressChecker>,
    ) -> Self {
        self.receipt_provider = Some(receipt_provider);
        self.safe_checker = Some(safe_checker);
        self
    }

    /// Apply the HOPR-aware policy to supported HOPR node-management transactions.
    ///
    /// Transactions the policy does not recognise are unaffected, so enabling it never
    /// changes behaviour for unsupported contracts or functions.
    pub fn with_hopr_policy(mut self, policy: Arc<HoprPolicy>) -> Self {
        self.hopr_policy = Some(policy);
        self
    }

    /// Run the HOPR-aware policy, if one is configured.
    ///
    /// Returns the admitted action to register after a successful broadcast, or `None` when
    /// the policy does not apply to this transaction.
    async fn apply_hopr_policy(
        &self,
        raw_tx: &[u8],
        mode: SubmissionMode,
    ) -> Result<Option<AdmittedAction>, TransactionExecutorError> {
        let Some(policy) = self.hopr_policy.as_ref() else {
            return Ok(None);
        };

        match policy.evaluate(raw_tx, mode).await {
            PolicyDecision::NotApplicable => Ok(None),
            PolicyDecision::Admit(action) => Ok(Some(action)),
            PolicyDecision::Rejected { operation, reason } => {
                record_transaction_status(STATUS_VALIDATION_FAILED);
                Err(TransactionExecutorError::HoprActionRejected { operation, reason })
            }
            PolicyDecision::Duplicate { operation, existing } => {
                Err(TransactionExecutorError::DuplicateHoprAction { operation, existing })
            }
            PolicyDecision::Throttled {
                operation,
                reason,
                retry_after,
            } => Err(TransactionExecutorError::HoprActionThrottled {
                operation,
                reason,
                retry_after,
            }),
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
        if let Err(e) = self.validator.validate_raw_transaction(&raw_tx) {
            warn!(error = %e, "Transaction validation failed");
            record_transaction_status(STATUS_VALIDATION_FAILED);
            return Err(e.into());
        }

        // Fire-and-forget leaves no tracked record, so deduplication cannot return an
        // identity here; preflight and invalid-action suppression still apply.
        self.apply_hopr_policy(&raw_tx, SubmissionMode::Untracked).await?;

        // Submit to RPC
        let tx_hash = match self.rpc_client.send_raw_transaction(raw_tx).await {
            Ok(hash) => hash,
            Err(e) => {
                record_transaction_status(STATUS_SUBMISSION_FAILED);
                return Err(TransactionExecutorError::RpcError(e));
            }
        };

        Ok(tx_hash)
    }

    /// Reserve receipt-monitoring capacity for a raw transaction.
    ///
    /// The reservation releases its slot when dropped, so an early return gives it back.
    fn reserve_submission(&self, raw_tx: &[u8]) -> Result<SubmissionReservation, TransactionExecutorError> {
        self.transaction_store
            .try_reserve_submission(
                raw_tx,
                self.config.max_submitted_transactions,
                self.config.max_submitted_transactions_per_identity,
            )
            .map_err(|exhausted| {
                let (limit, max) = match exhausted.limit {
                    SubmissionLimit::Global(max) => ("global", max),
                    SubmissionLimit::PerIdentity(max) => ("per_identity", max),
                };
                let summary = decode_transaction_summary(raw_tx);
                warn!(
                    limit,
                    max,
                    submitted = exhausted.submitted_total,
                    submitted_for_signer = exhausted.submitted_for_identity,
                    signer = ?exhausted.identity.map(Address::from),
                    tx_hash = ?summary.map(|tx| tx.transaction_hash),
                    nonce = ?summary.map(|tx| tx.nonce),
                    to = ?summary.and_then(|tx| tx.to),
                    max_fee_per_gas = ?summary.map(|tx| tx.max_fee_per_gas),
                    max_priority_fee_per_gas = ?summary.and_then(|tx| tx.max_priority_fee_per_gas),
                    "Rejecting raw transaction because submission capacity is exhausted"
                );
                TransactionExecutorError::OverloadedError
            })
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
        if let Err(e) = self.validator.validate_raw_transaction(&raw_tx) {
            warn!(error = %e, "Transaction validation failed");
            record_transaction_status(STATUS_VALIDATION_FAILED);
            return Err(e.into());
        }

        // The HOPR-aware policy runs before admission control: a deterministically invalid or
        // duplicated action should never consume monitoring capacity in the first place.
        let admitted = self.apply_hopr_policy(&raw_tx, SubmissionMode::Tracked).await?;

        // Admission control applies to this mode only: async submissions are the
        // only ones that occupy receipt-monitoring capacity. The slot is reserved
        // before broadcasting so concurrent submissions cannot overshoot the limits
        // while they wait on the RPC; returning early drops and releases it.
        //
        // A transaction that is already being monitored occupies its slot, so a repeat of it
        // takes no new one and must not be refused for capacity. Its hash is verified by the
        // node's answer below before it is treated as the same transaction.
        let reservation = if decode_transaction_summary(&raw_tx)
            .is_some_and(|summary| self.transaction_store.has_in_flight_hash(summary.transaction_hash))
        {
            None
        } else {
            Some(self.reserve_submission(&raw_tx)?)
        };

        // Submit to RPC first to get transaction hash
        let tx_hash = match self.rpc_client.send_raw_transaction(raw_tx.clone()).await {
            Ok(hash) => hash,
            Err(e) => {
                record_transaction_status(STATUS_SUBMISSION_FAILED);
                return Err(TransactionExecutorError::RpcError(e));
            }
        };

        let reservation = match reservation {
            Some(reservation) => reservation,
            None => {
                if let Some(existing) = self.transaction_store.resume_in_flight_hash(tx_hash) {
                    if let Some(action) = admitted {
                        action.register(existing);
                    }
                    return Err(TransactionExecutorError::AlreadyTracked {
                        existing,
                        diagnosis: None,
                    });
                }
                // The transaction concluded since the check, or the node answered with another
                // hash: this is a new submission like any other and needs capacity.
                self.reserve_submission(&raw_tx)?
            }
        };

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
            safe_execution: None,
            diagnosis: None,
            resume_generation: 0,
            last_resumed_at: None,
        };

        // The node may still hold a transaction we already track (an identical resubmission):
        // resuming that one rather than tracking the same hash twice is a single step per hash,
        // so concurrent identical submissions end up with one identity.
        match self.transaction_store.insert_or_resume(record, reservation) {
            Ok(TrackOutcome::Inserted) => {
                // Bind the logical action to this identity only once it is actually tracked, so
                // a failed submission never suppresses the retry that follows it.
                if let Some(action) = admitted {
                    action.register(id);
                }
                Ok(id)
            }
            Ok(TrackOutcome::Resumed { existing, diagnosis }) => {
                // The policy released the action when the original timed out, so it admitted
                // this retry afresh. The resumed transaction carries the action again: bind it,
                // or a re-signed copy at another nonce would be admitted and broadcast too.
                if let Some(action) = admitted {
                    action.register(existing);
                }
                Err(TransactionExecutorError::AlreadyTracked { existing, diagnosis })
            }
            Err(e) => {
                error!(id = %id, tx_hash = %tx_hash, error = %e, "Failed to store submitted transaction");
                Err(e.into())
            }
        }
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
        if let Err(e) = self.validator.validate_raw_transaction(&raw_tx) {
            warn!(error = %e, "Transaction validation failed");
            record_transaction_status(STATUS_VALIDATION_FAILED);
            return Err(e.into());
        }

        // Sync mode stores a terminal record rather than a tracked one, so deduplication has
        // no identity to return; preflight and invalid-action suppression still apply.
        self.apply_hopr_policy(&raw_tx, SubmissionMode::Untracked).await?;

        let confirmations = confirmations.unwrap_or(self.config.default_confirmations);
        let submitted_at = Utc::now();

        // Submit to RPC with confirmation wait
        let tx_hash = match self
            .rpc_client
            .send_raw_transaction_with_confirm(raw_tx.clone(), confirmations, Some(self.config.confirmation_timeout))
            .await
        {
            Ok(hash) => hash,
            Err(ConfirmationError::SubmissionFailed(e)) => {
                record_transaction_status(STATUS_SUBMISSION_FAILED);
                return Err(TransactionExecutorError::RpcError(e));
            }
            Err(ConfirmationError::Reverted(e)) => {
                record_transaction_status(STATUS_REVERTED);
                return Err(TransactionExecutorError::ExecutionFailed(e));
            }
            Err(ConfirmationError::Timeout(e)) => {
                record_transaction_status(STATUS_TIMEOUT);
                return Err(TransactionExecutorError::Timeout(e));
            }
        };

        // Attempt Safe execution enrichment if dependencies are available
        let mut record = TransactionRecord {
            id: Uuid::new_v4(),
            raw_transaction: raw_tx,
            transaction_hash: tx_hash,
            status: TransactionStatus::Confirmed,
            submitted_at,
            confirmed_at: Some(Utc::now()),
            error_message: None,
            safe_execution: None,
            diagnosis: None,
            resume_generation: 0,
            last_resumed_at: None,
        };

        if let (Some(receipt_provider), Some(safe_checker)) =
            (self.receipt_provider.as_ref(), self.safe_checker.as_ref())
        {
            record.safe_execution = enrich_safe_execution(
                &record,
                receipt_provider.as_ref(),
                safe_checker.as_ref(),
                self.config.enable_revert_reason_tracing,
            )
            .await;
        }

        if let Err(e) = self.transaction_store.insert(record.clone()) {
            error!(id = %record.id, tx_hash = %tx_hash, error = %e, "Failed to store confirmed transaction");
            return Err(e.into());
        }
        record_transaction_status(STATUS_CONFIRMED);
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

    use hopr_bindings::{
        exports::alloy::{
            consensus::{SignableTransaction, TxLegacy},
            eips::eip2718::Encodable2718,
            primitives::{Address as AlloyAddress, Bytes, TxKind, U256},
            signers::{Signer, local::PrivateKeySigner},
            sol_types::SolCall,
        },
        hopr_node_management_module::HoprNodeManagementModule,
        hopr_token::HoprToken,
    };

    use super::*;
    use crate::{
        hopr_action::HoprContracts,
        hopr_policy::{ChannelState, HoprChainState},
        transaction_store::UnminedReason,
    };

    /// Hash the mock node reports for `raw_tx`: distinct for distinct transactions.
    fn mock_tx_hash(raw_tx: &[u8]) -> Hash {
        // A decodable transaction has its real hash, as the node would report it.
        if let Some(summary) = decode_transaction_summary(raw_tx) {
            return summary.transaction_hash;
        }
        let mut hash = [0xABu8; 32];
        for (i, byte) in raw_tx.iter().enumerate() {
            hash[i % 32] ^= byte;
        }
        Hash::from(hash)
    }

    fn test_tx_hash() -> Hash {
        mock_tx_hash(&[0x01, 0x02, 0x03])
    }

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
        async fn send_raw_transaction(&self, raw_tx: Vec<u8>) -> Result<Hash, String> {
            *self.call_count.lock().unwrap() += 1;

            if self.should_fail {
                return Err("RPC error: transaction failed".to_string());
            }

            Ok(mock_tx_hash(&raw_tx))
        }

        async fn send_raw_transaction_with_confirm(
            &self,
            raw_tx: Vec<u8>,
            _confirmations: u64,
            _timeout: Option<Duration>,
        ) -> Result<Hash, ConfirmationError> {
            *self.call_count.lock().unwrap() += 1;

            if self.should_fail {
                return Err(ConfirmationError::SubmissionFailed("transaction failed".to_string()));
            }

            if self.should_timeout {
                return Err(ConfirmationError::Timeout(
                    "timed out waiting for confirmation".to_string(),
                ));
            }

            Ok(mock_tx_hash(&raw_tx))
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
        assert_eq!(result.unwrap(), test_tx_hash());
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
        assert_eq!(record.transaction_hash, test_tx_hash());
    }

    #[tokio::test]
    async fn test_async_resubmission_of_a_held_transaction_returns_the_tracked_one() {
        let executor = create_executor(MockRpcClient::new());

        let first = executor.send_raw_transaction_async(vec![0x01]).await.unwrap();
        let second = executor.send_raw_transaction_async(vec![0x01]).await;

        assert!(matches!(
            second,
            Err(TransactionExecutorError::AlreadyTracked { existing, .. }) if existing == first
        ));
        assert_eq!(executor.transaction_store().count(), 1);
    }

    #[tokio::test]
    async fn test_async_resubmission_of_a_timed_out_held_transaction_resumes_tracking() {
        let executor = create_executor(MockRpcClient::new());
        let store = executor.transaction_store();

        let first = executor.send_raw_transaction_async(vec![0x01]).await.unwrap();
        store
            .update_status_with_diagnosis(
                first,
                TransactionStatus::Timeout,
                None,
                Some(TransactionDiagnosis {
                    reason: UnminedReason::Queued,
                    signed_fees: None,
                    recommended_fees: None,
                    min_replacement_fees: None,
                    retry_after: Duration::from_secs(5),
                }),
            )
            .unwrap();

        let second = executor.send_raw_transaction_async(vec![0x01]).await;
        // The stored diagnosis is cleared on resumption, so the retry carries it instead.
        assert!(matches!(
            second,
            Err(TransactionExecutorError::AlreadyTracked { existing, diagnosis: Some(diagnosis) })
                if existing == first && diagnosis.reason == UnminedReason::Queued
        ));

        let resumed = store.get(first).unwrap();
        assert_eq!(resumed.status, TransactionStatus::Submitted);
        assert_eq!(resumed.diagnosis, None);
        assert_eq!(store.list_submitted_fair().len(), 1);

        store.update_status(first, TransactionStatus::Confirmed, None).unwrap();
        assert_eq!(store.get(first).unwrap().status, TransactionStatus::Confirmed);
    }

    #[tokio::test]
    async fn test_async_resubmission_of_a_dropped_transaction_is_tracked_afresh() {
        let executor = create_executor(MockRpcClient::new());

        let first = executor.send_raw_transaction_async(vec![0x01]).await.unwrap();
        executor
            .transaction_store()
            .update_status_with_diagnosis(
                first,
                TransactionStatus::Timeout,
                None,
                Some(TransactionDiagnosis {
                    reason: UnminedReason::Dropped,
                    signed_fees: None,
                    recommended_fees: None,
                    min_replacement_fees: None,
                    retry_after: Duration::from_secs(5),
                }),
            )
            .unwrap();

        let second = executor.send_raw_transaction_async(vec![0x01]).await.unwrap();

        assert_ne!(first, second);
        assert_eq!(executor.transaction_store().count(), 2);
    }

    /// Chain state that knows one module, so the HOPR policy applies to calls through it.
    struct ModuleChain;

    #[async_trait]
    impl HoprChainState for ModuleChain {
        async fn safe_for_module(&self, _module: Address) -> Result<Option<Address>, String> {
            Ok(Some(Address::from([0xDD; 20])))
        }

        async fn channel_status(
            &self,
            _source: Address,
            _destination: Address,
        ) -> Result<Option<ChannelState>, String> {
            Ok(None)
        }
    }

    /// A signed announcement through the node-management module, at the given nonce.
    async fn signed_announcement(signer: &PrivateKeySigner, nonce: u64) -> Vec<u8> {
        let announce = HoprToken::sendCall {
            recipient: AlloyAddress::from_slice(&[0x33; 20]),
            amount: U256::from(1_000u64),
            data: Bytes::from_static(b"key-binding-and-multiaddr"),
        }
        .abi_encode();
        let input = HoprNodeManagementModule::execTransactionFromModuleCall {
            to: AlloyAddress::from_slice(&[0x11; 20]),
            value: U256::ZERO,
            data: Bytes::from(announce),
            operation: 0,
        }
        .abi_encode();
        let tx = TxLegacy {
            chain_id: Some(1),
            nonce,
            gas_price: 1_000_000_000,
            gas_limit: 200_000,
            to: TxKind::Call(AlloyAddress::from_slice(&[0xAA; 20])),
            value: U256::ZERO,
            input: input.into(),
        };
        let signature = signer.sign_hash(&tx.signature_hash()).await.unwrap();
        let mut encoded = Vec::new();
        tx.into_signed(signature).encode_2718(&mut encoded);
        encoded
    }

    #[tokio::test]
    async fn test_resumed_transaction_carries_the_action_admitted_for_its_retry() {
        let store = Arc::new(TransactionStore::new());
        let policy = Arc::new(HoprPolicy::new(
            Arc::new(ModuleChain),
            store.clone(),
            HoprPolicyConfig {
                enabled: true,
                ..Default::default()
            },
            HoprContracts {
                token: Address::from([0x11; 20]),
                channels: Address::from([0xCC; 20]),
                announcements: Address::from([0x33; 20]),
            },
        ));
        let executor = RawTransactionExecutor::with_shared_dependencies(
            Arc::new(MockRpcClient::new()),
            store.clone(),
            Arc::new(TransactionValidator::new()),
            RawTransactionExecutorConfig::default(),
        )
        .with_hopr_policy(policy);
        let signer = PrivateKeySigner::random();
        let original = signed_announcement(&signer, 0).await;
        let resigned = signed_announcement(&signer, 1).await;

        let first = executor.send_raw_transaction_async(original.clone()).await.unwrap();
        store
            .update_status_with_diagnosis(
                first,
                TransactionStatus::Timeout,
                None,
                Some(TransactionDiagnosis {
                    reason: UnminedReason::Queued,
                    signed_fees: None,
                    recommended_fees: None,
                    min_replacement_fees: None,
                    retry_after: Duration::from_secs(5),
                }),
            )
            .unwrap();

        // The identical retry resumes the original, which carries the announcement again.
        let retry = executor.send_raw_transaction_async(original).await;
        assert!(matches!(
            retry,
            Err(TransactionExecutorError::AlreadyTracked { existing, .. }) if existing == first
        ));

        // So the same announcement signed again at another nonce resolves to it too.
        let duplicate = executor.send_raw_transaction_async(resigned).await;
        assert!(matches!(
            duplicate,
            Err(TransactionExecutorError::DuplicateHoprAction { existing, .. }) if existing == first
        ));
        assert_eq!(store.count(), 1);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn test_concurrent_identical_async_submissions_share_one_identity_and_slot() {
        let executor = Arc::new(create_executor(MockRpcClient::new()));

        let tasks = (0..16)
            .map(|_| {
                let executor = executor.clone();
                tokio::spawn(async move { executor.send_raw_transaction_async(vec![0x01]).await })
            })
            .collect::<Vec<_>>();
        let mut created = Vec::new();
        let mut tracked = Vec::new();
        for task in tasks {
            match task.await.unwrap() {
                Ok(id) => created.push(id),
                Err(TransactionExecutorError::AlreadyTracked { existing, .. }) => tracked.push(existing),
                Err(other) => panic!("unexpected outcome: {other}"),
            }
        }

        assert_eq!(created.len(), 1);
        assert!(tracked.iter().all(|existing| *existing == created[0]));
        let store = executor.transaction_store();
        assert_eq!(store.count(), 1);
        assert_eq!(store.submitted_count(), 1);
    }

    #[tokio::test]
    async fn test_async_overload_is_rejected_before_broadcast() {
        let executor = RawTransactionExecutor::new(
            MockRpcClient::new(),
            TransactionStore::new(),
            TransactionValidator::new(),
            RawTransactionExecutorConfig {
                max_submitted_transactions: 1,
                ..Default::default()
            },
        );

        // The first submission fills the single monitoring slot.
        assert!(executor.send_raw_transaction_async(vec![0x01]).await.is_ok());

        let result = executor.send_raw_transaction_async(vec![0x02]).await;
        assert!(matches!(result, Err(TransactionExecutorError::OverloadedError)));
    }

    /// A signed transfer from `signer` at `nonce`: decodable, so its hash and signer are real.
    async fn signed_transfer(signer: &PrivateKeySigner, nonce: u64) -> Vec<u8> {
        let tx = TxLegacy {
            chain_id: Some(1),
            nonce,
            gas_price: 1_000_000_000,
            gas_limit: 21_000,
            to: TxKind::Call(AlloyAddress::from_slice(&[0x22; 20])),
            value: U256::ZERO,
            input: Bytes::new(),
        };
        let signature = signer.sign_hash(&tx.signature_hash()).await.unwrap();
        let mut encoded = Vec::new();
        tx.into_signed(signature).encode_2718(&mut encoded);
        encoded
    }

    fn timed_out_queued() -> TransactionDiagnosis {
        TransactionDiagnosis {
            reason: UnminedReason::Queued,
            signed_fees: None,
            recommended_fees: None,
            min_replacement_fees: None,
            retry_after: Duration::from_secs(5),
        }
    }

    #[tokio::test]
    async fn test_async_retry_of_an_in_flight_transaction_is_not_refused_at_the_global_limit() {
        let executor = RawTransactionExecutor::new(
            MockRpcClient::new(),
            TransactionStore::new(),
            TransactionValidator::new(),
            RawTransactionExecutorConfig {
                max_submitted_transactions: 1,
                max_submitted_transactions_per_identity: 0,
                ..Default::default()
            },
        );
        let signer = PrivateKeySigner::random();
        let raw_tx = signed_transfer(&signer, 0).await;

        let first = executor.send_raw_transaction_async(raw_tx.clone()).await.unwrap();
        let retry = executor.send_raw_transaction_async(raw_tx).await;

        assert!(matches!(
            retry,
            Err(TransactionExecutorError::AlreadyTracked { existing, .. }) if existing == first
        ));
        // Another transaction still finds the single slot taken.
        let other = signed_transfer(&signer, 1).await;
        assert!(matches!(
            executor.send_raw_transaction_async(other).await,
            Err(TransactionExecutorError::OverloadedError)
        ));
        assert_eq!(executor.transaction_store().submitted_count(), 1);
    }

    #[tokio::test]
    async fn test_async_retry_of_an_in_flight_transaction_is_not_refused_at_the_per_signer_limit() {
        let executor = RawTransactionExecutor::new(
            MockRpcClient::new(),
            TransactionStore::new(),
            TransactionValidator::new(),
            RawTransactionExecutorConfig {
                max_submitted_transactions: 0,
                max_submitted_transactions_per_identity: 1,
                ..Default::default()
            },
        );
        let signer = PrivateKeySigner::random();
        let raw_tx = signed_transfer(&signer, 0).await;

        let first = executor.send_raw_transaction_async(raw_tx.clone()).await.unwrap();

        assert!(matches!(
            executor.send_raw_transaction_async(raw_tx).await,
            Err(TransactionExecutorError::AlreadyTracked { existing, .. }) if existing == first
        ));
        // Another signer is unaffected, the same signer's other transaction is refused.
        let other_nonce = signed_transfer(&signer, 1).await;
        assert!(matches!(
            executor.send_raw_transaction_async(other_nonce).await,
            Err(TransactionExecutorError::OverloadedError)
        ));
    }

    #[tokio::test]
    async fn test_async_retry_of_a_timed_out_transaction_still_needs_capacity() {
        let executor = RawTransactionExecutor::new(
            MockRpcClient::new(),
            TransactionStore::new(),
            TransactionValidator::new(),
            RawTransactionExecutorConfig {
                max_submitted_transactions: 1,
                max_submitted_transactions_per_identity: 0,
                ..Default::default()
            },
        );
        let store = executor.transaction_store();
        let first_signer = PrivateKeySigner::random();
        let raw_tx = signed_transfer(&first_signer, 0).await;

        let first = executor.send_raw_transaction_async(raw_tx.clone()).await.unwrap();
        store
            .update_status_with_diagnosis(first, TransactionStatus::Timeout, None, Some(timed_out_queued()))
            .unwrap();
        // The freed slot is taken by another transaction.
        let second = executor
            .send_raw_transaction_async(signed_transfer(&PrivateKeySigner::random(), 0).await)
            .await
            .unwrap();

        assert!(matches!(
            executor.send_raw_transaction_async(raw_tx.clone()).await,
            Err(TransactionExecutorError::OverloadedError)
        ));
        assert_eq!(store.get(first).unwrap().status, TransactionStatus::Timeout);

        // Once the slot is free again the retry resumes the timed-out transaction.
        store.update_status(second, TransactionStatus::Confirmed, None).unwrap();
        assert!(matches!(
            executor.send_raw_transaction_async(raw_tx).await,
            Err(TransactionExecutorError::AlreadyTracked { existing, .. }) if existing == first
        ));
        assert_eq!(store.get(first).unwrap().status, TransactionStatus::Submitted);
        assert_eq!(store.submitted_count(), 1);
    }

    #[tokio::test]
    async fn test_async_rpc_failure_releases_reserved_capacity() {
        let executor = RawTransactionExecutor::new(
            MockRpcClient::with_failure(),
            TransactionStore::new(),
            TransactionValidator::new(),
            RawTransactionExecutorConfig {
                max_submitted_transactions: 1,
                ..Default::default()
            },
        );

        // Both attempts reach the RPC: the first failure must not keep holding the only slot.
        for raw_tx in [vec![0x01], vec![0x02]] {
            let result = executor.send_raw_transaction_async(raw_tx).await;
            assert!(matches!(result, Err(TransactionExecutorError::RpcError(_))));
        }
        assert_eq!(executor.transaction_store().submitted_count(), 0);
    }

    #[tokio::test]
    async fn test_async_zero_limits_mean_unbounded() {
        let executor = RawTransactionExecutor::new(
            MockRpcClient::new(),
            TransactionStore::new(),
            TransactionValidator::new(),
            RawTransactionExecutorConfig {
                max_submitted_transactions: 0,
                max_submitted_transactions_per_identity: 0,
                ..Default::default()
            },
        );

        for i in 0..8u8 {
            assert!(executor.send_raw_transaction_async(vec![i]).await.is_ok());
        }
    }

    #[tokio::test]
    async fn test_async_per_identity_limit_is_rejected_before_broadcast() {
        let executor = RawTransactionExecutor::new(
            MockRpcClient::new(),
            TransactionStore::new(),
            TransactionValidator::new(),
            RawTransactionExecutorConfig {
                max_submitted_transactions: 0,
                max_submitted_transactions_per_identity: 1,
                ..Default::default()
            },
        );

        // Both envelopes are undecodable, so they share the same identity bucket.
        assert!(executor.send_raw_transaction_async(vec![0x01]).await.is_ok());

        let result = executor.send_raw_transaction_async(vec![0x02]).await;
        assert!(matches!(result, Err(TransactionExecutorError::OverloadedError)));
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
        assert_eq!(record.transaction_hash, test_tx_hash());
        assert!(record.confirmed_at.is_some());
    }

    #[tokio::test]
    async fn test_sync_mode_handles_timeout() {
        let executor = create_executor(MockRpcClient::with_timeout());
        let raw_tx = vec![0x01, 0x02, 0x03];

        let result = executor.send_raw_transaction_sync(raw_tx, None).await;
        assert!(matches!(result, Err(TransactionExecutorError::Timeout(_))));

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
