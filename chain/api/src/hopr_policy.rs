//! HOPR-aware transaction policy applied before a raw transaction is broadcast.
//!
//! Blokli's generic relay behaviour is unchanged for every transaction this module does not
//! recognise. For the supported HOPR node-management operations (see [`crate::hopr_action`])
//! it adds three things, in this order:
//!
//! 1. **Deterministic preflight.** Only conditions that are decidable from indexed chain state are checked. Note the
//!    qualifier: the index trails the chain head by the finality window, so a rejection is deterministic with respect
//!    to *indexed* state, not to the chain. A channel whose state changed within that window can be judged on stale
//!    data — the realistic case being a node that acts again within seconds of its own previous transaction. The rules
//!    mirror `HoprChannels` exactly and nothing more: funding is refused only while the channel is `PendingToClose`
//!    (funding a closed channel is how one is opened); closure initiation is refused only on a closed or absent channel
//!    (re-initiating on a closing channel legitimately extends the notice period); finalization is refused unless the
//!    channel is `PendingToClose`. Announcement is never refused, and Blokli deliberately performs no balance or
//!    allowance pre-check — those are racy against the chain and belong to the caller.
//! 2. **Logical-action deduplication.** Retries of the same action are re-signed with a new nonce, so raw-hash
//!    deduplication cannot see them. While an equivalent action is still being tracked, the existing transaction
//!    identity is returned instead of broadcasting a second copy.
//! 3. **Invalid-action suppression.** A signer that repeatedly submits deterministically invalid actions is put on a
//!    cooldown, so it cannot keep consuming broadcast and monitoring capacity. The cooldown is per signer and per
//!    operation, so one bad channel action does not suppress that node's unrelated submissions.
//!
//! The policy is advisory rather than transactional: evaluation and the subsequent broadcast
//! are not atomic, so two genuinely simultaneous submissions of the same action can both be
//! admitted. It bounds sustained retry storms, which is the failure this is designed against.

use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use async_trait::async_trait;
use blokli_db::BlokliDbAllOperations;
use dashmap::DashMap;
use hopr_types::{internal::channels::ChannelStatus, primitive::prelude::Address};
use tracing::{debug, warn};
use uuid::Uuid;

use crate::{
    hopr_action::{DecodedHoprAction, HoprContracts, HoprOperation, decode_hopr_action},
    transaction_store::{TransactionStatus, TransactionStore},
};

/// Why a supported HOPR action was refused before broadcast.
///
/// Every variant is decidable from indexed chain state alone, which is what makes refusing
/// the transaction safe: no reachable chain state would have made the call succeed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValidationReason {
    /// A channel operation referenced a channel that was never opened.
    ChannelNotFound,
    /// Closure finalization was attempted while the channel was not `PendingToClose`.
    ChannelNotPendingToClose,
    /// Funding was attempted while the channel was already closing.
    ChannelAlreadyClosing,
    /// A channel operation was attempted on a closed channel.
    ChannelClosed,
}

impl ValidationReason {
    /// Stable, low-cardinality code used for metric labels and client-visible error codes.
    pub fn code(self) -> &'static str {
        match self {
            ValidationReason::ChannelNotFound => "CHANNEL_NOT_FOUND",
            ValidationReason::ChannelNotPendingToClose => "CHANNEL_NOT_PENDING_TO_CLOSE",
            ValidationReason::ChannelAlreadyClosing => "CHANNEL_ALREADY_CLOSING",
            ValidationReason::ChannelClosed => "CHANNEL_CLOSED",
        }
    }

    /// Human-readable explanation, suitable for returning to the submitting client.
    pub fn message(self) -> &'static str {
        match self {
            ValidationReason::ChannelNotFound => "the channel this action refers to does not exist",
            ValidationReason::ChannelNotPendingToClose => {
                "an outgoing channel closure can only be finalized while the channel is pending to close"
            }
            ValidationReason::ChannelAlreadyClosing => "a channel that is closing cannot be funded",
            ValidationReason::ChannelClosed => "the channel is closed",
        }
    }
}

impl std::fmt::Display for ValidationReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.message())
    }
}

/// How the submission that is being evaluated will be tracked.
///
/// Deduplication needs an identity to hand back in place of a second broadcast, so it only
/// applies to the asynchronous mode — the one mode that leaves a tracked record behind.
/// Synchronous and fire-and-forget submissions are still preflighted and still throttled.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubmissionMode {
    /// Asynchronous submission: a transaction identity is stored and monitored.
    Tracked,
    /// Synchronous or fire-and-forget submission: no `Submitted` record is left behind.
    Untracked,
}

/// Outcome of evaluating a raw transaction against the HOPR-aware policy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PolicyDecision {
    /// The policy does not apply: either it is disabled, the calldata is not a supported HOPR
    /// operation, or the call target is not a known HOPR module. Generic behaviour applies.
    NotApplicable,
    /// The action passed preflight and may be broadcast.
    Admit(AdmittedAction),
    /// The action is deterministically invalid and must not be broadcast.
    Rejected {
        /// Operation label, as produced by [`HoprOperation::name`].
        operation: &'static str,
        /// Why the action was refused.
        reason: ValidationReason,
    },
    /// An equivalent logical action is already being tracked.
    Duplicate {
        /// Operation label, as produced by [`HoprOperation::name`].
        operation: &'static str,
        /// Identity of the transaction already tracking this action.
        existing: Uuid,
    },
    /// The signer is on cooldown after repeated deterministically invalid submissions.
    Throttled {
        /// Operation label, as produced by [`HoprOperation::name`].
        operation: &'static str,
        /// The reason of the most recent rejection that led to the cooldown.
        reason: ValidationReason,
        /// How long until this signer may submit this operation again.
        retry_after: Duration,
    },
}

/// A supported action that passed preflight, ready to be registered once broadcast succeeds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdmittedAction {
    /// Deduplication key identifying the logical action across re-signed retries.
    key: String,
    /// Operation label, as produced by [`HoprOperation::name`].
    pub operation: &'static str,
}

/// Read-only view of the chain state the policy needs.
///
/// Abstracted so the policy can be unit-tested without a database.
#[async_trait]
pub trait HoprChainState: Send + Sync {
    /// Resolve the Safe registered for a node-management module address.
    ///
    /// Returns `Ok(None)` when the address is not a known module, which is what gates the
    /// whole policy: an unknown module is not ours and keeps generic behaviour.
    async fn safe_for_module(&self, module: Address) -> Result<Option<Address>, String>;

    /// Current status of the channel from `source` to `destination`, if it exists.
    async fn channel_status(&self, source: Address, destination: Address) -> Result<Option<ChannelState>, String>;
}

/// Indexed status of a payment channel, reduced to what the preflight distinguishes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChannelState {
    /// The channel is open and can be funded or set to closing.
    Open,
    /// The channel closure has been initiated and may be finalized once the grace period ends.
    PendingToClose,
    /// The channel is closed.
    Closed,
}

/// Database-backed [`HoprChainState`].
pub struct DbHoprChainState<T> {
    db: T,
}

impl<T> DbHoprChainState<T> {
    /// Wrap a database handle as a chain-state reader.
    pub fn new(db: T) -> Self {
        Self { db }
    }
}

#[async_trait]
impl<T: BlokliDbAllOperations + Send + Sync> HoprChainState for DbHoprChainState<T> {
    async fn safe_for_module(&self, module: Address) -> Result<Option<Address>, String> {
        self.db
            .get_safe_contract_by_module_address(None, module)
            .await
            .map_err(|e| format!("get_safe_contract_by_module_address failed: {e}"))
            .and_then(|entry| {
                entry
                    .map(|entry| {
                        Address::try_from(entry.address.as_slice())
                            .map_err(|e| format!("invalid Safe address in safe contract entry: {e}"))
                    })
                    .transpose()
            })
    }

    async fn channel_status(&self, source: Address, destination: Address) -> Result<Option<ChannelState>, String> {
        let entry = self
            .db
            .get_channel_by_parties(None, &source, &destination)
            .await
            .map_err(|e| format!("get_channel_by_parties failed: {e}"))?;

        Ok(entry.map(|entry| match entry.status {
            ChannelStatus::Open => ChannelState::Open,
            ChannelStatus::PendingToClose(_) => ChannelState::PendingToClose,
            ChannelStatus::Closed => ChannelState::Closed,
        }))
    }
}

/// Tuning for the HOPR-aware policy.
#[derive(Debug, Clone)]
pub struct HoprPolicyConfig {
    /// Whether the policy is applied at all. When `false`, every transaction keeps generic
    /// behaviour and no database lookups are performed.
    pub enabled: bool,
    /// How long a logical action stays registered while its transaction is still tracked.
    ///
    /// This is an upper bound only: an entry whose transaction has already reached a terminal
    /// state is released as soon as it is looked up again, well before the TTL expires.
    pub action_ttl: Duration,
    /// Consecutive deterministic rejections of one operation by one signer before a cooldown
    /// is applied. `0` disables invalid-action suppression.
    pub invalid_action_threshold: u32,
    /// How long a signer is suppressed for an operation once the threshold is reached.
    pub invalid_action_cooldown: Duration,
    /// Upper bound on logical actions tracked for deduplication.
    ///
    /// Both tracking maps are keyed by values a client controls, so they need a ceiling that
    /// does not depend on clients behaving. Reaching it makes the policy stop *tracking*, not
    /// stop working: an untracked action is simply broadcast, which is the behaviour without
    /// the policy at all.
    pub max_tracked_actions: usize,
    /// Upper bound on `(signer, operation)` pairs tracked for invalid-action suppression.
    pub max_tracked_signers: usize,
    /// How long a signer's failure accounting survives without further submissions.
    pub failure_retention: Duration,
}

impl Default for HoprPolicyConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            action_ttl: Duration::from_secs(120),
            invalid_action_threshold: 3,
            invalid_action_cooldown: Duration::from_secs(60),
            max_tracked_actions: 10_000,
            max_tracked_signers: 10_000,
            failure_retention: Duration::from_secs(600),
        }
    }
}

/// A logical action currently being tracked.
#[derive(Debug, Clone, Copy)]
struct RegisteredAction {
    transaction: Uuid,
    registered_at: Instant,
}

/// Deterministic-failure accounting for one (signer, operation) pair.
#[derive(Debug, Clone, Copy)]
struct FailureState {
    consecutive: u32,
    reason: ValidationReason,
    suppressed_until: Option<Instant>,
    /// Last time this pair was seen, so an abandoned entry can be reclaimed.
    last_seen: Instant,
}

/// The HOPR-aware transaction policy.
///
/// Cloning is not supported; share it behind an `Arc`. All state is in-memory and per process,
/// matching the transaction store it complements.
pub struct HoprPolicy {
    chain: Arc<dyn HoprChainState>,
    /// Addresses the decoder gates on, so a foreign contract sharing a selector is never
    /// mistaken for a HOPR operation.
    contracts: HoprContracts,
    store: Arc<TransactionStore>,
    config: HoprPolicyConfig,
    /// Logical action key to the transaction currently carrying it.
    actions: DashMap<String, RegisteredAction>,
    /// (signer, operation) to its deterministic-failure accounting.
    failures: DashMap<(Address, &'static str), FailureState>,
}

impl std::fmt::Debug for HoprPolicy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HoprPolicy")
            .field("config", &self.config)
            .field("tracked_actions", &self.actions.len())
            .finish_non_exhaustive()
    }
}

impl HoprPolicy {
    /// Build a policy over the given chain-state reader and transaction store.
    pub fn new(
        chain: Arc<dyn HoprChainState>,
        store: Arc<TransactionStore>,
        config: HoprPolicyConfig,
        contracts: HoprContracts,
    ) -> Self {
        Self {
            chain,
            contracts,
            store,
            config,
            actions: DashMap::new(),
            failures: DashMap::new(),
        }
    }

    /// Evaluate a raw signed transaction before broadcasting it.
    ///
    /// A database error during preflight yields [`PolicyDecision::NotApplicable`]: the policy
    /// is an optional safety net and must never turn a transient lookup failure into a
    /// rejected submission.
    pub async fn evaluate(&self, raw_tx: &[u8], mode: SubmissionMode) -> PolicyDecision {
        if !self.config.enabled {
            return PolicyDecision::NotApplicable;
        }

        let Some(action) = decode_hopr_action(raw_tx, &self.contracts) else {
            return PolicyDecision::NotApplicable;
        };

        // Resolve the acting Safe. A module we do not know is not a HOPR module we manage, so
        // the transaction keeps generic behaviour.
        let source = match self.resolve_source(&action).await {
            Ok(Some(source)) => source,
            Ok(None) => return PolicyDecision::NotApplicable,
            Err(e) => {
                warn!(error = %e, "HOPR policy could not resolve the acting Safe; falling back to generic handling");
                return PolicyDecision::NotApplicable;
            }
        };

        let operation = action.operation.name();

        // An already-suppressed signer is refused before any further lookup.
        if let Some(retry_after) = self.suppression_for(action.signer, operation) {
            let reason = self
                .failures
                .get(&(action.signer, operation))
                .map(|state| state.reason)
                .unwrap_or(ValidationReason::ChannelNotFound);
            crate::metrics::record_hopr_validation(operation, "throttled");
            debug!(
                tx_hash = %action.transaction_hash, signer = %action.signer, operation,
                reason = reason.code(), retry_after_secs = retry_after.as_secs(),
                "suppressing HOPR action: this signer is on invalid-action cooldown"
            );
            return PolicyDecision::Throttled {
                operation,
                reason,
                retry_after,
            };
        }

        match self.preflight(&action, source).await {
            Ok(Some(reason)) => {
                self.record_failure(action.signer, operation, reason);
                crate::metrics::record_hopr_validation(operation, reason.code());
                debug!(
                    tx_hash = %action.transaction_hash, %source, signer = %action.signer,
                    operation, reason = reason.code(),
                    "rejecting HOPR action before broadcast"
                );
                return PolicyDecision::Rejected { operation, reason };
            }
            Ok(None) => self.clear_failures(action.signer, operation),
            Err(e) => {
                warn!(error = %e, operation, "HOPR policy preflight failed; falling back to generic handling");
                return PolicyDecision::NotApplicable;
            }
        }

        let key = action_key(&action, source);
        if mode == SubmissionMode::Tracked {
            if let Some(existing) = self.in_flight(&key) {
                crate::metrics::record_hopr_validation(operation, "deduplicated");
                debug!(
                    tx_hash = %action.transaction_hash, signer = %action.signer, operation, %existing,
                    "returning the in-flight transaction for an equivalent HOPR action"
                );
                return PolicyDecision::Duplicate { operation, existing };
            }
        }

        crate::metrics::record_hopr_validation(operation, "admitted");
        PolicyDecision::Admit(AdmittedAction { key, operation })
    }

    /// Bind an admitted action to the transaction that now carries it.
    ///
    /// Called once the submission has been accepted and given an identity. Until the
    /// transaction reaches a terminal state, or [`HoprPolicyConfig::action_ttl`] elapses,
    /// an equivalent action resolves to `transaction` instead of being broadcast again.
    pub fn register(&self, action: &AdmittedAction, transaction: Uuid) {
        if !self.reserve_action_slot() {
            warn!(
                tracked = self.actions.len(),
                "HOPR action tracking is at capacity; this action will not be deduplicated"
            );
            return;
        }

        self.actions.insert(
            action.key.clone(),
            RegisteredAction {
                transaction,
                registered_at: Instant::now(),
            },
        );
    }

    /// Resolve the channel source.
    ///
    /// A `*Safe` call names it outright and a direct plain call makes it the signer, so the
    /// module lookup is only reached by a module-routed plain call. That means the policy
    /// does not require a node to be indexed before it applies: what gates it is that the
    /// call targets one of Blokli's own known HOPR contracts and decodes as a supported
    /// operation.
    async fn resolve_source(&self, action: &DecodedHoprAction) -> Result<Option<Address>, String> {
        match action.explicit_source {
            Some(source) => Ok(Some(source)),
            None => self.chain.safe_for_module(action.target).await,
        }
    }

    /// Check the deterministic preconditions of an operation.
    ///
    /// `Ok(None)` means the action may proceed; `Ok(Some(reason))` that it is deterministically
    /// invalid. The rules mirror `HoprChannels._fundChannelInternal`,
    /// `_initiateOutgoingChannelClosureInternal` and `_finalizeOutgoingChannelClosureInternal`
    /// exactly, and nothing more — anything the contract accepts must be admitted here.
    ///
    /// Announcement is never refused: it carries no channel state, and Blokli performs no
    /// balance or allowance pre-check.
    async fn preflight(&self, action: &DecodedHoprAction, source: Address) -> Result<Option<ValidationReason>, String> {
        let destination = match &action.operation {
            HoprOperation::Announce { .. } => return Ok(None),
            HoprOperation::FundChannel { destination, .. }
            | HoprOperation::InitiateOutgoingChannelClosure { destination }
            | HoprOperation::FinalizeOutgoingChannelClosure { destination } => *destination,
        };

        // An absent channel is a zero-valued struct on chain, so the contract sees it as
        // `CLOSED`. The two are only distinguished here to give a clearer reason.
        let state = self.chain.channel_status(source, destination).await?;

        Ok(match (&action.operation, state) {
            // Funding opens or reopens a closed channel, so only an in-progress closure is
            // invalid: `_fundChannelInternal` reverts on PENDING_TO_CLOSE alone.
            (HoprOperation::FundChannel { .. }, Some(ChannelState::PendingToClose)) => {
                Some(ValidationReason::ChannelAlreadyClosing)
            }
            (HoprOperation::FundChannel { .. }, _) => None,

            // Initiating closure on a channel that is already closing is legitimate — it
            // extends the notice period — so only a closed or absent channel is invalid.
            (HoprOperation::InitiateOutgoingChannelClosure { .. }, None) => Some(ValidationReason::ChannelNotFound),
            (HoprOperation::InitiateOutgoingChannelClosure { .. }, Some(ChannelState::Closed)) => {
                Some(ValidationReason::ChannelClosed)
            }
            (HoprOperation::InitiateOutgoingChannelClosure { .. }, _) => None,

            // Finalization requires PENDING_TO_CLOSE and nothing else.
            (HoprOperation::FinalizeOutgoingChannelClosure { .. }, Some(ChannelState::PendingToClose)) => None,
            (HoprOperation::FinalizeOutgoingChannelClosure { .. }, None) => Some(ValidationReason::ChannelNotFound),
            (HoprOperation::FinalizeOutgoingChannelClosure { .. }, Some(ChannelState::Closed)) => {
                Some(ValidationReason::ChannelClosed)
            }
            (HoprOperation::FinalizeOutgoingChannelClosure { .. }, Some(ChannelState::Open)) => {
                Some(ValidationReason::ChannelNotPendingToClose)
            }

            (HoprOperation::Announce { .. }, _) => None,
        })
    }

    /// The transaction already carrying this logical action, if it is still in flight.
    ///
    /// An entry is released when its TTL elapses or its transaction has reached a terminal
    /// state, so a finished action never blocks the next legitimate submission.
    fn in_flight(&self, key: &str) -> Option<Uuid> {
        let registered = *self.actions.get(key)?;

        if registered.registered_at.elapsed() >= self.config.action_ttl {
            self.actions.remove(key);
            return None;
        }

        match self.store.get(registered.transaction) {
            Ok(record) if record.status == TransactionStatus::Submitted => Some(registered.transaction),
            // Terminal, or no longer known to the store: the action is concluded.
            _ => {
                self.actions.remove(key);
                None
            }
        }
    }

    /// Remaining cooldown for a (signer, operation) pair, if it is currently suppressed.
    fn suppression_for(&self, signer: Address, operation: &'static str) -> Option<Duration> {
        let mut entry = self.failures.get_mut(&(signer, operation))?;
        let until = entry.suppressed_until?;
        let remaining = until.checked_duration_since(Instant::now());
        match remaining {
            Some(remaining) if !remaining.is_zero() => Some(remaining),
            // The cooldown elapsed: give the signer a clean slate rather than letting a single
            // further rejection immediately re-suppress it.
            _ => {
                entry.suppressed_until = None;
                entry.consecutive = 0;
                None
            }
        }
    }

    /// Count a deterministic rejection, applying a cooldown once the threshold is reached.
    fn record_failure(&self, signer: Address, operation: &'static str, reason: ValidationReason) {
        if self.config.invalid_action_threshold == 0 {
            return;
        }

        if !self.failures.contains_key(&(signer, operation)) && !self.reserve_failure_slot() {
            warn!(
                tracked = self.failures.len(),
                "HOPR invalid-action tracking is at capacity; this signer will not be suppressed"
            );
            return;
        }

        let mut entry = self.failures.entry((signer, operation)).or_insert(FailureState {
            consecutive: 0,
            reason,
            suppressed_until: None,
            last_seen: Instant::now(),
        });
        entry.consecutive = entry.consecutive.saturating_add(1);
        entry.reason = reason;
        entry.last_seen = Instant::now();

        if entry.consecutive >= self.config.invalid_action_threshold {
            entry.suppressed_until = Some(Instant::now() + self.config.invalid_action_cooldown);
            warn!(
                %signer, operation, reason = reason.code(), failures = entry.consecutive,
                cooldown_secs = self.config.invalid_action_cooldown.as_secs(),
                "suppressing repeated deterministically invalid HOPR submissions"
            );
        }
    }

    /// Make room in the action map, reclaiming expired entries first.
    ///
    /// Returns `false` when the map is full of entries that are all still live, in which case
    /// the caller must skip tracking rather than grow without bound.
    ///
    /// The check and the insert that follows are not atomic, so concurrent registrations can
    /// overshoot the limit by the number of them in flight. The limit exists to stop
    /// unbounded growth, not to hold an exact count, and overshooting costs one map entry.
    fn reserve_action_slot(&self) -> bool {
        if self.actions.len() < self.config.max_tracked_actions {
            return true;
        }
        let ttl = self.config.action_ttl;
        self.actions.retain(|_, action| action.registered_at.elapsed() < ttl);
        self.actions.len() < self.config.max_tracked_actions
    }

    /// Make room in the failure map, reclaiming entries that have gone idle.
    fn reserve_failure_slot(&self) -> bool {
        if self.failures.len() < self.config.max_tracked_signers {
            return true;
        }
        let retention = self.config.failure_retention;
        self.failures.retain(|_, state| state.last_seen.elapsed() < retention);
        self.failures.len() < self.config.max_tracked_signers
    }

    /// Drop the failure accounting for a (signer, operation) pair after a valid submission.
    fn clear_failures(&self, signer: Address, operation: &'static str) {
        self.failures.remove(&(signer, operation));
    }
}

/// Build the deduplication key identifying a logical action across re-signed retries.
///
/// The key deliberately excludes the nonce, gas parameters and signature, which is exactly
/// what distinguishes it from raw-hash deduplication: a retry of the same intent produces the
/// same key even though it is a different transaction.
///
/// It covers the acting Safe as well as the signer because one chain key can own several
/// Safes. Keyed on the signer alone, the same action for two different Safes would collide
/// and the second would be reported as a duplicate of the first.
fn action_key(action: &DecodedHoprAction, source: Address) -> String {
    let signer = action.signer;
    match &action.operation {
        HoprOperation::Announce { payload_digest } => format!("announce:{signer}:{source}:{payload_digest}"),
        HoprOperation::FundChannel { destination, amount } => {
            format!("fund_channel:{signer}:{source}:{destination}:{amount}")
        }
        HoprOperation::InitiateOutgoingChannelClosure { destination } => {
            format!("initiate_channel_closure:{signer}:{source}:{destination}")
        }
        HoprOperation::FinalizeOutgoingChannelClosure { destination } => {
            format!("finalize_channel_closure:{signer}:{source}:{destination}")
        }
    }
}

#[cfg(test)]
mod tests {
    use chrono::Utc;
    use hopr_bindings::{
        exports::alloy::{
            consensus::{SignableTransaction, TxLegacy},
            eips::eip2718::Encodable2718,
            primitives::{Address as AlloyAddress, Bytes, TxKind, U256, aliases::U96},
            signers::{Signer, local::PrivateKeySigner},
            sol_types::SolCall,
        },
        hopr_channels::HoprChannels,
        hopr_node_management_module::HoprNodeManagementModule,
        hopr_token::HoprToken,
    };
    use hopr_types::crypto::types::Hash;

    use super::*;
    use crate::{hopr_action::HoprContracts, transaction_store::TransactionRecord};

    const MODULE: [u8; 20] = [0xAA; 20];
    const SAFE: [u8; 20] = [0xDD; 20];
    const DESTINATION: [u8; 20] = [0xBB; 20];
    const CHANNELS: [u8; 20] = [0xCC; 20];
    const TOKEN: [u8; 20] = [0x11; 20];
    const ANNOUNCEMENTS: [u8; 20] = [0x33; 20];

    fn contracts() -> HoprContracts {
        HoprContracts {
            token: Address::from(TOKEN),
            channels: Address::from(CHANNELS),
            announcements: Address::from(ANNOUNCEMENTS),
        }
    }

    /// Chain-state stub: a fixed module→Safe mapping and a fixed channel state.
    struct StubChain {
        safe: Option<Address>,
        channel: Option<ChannelState>,
        fail: bool,
    }

    impl StubChain {
        fn with_channel(channel: Option<ChannelState>) -> Arc<dyn HoprChainState> {
            Arc::new(StubChain {
                safe: Some(Address::from(SAFE)),
                channel,
                fail: false,
            })
        }

        fn unknown_module() -> Arc<dyn HoprChainState> {
            Arc::new(StubChain {
                safe: None,
                channel: None,
                fail: false,
            })
        }

        fn failing() -> Arc<dyn HoprChainState> {
            Arc::new(StubChain {
                safe: Some(Address::from(SAFE)),
                channel: None,
                fail: true,
            })
        }
    }

    #[async_trait]
    impl HoprChainState for StubChain {
        async fn safe_for_module(&self, _module: Address) -> Result<Option<Address>, String> {
            if self.fail {
                return Err("database unavailable".into());
            }
            Ok(self.safe)
        }

        async fn channel_status(
            &self,
            _source: Address,
            _destination: Address,
        ) -> Result<Option<ChannelState>, String> {
            if self.fail {
                return Err("database unavailable".into());
            }
            Ok(self.channel)
        }
    }

    async fn signed_module_call(signer: &PrivateKeySigner, inner: Vec<u8>) -> Vec<u8> {
        signed_module_call_to(signer, CHANNELS, inner, 0).await
    }

    /// Mirrors how `SafePayloadGenerator` frames every operation: through the module.
    async fn signed_module_call_to(signer: &PrivateKeySigner, target: [u8; 20], inner: Vec<u8>, nonce: u64) -> Vec<u8> {
        let input = HoprNodeManagementModule::execTransactionFromModuleCall {
            to: AlloyAddress::from_slice(&target),
            value: U256::ZERO,
            data: Bytes::from(inner),
            operation: 0,
        }
        .abi_encode();

        let tx = TxLegacy {
            chain_id: Some(1),
            nonce,
            gas_price: 1_000_000_000,
            gas_limit: 200_000,
            to: TxKind::Call(AlloyAddress::from_slice(&MODULE)),
            value: U256::ZERO,
            input: input.into(),
        };
        let signature = signer.sign_hash(&tx.signature_hash()).await.expect("signing failed");
        let mut encoded = Vec::new();
        tx.into_signed(signature).encode_2718(&mut encoded);
        encoded
    }

    fn finalize_closure() -> Vec<u8> {
        HoprChannels::finalizeOutgoingChannelClosureSafeCall {
            selfAddress: AlloyAddress::from_slice(&SAFE),
            destination: AlloyAddress::from_slice(&DESTINATION),
        }
        .abi_encode()
    }

    fn initiate_closure() -> Vec<u8> {
        HoprChannels::initiateOutgoingChannelClosureSafeCall {
            selfAddress: AlloyAddress::from_slice(&SAFE),
            destination: AlloyAddress::from_slice(&DESTINATION),
        }
        .abi_encode()
    }

    fn fund_channel(amount: u64) -> Vec<u8> {
        HoprChannels::fundChannelSafeCall {
            selfAddress: AlloyAddress::from_slice(&SAFE),
            account: AlloyAddress::from_slice(&DESTINATION),
            amount: U96::from(amount),
        }
        .abi_encode()
    }

    /// Mirrors the ERC777 announcement hook, which is how both payload generators announce.
    fn announce() -> Vec<u8> {
        HoprToken::sendCall {
            recipient: AlloyAddress::from_slice(&ANNOUNCEMENTS),
            amount: U256::from(1_000u64),
            data: Bytes::from_static(b"key-binding-and-multiaddr"),
        }
        .abi_encode()
    }

    fn policy(chain: Arc<dyn HoprChainState>, store: Arc<TransactionStore>) -> HoprPolicy {
        HoprPolicy::new(chain, store, HoprPolicyConfig::default(), contracts())
    }

    fn store() -> Arc<TransactionStore> {
        Arc::new(TransactionStore::new())
    }

    fn submitted_record(id: Uuid) -> TransactionRecord {
        TransactionRecord {
            id,
            raw_transaction: vec![0x01],
            transaction_hash: Hash::from([0xAB; 32]),
            status: TransactionStatus::Submitted,
            submitted_at: Utc::now(),
            confirmed_at: None,
            error_message: None,
            safe_execution: None,
        }
    }

    #[tokio::test]
    async fn finalization_is_rejected_unless_the_channel_is_pending_to_close() {
        let raw = signed_module_call(&PrivateKeySigner::random(), finalize_closure()).await;

        let open = policy(StubChain::with_channel(Some(ChannelState::Open)), store());
        assert_eq!(
            open.evaluate(&raw, SubmissionMode::Tracked).await,
            PolicyDecision::Rejected {
                operation: "finalize_channel_closure",
                reason: ValidationReason::ChannelNotPendingToClose,
            }
        );

        let missing = policy(StubChain::with_channel(None), store());
        assert_eq!(
            missing.evaluate(&raw, SubmissionMode::Tracked).await,
            PolicyDecision::Rejected {
                operation: "finalize_channel_closure",
                reason: ValidationReason::ChannelNotFound,
            }
        );

        let closed = policy(StubChain::with_channel(Some(ChannelState::Closed)), store());
        assert_eq!(
            closed.evaluate(&raw, SubmissionMode::Tracked).await,
            PolicyDecision::Rejected {
                operation: "finalize_channel_closure",
                reason: ValidationReason::ChannelClosed,
            }
        );
    }

    #[tokio::test]
    async fn finalization_is_admitted_while_the_channel_is_pending_to_close() {
        let raw = signed_module_call(&PrivateKeySigner::random(), finalize_closure()).await;
        let policy = policy(StubChain::with_channel(Some(ChannelState::PendingToClose)), store());

        assert!(matches!(
            policy.evaluate(&raw, SubmissionMode::Tracked).await,
            PolicyDecision::Admit(_)
        ));
    }

    #[tokio::test]
    async fn initiation_is_refused_only_on_a_closed_or_absent_channel() {
        let raw = signed_module_call(&PrivateKeySigner::random(), initiate_closure()).await;

        let open = policy(StubChain::with_channel(Some(ChannelState::Open)), store());
        assert!(matches!(
            open.evaluate(&raw, SubmissionMode::Tracked).await,
            PolicyDecision::Admit(_)
        ));

        // Re-initiating on a channel that is already closing is legitimate: the contract
        // treats it as extending the notice period, so Blokli must not refuse it.
        let closing = policy(StubChain::with_channel(Some(ChannelState::PendingToClose)), store());
        assert!(matches!(
            closing.evaluate(&raw, SubmissionMode::Tracked).await,
            PolicyDecision::Admit(_)
        ));

        let closed = policy(StubChain::with_channel(Some(ChannelState::Closed)), store());
        assert_eq!(
            closed.evaluate(&raw, SubmissionMode::Tracked).await,
            PolicyDecision::Rejected {
                operation: "initiate_channel_closure",
                reason: ValidationReason::ChannelClosed,
            }
        );

        let missing = policy(StubChain::with_channel(None), store());
        assert_eq!(
            missing.evaluate(&raw, SubmissionMode::Tracked).await,
            PolicyDecision::Rejected {
                operation: "initiate_channel_closure",
                reason: ValidationReason::ChannelNotFound,
            }
        );
    }

    #[tokio::test]
    async fn funding_is_refused_only_while_the_channel_is_closing() {
        let raw = signed_module_call(&PrivateKeySigner::random(), fund_channel(1_000)).await;

        let closing = policy(StubChain::with_channel(Some(ChannelState::PendingToClose)), store());
        assert_eq!(
            closing.evaluate(&raw, SubmissionMode::Tracked).await,
            PolicyDecision::Rejected {
                operation: "fund_channel",
                reason: ValidationReason::ChannelAlreadyClosing,
            }
        );

        // Funding a closed or absent channel is how a channel is opened or reopened, so both
        // must be admitted. Blokli performs no balance pre-check either.
        for state in [None, Some(ChannelState::Closed), Some(ChannelState::Open)] {
            let policy = policy(StubChain::with_channel(state), store());
            assert!(
                matches!(
                    policy.evaluate(&raw, SubmissionMode::Tracked).await,
                    PolicyDecision::Admit(_)
                ),
                "funding should be admitted for {state:?}"
            );
        }
    }

    #[tokio::test]
    async fn announcement_is_never_refused() {
        // The most hostile channel state must not matter: an announcement carries none, and
        // Blokli performs no balance or allowance pre-check.
        let raw = signed_module_call_to(&PrivateKeySigner::random(), TOKEN, announce(), 0).await;
        let policy = policy(StubChain::with_channel(None), store());

        assert!(matches!(
            policy.evaluate(&raw, SubmissionMode::Tracked).await,
            PolicyDecision::Admit(_)
        ));
    }

    #[tokio::test]
    async fn an_unknown_module_keeps_generic_behaviour() {
        // A module-routed *plain* call does not name its Safe, so the source can only come
        // from the module lookup. An unknown module means the policy cannot apply.
        let plain = HoprChannels::finalizeOutgoingChannelClosureCall {
            destination: AlloyAddress::from_slice(&DESTINATION),
        }
        .abi_encode();
        let raw = signed_module_call(&PrivateKeySigner::random(), plain).await;
        let policy = policy(StubChain::unknown_module(), store());

        assert_eq!(
            policy.evaluate(&raw, SubmissionMode::Tracked).await,
            PolicyDecision::NotApplicable
        );
    }

    #[tokio::test]
    async fn a_safe_naming_call_does_not_need_the_module_to_be_known() {
        // `*Safe` calls carry the acting Safe in the calldata, so the policy applies without
        // a module lookup. This is what keeps it working for a node blokli has not yet
        // indexed, and what lets it cover nodes that hold no Safe at all.
        let raw = signed_module_call(&PrivateKeySigner::random(), finalize_closure()).await;
        let policy = policy(StubChain::unknown_module(), store());

        assert_eq!(
            policy.evaluate(&raw, SubmissionMode::Tracked).await,
            PolicyDecision::Rejected {
                operation: "finalize_channel_closure",
                reason: ValidationReason::ChannelNotFound,
            }
        );
    }

    #[tokio::test]
    async fn a_database_failure_never_rejects_a_transaction() {
        let raw = signed_module_call(&PrivateKeySigner::random(), finalize_closure()).await;
        let policy = policy(StubChain::failing(), store());

        assert_eq!(
            policy.evaluate(&raw, SubmissionMode::Tracked).await,
            PolicyDecision::NotApplicable
        );
    }

    #[tokio::test]
    async fn a_re_signed_retry_resolves_to_the_tracked_transaction() {
        let signer = PrivateKeySigner::random();
        let store = store();
        let policy = policy(StubChain::with_channel(Some(ChannelState::Open)), store.clone());

        let first = signed_module_call(&signer, fund_channel(1_000)).await;
        let PolicyDecision::Admit(action) = policy.evaluate(&first, SubmissionMode::Tracked).await else {
            panic!("the first submission should be admitted");
        };

        let id = Uuid::new_v4();
        store.insert(submitted_record(id)).expect("insert failed");
        policy.register(&action, id);

        // A retry of the same intent is a different raw transaction: different nonce, hence a
        // different hash and a different signature. Only the logical key matches.
        let retry = {
            let input = HoprNodeManagementModule::execTransactionFromModuleCall {
                to: AlloyAddress::from_slice(&CHANNELS),
                value: U256::ZERO,
                data: Bytes::from(fund_channel(1_000)),
                operation: 0,
            }
            .abi_encode();
            let tx = TxLegacy {
                chain_id: Some(1),
                nonce: 7,
                gas_price: 2_000_000_000,
                gas_limit: 300_000,
                to: TxKind::Call(AlloyAddress::from_slice(&MODULE)),
                value: U256::ZERO,
                input: input.into(),
            };
            let signature = signer.sign_hash(&tx.signature_hash()).await.expect("signing failed");
            let mut encoded = Vec::new();
            tx.into_signed(signature).encode_2718(&mut encoded);
            encoded
        };
        assert_ne!(first, retry, "the retry must be a distinct raw transaction");

        assert_eq!(
            policy.evaluate(&retry, SubmissionMode::Tracked).await,
            PolicyDecision::Duplicate {
                operation: "fund_channel",
                existing: id,
            }
        );
    }

    #[tokio::test]
    async fn two_safes_under_one_chain_key_do_not_deduplicate_against_each_other() {
        let signer = PrivateKeySigner::random();
        let store = store();
        let policy = policy(StubChain::with_channel(Some(ChannelState::Open)), store.clone());

        // One chain key can own several Safes, so the same action for two of them is two
        // distinct intents even though the signer, counterparty and amount all match.
        let other_safe = [0xEE; 20];
        let for_first = signed_module_call(&signer, fund_channel(1_000)).await;
        let for_second = signed_module_call_to(
            &signer,
            CHANNELS,
            HoprChannels::fundChannelSafeCall {
                selfAddress: AlloyAddress::from_slice(&other_safe),
                account: AlloyAddress::from_slice(&DESTINATION),
                amount: U96::from(1_000u64),
            }
            .abi_encode(),
            1,
        )
        .await;

        let PolicyDecision::Admit(action) = policy.evaluate(&for_first, SubmissionMode::Tracked).await else {
            panic!("the first Safe's funding should be admitted");
        };
        let id = Uuid::new_v4();
        store.insert(submitted_record(id)).expect("insert failed");
        policy.register(&action, id);

        assert!(
            matches!(
                policy.evaluate(&for_second, SubmissionMode::Tracked).await,
                PolicyDecision::Admit(_)
            ),
            "a second Safe's funding must not be swallowed as a duplicate of the first"
        );
    }

    #[tokio::test]
    async fn a_concluded_action_stops_deduplicating() {
        let signer = PrivateKeySigner::random();
        let store = store();
        let policy = policy(StubChain::with_channel(Some(ChannelState::Open)), store.clone());

        let raw = signed_module_call(&signer, fund_channel(1_000)).await;
        let PolicyDecision::Admit(action) = policy.evaluate(&raw, SubmissionMode::Tracked).await else {
            panic!("should be admitted");
        };

        let id = Uuid::new_v4();
        store.insert(submitted_record(id)).expect("insert failed");
        policy.register(&action, id);

        store
            .update_status(id, TransactionStatus::Confirmed, None)
            .expect("status update failed");

        // The tracked transaction concluded, so the next submission of the same action is a
        // legitimate new attempt rather than a duplicate.
        assert!(matches!(
            policy.evaluate(&raw, SubmissionMode::Tracked).await,
            PolicyDecision::Admit(_)
        ));
    }

    #[tokio::test]
    async fn untracked_submissions_are_never_deduplicated() {
        let signer = PrivateKeySigner::random();
        let store = store();
        let policy = policy(StubChain::with_channel(Some(ChannelState::Open)), store.clone());

        let raw = signed_module_call(&signer, fund_channel(1_000)).await;
        let PolicyDecision::Admit(action) = policy.evaluate(&raw, SubmissionMode::Tracked).await else {
            panic!("should be admitted");
        };
        let id = Uuid::new_v4();
        store.insert(submitted_record(id)).expect("insert failed");
        policy.register(&action, id);

        // Sync and fire-and-forget leave no tracked record, so there is no identity to hand
        // back in place of a broadcast.
        assert!(matches!(
            policy.evaluate(&raw, SubmissionMode::Untracked).await,
            PolicyDecision::Admit(_)
        ));
    }

    #[tokio::test]
    async fn repeated_invalid_actions_are_suppressed_then_released() {
        let signer = PrivateKeySigner::random();
        let raw = signed_module_call(&signer, finalize_closure()).await;
        let config = HoprPolicyConfig {
            invalid_action_threshold: 2,
            invalid_action_cooldown: Duration::from_millis(80),
            ..Default::default()
        };
        let policy = HoprPolicy::new(
            StubChain::with_channel(Some(ChannelState::Open)),
            store(),
            config,
            contracts(),
        );

        // Below the threshold the rejection is reported on its own merits.
        assert!(matches!(
            policy.evaluate(&raw, SubmissionMode::Tracked).await,
            PolicyDecision::Rejected { .. }
        ));
        // The second rejection reaches the threshold and arms the cooldown.
        assert!(matches!(
            policy.evaluate(&raw, SubmissionMode::Tracked).await,
            PolicyDecision::Rejected { .. }
        ));
        assert!(matches!(
            policy.evaluate(&raw, SubmissionMode::Tracked).await,
            PolicyDecision::Throttled {
                operation: "finalize_channel_closure",
                reason: ValidationReason::ChannelNotPendingToClose,
                ..
            }
        ));

        tokio::time::sleep(Duration::from_millis(120)).await;

        // Once the cooldown elapses the signer gets a clean slate rather than being
        // re-suppressed by a single further rejection.
        assert!(matches!(
            policy.evaluate(&raw, SubmissionMode::Tracked).await,
            PolicyDecision::Rejected { .. }
        ));
    }

    #[tokio::test]
    async fn suppression_is_scoped_to_one_operation() {
        let signer = PrivateKeySigner::random();
        let config = HoprPolicyConfig {
            invalid_action_threshold: 1,
            ..Default::default()
        };
        let policy = HoprPolicy::new(
            StubChain::with_channel(Some(ChannelState::Open)),
            store(),
            config,
            contracts(),
        );

        let invalid = signed_module_call(&signer, finalize_closure()).await;
        assert!(matches!(
            policy.evaluate(&invalid, SubmissionMode::Tracked).await,
            PolicyDecision::Rejected { .. }
        ));
        assert!(matches!(
            policy.evaluate(&invalid, SubmissionMode::Tracked).await,
            PolicyDecision::Throttled { .. }
        ));

        // The same node's unrelated funding must still get through.
        let funding = signed_module_call(&signer, fund_channel(500)).await;
        assert!(matches!(
            policy.evaluate(&funding, SubmissionMode::Tracked).await,
            PolicyDecision::Admit(_)
        ));
    }

    #[tokio::test]
    async fn action_tracking_is_bounded_by_fresh_keys() {
        let store = store();
        let config = HoprPolicyConfig {
            max_tracked_actions: 4,
            ..Default::default()
        };
        let policy = HoprPolicy::new(
            StubChain::with_channel(Some(ChannelState::Open)),
            store.clone(),
            config,
            contracts(),
        );

        // Every submission is a different signer, so every action key is distinct and none of
        // them can be reclaimed by the TTL. Tracking must stop rather than grow.
        for nonce in 0..40u64 {
            let raw = signed_module_call_to(&PrivateKeySigner::random(), CHANNELS, fund_channel(1), nonce).await;
            if let PolicyDecision::Admit(action) = policy.evaluate(&raw, SubmissionMode::Tracked).await {
                let id = Uuid::new_v4();
                store.insert(submitted_record(id)).expect("insert failed");
                policy.register(&action, id);
            }
        }

        assert!(
            policy.actions.len() <= 4,
            "action tracking grew past its bound: {}",
            policy.actions.len()
        );
    }

    #[tokio::test]
    async fn suppression_tracking_is_bounded_by_fresh_keys() {
        let config = HoprPolicyConfig {
            max_tracked_signers: 4,
            ..Default::default()
        };
        // Finalizing against an open channel is always rejected, so each fresh signer creates
        // a failure entry. The map must not follow the attacker's key supply.
        let policy = HoprPolicy::new(
            StubChain::with_channel(Some(ChannelState::Open)),
            store(),
            config,
            contracts(),
        );

        for nonce in 0..40u64 {
            let raw = signed_module_call_to(&PrivateKeySigner::random(), CHANNELS, finalize_closure(), nonce).await;
            assert!(matches!(
                policy.evaluate(&raw, SubmissionMode::Tracked).await,
                PolicyDecision::Rejected { .. }
            ));
        }

        assert!(
            policy.failures.len() <= 4,
            "suppression tracking grew past its bound: {}",
            policy.failures.len()
        );
    }

    #[tokio::test]
    async fn a_disabled_policy_does_no_work_at_all() {
        let raw = signed_module_call(&PrivateKeySigner::random(), finalize_closure()).await;
        let config = HoprPolicyConfig {
            enabled: false,
            ..Default::default()
        };
        // A failing chain reader proves nothing was looked up.
        let policy = HoprPolicy::new(StubChain::failing(), store(), config, contracts());

        assert_eq!(
            policy.evaluate(&raw, SubmissionMode::Tracked).await,
            PolicyDecision::NotApplicable
        );
    }
}
