//! Post-finality verification of indexed logs.
//!
//! A background task re-checks indexed blocks after the chain tip tracker reports them as final.
//! The task runs `eth_getLogs` again with the filter set of the indexer. It compares the result
//! with the logs in the logs database on block number, block hash, transaction hash and log index.
//! A difference means one of two things. The indexer stored logs from a chain that the canonical
//! chain does not confirm, or the canonical chain has logs that the indexer never stored.
//!
//! A backend can return an empty or partial `eth_getLogs` response without an error. For this
//! reason, the task counts a stored log as wrong only when the canonical hash of its block differs
//! from the stored block hash, and skips the round when the backend does not have the block. It
//! also re-checks a range that does not match on the next tip update. A mismatch is
//! corroborated when both checks report the same difference. The task counts a corroborated
//! mismatch in `blokli_fcr_false_confirmations_total`, persists it in
//! `chain_info.verification_failed_at` and records it in the shared [`VerificationStatus`] flag.
//! The flag is sticky. The indexer stops advancing and readiness reports not ready until an
//! operator restores or rebuilds the index. A failed RPC or database read is not a mismatch. The
//! task tries the same range again on the next tip update.
//!
//! When the daemon starts with an existing index, the in-memory cursor starts
//! [`STARTUP_LOOKBACK_BLOCKS`] blocks behind the committed watermark. One range fetch then
//! verifies the blocks that the indexer stored shortly before a clean restart. These blocks can
//! become final while the daemon is down.
//!
//! The set of Safe contract addresses grows at runtime. The stored logs of a block contain only
//! the addresses that the indexer knew when it indexed that block. Thus the task compares stored
//! Safe logs strictly with the canonical chain (`missing_from_chain` and `changed`). It checks
//! `missing_from_store` only for the static contract addresses.

use std::{
    collections::{BTreeMap, BTreeSet},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

use async_trait::async_trait;
use blokli_chain_rpc::{
    HoprIndexerRpcOperations,
    errors::{Result as RpcResult, RpcError},
    tip::ChainTip,
};
use blokli_db::{
    BlokliDbGeneralModelOperations, api::logs::BlokliDbLogOperations, info::BlokliDbInfoOperations,
    safe_contracts::BlokliDbSafeContractOperations, safe_history::BlokliDbSafeHistoryOperations,
};
use hopr_bindings::exports::alloy::rpc::types::Filter;
#[cfg(all(feature = "telemetry", not(test)))]
use hopr_types::telemetry::SimpleCounter;
use hopr_types::{
    crypto::types::Hash,
    primitive::prelude::{Address, SerializableLog, ToHex},
};
use serde::Serialize;
use tokio::sync::watch;
use tracing::{debug, error, info, warn};

use crate::{
    IndexerState,
    block::{LogFilterPhase, generate_log_filters},
    errors::Result,
    traits::ChainLogHandler,
};

/// Maximum number of example entries that the report of a mismatch contains.
const MAX_MISMATCH_EXAMPLES: usize = 10;

/// Maximum number of blocks that one round verifies.
///
/// The task verifies a larger backlog over several tip updates. Thus one round never fetches an
/// unbounded amount of logs from the RPC endpoint.
const MAX_VERIFICATION_RANGE: u64 = 1000;

/// Number of blocks behind the startup watermark that the task re-checks for an existing index.
///
/// This covers blocks that the indexer stored shortly before a clean restart and that became final
/// while the daemon was down. With this lookback, the daemon does not persist a moving verification
/// cursor on every epoch.
const STARTUP_LOOKBACK_BLOCKS: u64 = 256;

/// Minimal RPC interface that post-finality verification needs.
#[async_trait]
pub(crate) trait CanonicalLogFetcher: Send + Sync {
    /// Fetches canonical logs matching `filters` in the inclusive block range `[from_block, to_block]`.
    ///
    /// # Errors
    ///
    /// Returns an error when the RPC request fails.
    async fn get_logs_for_block_range(
        &self,
        filters: Vec<Filter>,
        from_block: u64,
        to_block: u64,
    ) -> RpcResult<Vec<SerializableLog>>;

    /// Returns the canonical hash of the block at `block`, or `None` if the endpoint does not have the block.
    ///
    /// # Errors
    ///
    /// Returns an error when the RPC request fails.
    async fn block_hash(&self, block: u64) -> RpcResult<Option<Hash>>;
}

#[async_trait]
impl<T: HoprIndexerRpcOperations + Send + Sync> CanonicalLogFetcher for T {
    async fn get_logs_for_block_range(
        &self,
        filters: Vec<Filter>,
        from_block: u64,
        to_block: u64,
    ) -> RpcResult<Vec<SerializableLog>> {
        HoprIndexerRpcOperations::get_logs_for_block_range(self, filters, from_block, to_block).await
    }

    async fn block_hash(&self, block: u64) -> RpcResult<Option<Hash>> {
        HoprIndexerRpcOperations::block_hash(self, block).await
    }
}

/// Shared sticky flag that records whether post-finality verification found a mismatch.
///
/// [`IndexerState`] owns the flag. Thus the indexer, the verification task and the API readiness
/// checker all read the same value.
#[derive(Debug, Clone, Default)]
pub struct VerificationStatus {
    failed: Arc<AtomicBool>,
}

impl VerificationStatus {
    /// Returns whether a verified range differed from the canonical logs.
    pub fn failed(&self) -> bool {
        self.failed.load(Ordering::SeqCst)
    }

    /// Records that a verified range differed from the canonical logs.
    ///
    /// The flag never clears in this process. Startup sets it again from a durable verification
    /// halt.
    pub fn mark_failed(&self) {
        self.failed.store(true, Ordering::SeqCst);
    }
}

/// Identity of a log in the chain.
///
/// Verification compares logs on this identity only. The position in the chain and the hashes
/// that pin the log to a block and a transaction fix the event payload. Both hashes are
/// hexadecimal strings, so that a report can show a mismatch directly.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LogIdentity {
    /// Block that contains the log.
    pub block_number: u64,
    /// Hash of that block.
    pub block_hash: String,
    /// Hash of the transaction that emitted the log.
    pub tx_hash: String,
    /// Index of the log in its block.
    pub log_index: u64,
}

impl LogIdentity {
    fn from_log(log: &SerializableLog) -> Self {
        Self {
            block_number: log.block_number,
            block_hash: Hash::from(log.block_hash).to_hex(),
            tx_hash: Hash::from(log.tx_hash).to_hex(),
            log_index: log.log_index,
        }
    }
}

/// A log whose position holds a different log on the canonical chain.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ChangedLog {
    /// Identity of the log that the indexer stored.
    pub stored: LogIdentity,
    /// Identity of the log at the same position on the canonical chain.
    pub canonical: LogIdentity,
}

/// Differences between the stored logs and the canonical logs of one verified range.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct VerificationMismatch {
    /// Logs that are stored but that the canonical chain does not contain.
    pub missing_from_chain: Vec<LogIdentity>,
    /// Canonical logs that the store does not contain.
    pub missing_from_store: Vec<LogIdentity>,
    /// Positions that hold a different block or transaction hash on the canonical chain.
    pub changed: Vec<ChangedLog>,
}

impl VerificationMismatch {
    /// Returns whether the stored and the canonical logs agree.
    pub fn is_empty(&self) -> bool {
        self.missing_from_chain.is_empty() && self.missing_from_store.is_empty() && self.changed.is_empty()
    }

    /// Returns the total number of differences.
    pub fn len(&self) -> usize {
        self.missing_from_chain.len() + self.missing_from_store.len() + self.changed.len()
    }
}

/// Returns the position of a log in the chain.
fn log_position(log: &SerializableLog) -> (u64, u64) {
    (log.block_number, log.log_index)
}

/// Compares the stored logs of a range with the canonical logs of the same range.
///
/// The function matches logs by their position in the chain: a block number and a log index. Two
/// logs at the same position differ when their block hashes or their transaction hashes differ. A
/// reorganization causes this type of difference. The function does not compare the event
/// payload, because a matching position and block hash fix the payload.
///
/// `static_addresses` are the contract addresses that the indexer filters from the start of
/// indexing. The function reports canonical logs that are missing from the store only for these
/// addresses. Thus Safe contracts that the indexer discovers at runtime never cause a false halt
/// for a missing log. The function still checks all stored logs, Safe logs included, for
/// `missing_from_chain` and `changed`.
pub fn compare_logs(
    stored: &[SerializableLog],
    canonical: &[SerializableLog],
    static_addresses: &BTreeSet<Address>,
) -> VerificationMismatch {
    let stored_by_position = stored
        .iter()
        .map(|log| (log_position(log), log))
        .collect::<BTreeMap<_, _>>();
    let canonical_by_position = canonical
        .iter()
        .map(|log| (log_position(log), log))
        .collect::<BTreeMap<_, _>>();

    let mut mismatch = VerificationMismatch::default();

    for (position, stored_log) in &stored_by_position {
        match canonical_by_position.get(position) {
            None => mismatch.missing_from_chain.push(LogIdentity::from_log(stored_log)),
            Some(canonical_log)
                if canonical_log.block_hash != stored_log.block_hash || canonical_log.tx_hash != stored_log.tx_hash =>
            {
                mismatch.changed.push(ChangedLog {
                    stored: LogIdentity::from_log(stored_log),
                    canonical: LogIdentity::from_log(canonical_log),
                })
            }
            Some(_) => {}
        }
    }

    for (position, canonical_log) in &canonical_by_position {
        if !stored_by_position.contains_key(position) && static_addresses.contains(&canonical_log.address) {
            mismatch.missing_from_store.push(LogIdentity::from_log(canonical_log));
        }
    }

    mismatch
}

/// Plan for one post-finality verification round.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CursorStep {
    /// Nothing to verify. The finalized block is unknown or did not move past the cursor.
    Idle,
    /// Verify the inclusive range. If the range matches, advance the cursor to `to`.
    Verify {
        /// First block of the range.
        from: u64,
        /// Last block of the range.
        to: u64,
    },
}

/// Chooses the start block of the in-memory cursor after a finalized block is known.
///
/// For an existing index (`last_indexed_at_startup > 0`), the cursor starts
/// [`STARTUP_LOOKBACK_BLOCKS`] blocks behind the committed watermark. The start is at most
/// `finalized` and at least `floor`. One range fetch then verifies the blocks that the indexer
/// stored right before a restart and that became final while the daemon was down.
///
/// For an empty index (`last_indexed_at_startup == 0`), the cursor starts at
/// `finalized.max(floor)` and does not walk older history.
///
/// Returns `None` while the finalized block is unknown.
pub fn initial_cursor(last_indexed_at_startup: u64, finalized: Option<u64>, floor: u64) -> Option<u64> {
    finalized.map(|finalized| {
        if last_indexed_at_startup > 0 {
            last_indexed_at_startup
                .saturating_sub(STARTUP_LOOKBACK_BLOCKS)
                .min(finalized)
                .max(floor)
        } else {
            finalized.max(floor)
        }
    })
}

/// Returns the differences that both comparisons report.
///
/// The task fetches two comparisons of the same range a few seconds apart. A backend that sends a
/// partial result causes differences that the other comparison does not show. Thus only a
/// difference that both comparisons report counts. The order of the result follows `first`.
pub fn corroborated_mismatch(first: &VerificationMismatch, second: &VerificationMismatch) -> VerificationMismatch {
    VerificationMismatch {
        missing_from_chain: first
            .missing_from_chain
            .iter()
            .filter(|entry| second.missing_from_chain.contains(*entry))
            .cloned()
            .collect(),
        missing_from_store: first
            .missing_from_store
            .iter()
            .filter(|entry| second.missing_from_store.contains(*entry))
            .cloned()
            .collect(),
        changed: first
            .changed
            .iter()
            .filter(|entry| second.changed.contains(*entry))
            .cloned()
            .collect(),
    }
}

/// Plans the next post-finality verification round for a cursor that is already set.
///
/// The cursor advances only over blocks that the indexer already committed. `last_indexed` is the
/// upper limit of the range. Thus a finalized tag that moves faster than the indexer never asks
/// for logs that the indexer did not store yet. A round covers at most [`MAX_VERIFICATION_RANGE`]
/// blocks. The caller advances the cursor only after the range matched. Thus a finalized block
/// that moves backwards never moves the cursor backwards.
pub fn next_cursor_step(cursor: u64, finalized: Option<u64>, last_indexed: u64) -> CursorStep {
    let Some(finalized) = finalized else {
        return CursorStep::Idle;
    };

    let upper = finalized.min(last_indexed);
    if upper > cursor {
        let from = cursor.saturating_add(1);
        CursorStep::Verify {
            from,
            to: upper.min(from.saturating_add(MAX_VERIFICATION_RANGE - 1)),
        }
    } else {
        CursorStep::Idle
    }
}

/// Formats one log of a mismatch as an example entry, with `kind` as the label.
fn describe_log(log: &LogIdentity, kind: &str) -> String {
    format!(
        "{kind}: block {} log {} block hash {} tx hash {}",
        log.block_number, log.log_index, log.block_hash, log.tx_hash,
    )
}

/// Summarizes the differences of a mismatch in at most [`MAX_MISMATCH_EXAMPLES`] entries.
fn describe_mismatch(mismatch: &VerificationMismatch) -> Vec<String> {
    let missing_from_chain = mismatch
        .missing_from_chain
        .iter()
        .map(|log| describe_log(log, "missing from chain"));
    let missing_from_store = mismatch
        .missing_from_store
        .iter()
        .map(|log| describe_log(log, "missing from store"));
    let changed = mismatch.changed.iter().map(|change| {
        format!(
            "changed: block {} log {}, stored block hash {} tx hash {}, canonical block hash {} tx hash {}",
            change.stored.block_number,
            change.stored.log_index,
            change.stored.block_hash,
            change.stored.tx_hash,
            change.canonical.block_hash,
            change.canonical.tx_hash,
        )
    });

    missing_from_chain
        .chain(missing_from_store)
        .chain(changed)
        .take(MAX_MISMATCH_EXAMPLES)
        .collect()
}

/// Logs a failed verification attempt and increases the failure streak.
///
/// The first failure of a streak logs a warning. Later failures of the same streak log at debug
/// level.
fn record_failure(failure_streak: &mut u32, range: Option<(u64, u64)>, error: &str) {
    if *failure_streak == 0 {
        warn!(
            ?range,
            error, "post-finality verification failed, retrying the same range on the next tip update"
        );
    } else {
        debug!(
            ?range,
            error, "post-finality verification failed again, retrying the same range on the next tip update"
        );
    }

    *failure_streak = failure_streak.saturating_add(1);
}

#[cfg(all(feature = "telemetry", not(test)))]
lazy_static::lazy_static! {
    static ref METRIC_FCR_FALSE_CONFIRMATIONS_TOTAL: Option<SimpleCounter> =
        SimpleCounter::new(
            "blokli_fcr_false_confirmations_total",
            "Verified block ranges whose stored logs differ from the canonical logs after finality",
        ).inspect_err(|error| error!(%error, metric = "blokli_fcr_false_confirmations_total", "failed to register telemetry metric")).ok();
}

/// Counts one corroborated mismatch in `blokli_fcr_false_confirmations_total`.
///
/// Without telemetry, or when the metric is not registered, the function does nothing.
fn record_false_confirmation() {
    #[cfg(all(feature = "telemetry", not(test)))]
    if let Some(metric) = METRIC_FCR_FALSE_CONFIRMATIONS_TOTAL.as_ref() {
        metric.increment();
    }
}

/// Fetches the canonical logs of a range again and compares them with the stored logs.
///
/// # Errors
///
/// Returns an error when the filter set, the RPC request or the database read fails.
async fn verify_range<T, U, Db>(rpc: &T, db: &Db, logs_handler: &U, from: u64, to: u64) -> Result<VerificationMismatch>
where
    T: CanonicalLogFetcher,
    U: ChainLogHandler + Send + Sync + 'static,
    Db: BlokliDbGeneralModelOperations
        + BlokliDbInfoOperations
        + BlokliDbLogOperations
        + BlokliDbSafeContractOperations
        + BlokliDbSafeHistoryOperations
        + Clone
        + Send
        + Sync
        + 'static,
{
    let filters = generate_log_filters(db, logs_handler, LogFilterPhase::Continuous).await?;

    // The canonical fetch uses all filters that are known now. The filter set only grows, so this
    // is a superset of the filters that the block was indexed with. The indexer fetches token
    // approvals but drops them before storage. Thus the comparison uses the non-token set.
    let canonical = rpc.get_logs_for_block_range(filters.no_token, from, to).await?;

    // Read blocks `from..=to`. The offset is the number of blocks stored after `from`.
    let stored = db.get_logs(Some(from), Some(to - from)).await?;

    // Only the static contracts from startup count for logs that are missing from the store.
    // Stored logs of Safe contracts that the indexer discovered at runtime still get the strict
    // check.
    let token = logs_handler.contract_addresses_map().token;
    let static_addresses = logs_handler
        .contract_addresses()
        .into_iter()
        .filter(|address| *address != token)
        .collect::<BTreeSet<_>>();

    let mut mismatch = compare_logs(&stored, &canonical, &static_addresses);
    if mismatch.missing_from_chain.is_empty() && mismatch.changed.is_empty() {
        return Ok(mismatch);
    }

    // A lagging or load-balanced backend can return an empty or partial `eth_getLogs` result
    // without an error. If the backend does not have the range yet, skip the round. Otherwise
    // count a stored log as wrong only when the canonical hash of its block differs from the
    // stored block hash.
    if rpc.block_hash(to).await?.is_none() {
        return Err(RpcError::Other(format!("block {to} is not available on the endpoint")).into());
    }
    let blocks = mismatch
        .missing_from_chain
        .iter()
        .chain(mismatch.changed.iter().map(|changed| &changed.stored))
        .map(|entry| entry.block_number)
        .collect::<BTreeSet<_>>();
    let mut canonical_hashes = BTreeMap::new();
    for block in blocks {
        let hash = rpc
            .block_hash(block)
            .await?
            .ok_or_else(|| RpcError::Other(format!("block {block} is not available on the endpoint")))?;
        canonical_hashes.insert(block, hash.to_hex());
    }
    let block_differs = |entry: &LogIdentity| {
        canonical_hashes
            .get(&entry.block_number)
            .is_some_and(|hash| *hash != entry.block_hash)
    };
    mismatch.missing_from_chain.retain(|entry| block_differs(entry));
    mismatch.changed.retain(|changed| block_differs(&changed.stored));

    Ok(mismatch)
}

/// Inputs of the post-finality verification task that stay the same while it runs.
pub(crate) struct VerificationContext<U, Db> {
    /// Database that holds the indexed logs and the halt marker.
    pub(crate) db: Db,
    /// Log handler that supplies the contract addresses and topics for the log filters.
    pub(crate) logs_handler: Arc<U>,
    /// Indexer state that receives a corroborated mismatch.
    pub(crate) indexer_state: IndexerState,
    /// Committed watermark before live indexing starts.
    pub(crate) last_indexed_at_startup: u64,
    /// Block below the configured start block.
    pub(crate) floor: u64,
    /// Publishes the number of completed rounds.
    ///
    /// Tests use it to wait for a round instead of sleeping. Production passes `None`.
    pub(crate) round_updates: Option<watch::Sender<u32>>,
}

/// Verifies indexed ranges after the chain tip tracker reports them as final.
///
/// The task runs until the tip publisher stops, the indexer shuts down, or the task finds a
/// corroborated mismatch.
pub(crate) async fn run_verification<T, U, Db>(
    mut tip_updates: watch::Receiver<Option<ChainTip>>,
    rpc: &T,
    context: VerificationContext<U, Db>,
) where
    T: CanonicalLogFetcher,
    U: ChainLogHandler + Send + Sync + 'static,
    Db: BlokliDbGeneralModelOperations
        + BlokliDbInfoOperations
        + BlokliDbLogOperations
        + BlokliDbSafeContractOperations
        + BlokliDbSafeHistoryOperations
        + Clone
        + Send
        + Sync
        + 'static,
{
    let VerificationContext {
        db,
        logs_handler,
        indexer_state,
        last_indexed_at_startup,
        floor,
        round_updates,
    } = context;

    let mut cursor: Option<u64> = None;
    let mut pending_mismatch: Option<((u64, u64), VerificationMismatch)> = None;
    let mut failure_streak: u32 = 0;
    let mut rounds: u32 = 0;

    loop {
        // Read the committed block before the finalized block of this round. A block that the
        // indexer commits while the task plans this round then falls into the next round. The task
        // does not compare it with a canonical set that it fetched before the block existed.
        let last_indexed = match db.get_indexer_state_info(None).await {
            Ok(info) => Some(u64::from(info.latest_block_number)),
            Err(error) => {
                record_failure(&mut failure_streak, None, &error.to_string());
                None
            }
        };

        if let Some(last_indexed) = last_indexed {
            let finalized = tip_updates.borrow().and_then(|tip| tip.finalized);

            if cursor.is_none() {
                if let Some(start) = initial_cursor(last_indexed_at_startup, finalized, floor) {
                    cursor = Some(start);
                    info!(
                        verified_up_to = start,
                        "post-finality verification started, blocks above the cursor are re-checked once they become \
                         final"
                    );
                }
            }

            let range = if let Some((pending_range, _)) = &pending_mismatch {
                Some(*pending_range)
            } else {
                cursor.and_then(|current| match next_cursor_step(current, finalized, last_indexed) {
                    CursorStep::Idle => None,
                    CursorStep::Verify { from, to } => Some((from, to)),
                })
            };

            if let Some((from, to)) = range {
                match verify_range(rpc, &db, logs_handler.as_ref(), from, to).await {
                    Ok(mismatch) if mismatch.is_empty() => {
                        if pending_mismatch.take().is_some() {
                            debug!(from_block = from, to_block = to, "the re-check matched the stored logs");
                        }
                        cursor = Some(to);
                        failure_streak = 0;
                        debug!(
                            from_block = from,
                            to_block = to,
                            "post-finality verification matched the stored logs"
                        );
                    }
                    Ok(mismatch) => match pending_mismatch.take() {
                        None => {
                            // A load-balanced or lagging backend can return an empty or partial
                            // response without an error. Keep the cursor and re-check the same
                            // range on the next tip update.
                            warn!(
                                from_block = from,
                                to_block = to,
                                missing_from_chain = mismatch.missing_from_chain.len(),
                                missing_from_store = mismatch.missing_from_store.len(),
                                changed = mismatch.changed.len(),
                                examples = ?describe_mismatch(&mismatch),
                                "post-finality verification found logs that the canonical chain does not confirm, \
                                 re-checking on the next tip update"
                            );
                            pending_mismatch = Some(((from, to), mismatch));
                        }
                        Some((_, previous)) => {
                            let confirmed = corroborated_mismatch(&previous, &mismatch);
                            if confirmed.is_empty() {
                                record_failure(
                                    &mut failure_streak,
                                    Some((from, to)),
                                    "the two comparisons of the range report different differences",
                                );
                            } else {
                                error!(
                                    from_block = from,
                                    to_block = to,
                                    missing_from_chain = confirmed.missing_from_chain.len(),
                                    missing_from_store = confirmed.missing_from_store.len(),
                                    changed = confirmed.changed.len(),
                                    examples = ?describe_mismatch(&confirmed),
                                    "post-finality verification found logs that the canonical chain does not confirm, \
                                     halting the indexer"
                                );
                                if let Err(error) = db.set_verification_failed_at(Some(from)).await {
                                    error!(
                                        from_block = from,
                                        to_block = to,
                                        %error,
                                        "failed to persist post-finality verification halt marker"
                                    );
                                }
                                record_false_confirmation();
                                indexer_state.mark_verification_failed();
                                return;
                            }
                        }
                    },
                    Err(error) => {
                        pending_mismatch = None;
                        record_failure(&mut failure_streak, Some((from, to)), &error.to_string());
                    }
                }
            }
        }

        rounds = rounds.saturating_add(1);
        if let Some(round_updates) = &round_updates {
            round_updates.send_replace(rounds);
        }

        if tip_updates.changed().await.is_err() {
            debug!("chain tip publisher stopped, ending post-finality verification");
            return;
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{
        collections::BTreeSet,
        sync::{
            Arc,
            atomic::{AtomicUsize, Ordering as AtomicOrdering},
        },
        time::Duration,
    };

    use async_trait::async_trait;
    use blokli_chain_rpc::{
        errors::{Result as RpcResult, RpcError},
        tip::{ChainTip, FcrState, HeadSource},
    };
    use blokli_chain_types::ContractAddresses;
    use blokli_db::{
        api::logs::BlokliDbLogOperations, db::BlokliDb, info::BlokliDbInfoOperations,
        safe_contracts::BlokliDbSafeContractOperations,
    };
    use hopr_bindings::exports::alloy::rpc::types::Filter;
    use hopr_types::{
        crypto::types::Hash,
        primitive::prelude::{Address, SerializableLog},
    };
    use mockall::mock;
    use tokio::{sync::watch, task::JoinHandle, time::timeout};

    use super::{
        CanonicalLogFetcher, ChangedLog, CursorStep, LogIdentity, STARTUP_LOOKBACK_BLOCKS, VerificationContext,
        VerificationMismatch, compare_logs, corroborated_mismatch, initial_cursor, next_cursor_step, run_verification,
    };
    use crate::{IndexerState, traits::MockChainLogHandler};

    fn static_addr() -> Address {
        Address::from([0x11; 20])
    }

    fn safe_addr() -> Address {
        Address::from([0x33; 20])
    }

    fn sample_log(block_number: u64, log_index: u64, block_hash: u8, tx_hash: u8) -> SerializableLog {
        SerializableLog {
            address: static_addr(),
            topics: vec![[0x22; 32]],
            data: vec![1, 2, 3],
            tx_index: 0,
            block_number,
            block_hash: [block_hash; 32],
            tx_hash: [tx_hash; 32],
            log_index,
            ..Default::default()
        }
    }

    fn sample_safe_log(block_number: u64) -> SerializableLog {
        SerializableLog {
            address: safe_addr(),
            ..sample_log(block_number, 1, 3, 4)
        }
    }

    fn static_set() -> BTreeSet<Address> {
        BTreeSet::from([static_addr()])
    }

    #[test]
    fn compare_logs_identical_empty_and_safe_rules() {
        let static_addrs = static_set();
        let logs = vec![sample_log(10, 0, 1, 2), sample_log(10, 1, 3, 4)];
        assert!(compare_logs(&logs, &logs, &static_addrs).is_empty());
        assert!(compare_logs(&[], &[], &static_addrs).is_empty());

        // A canonical Safe log that is missing from the store does not count.
        let stored = vec![sample_log(20, 0, 1, 2)];
        let canonical = vec![sample_log(20, 0, 1, 2), sample_safe_log(20)];
        assert!(compare_logs(&stored, &canonical, &static_addrs).is_empty());

        // Stored Safe logs still get the strict check for missing_from_chain and changed.
        let stored_safes = vec![sample_safe_log(20), sample_safe_log(21)];
        let mismatch = compare_logs(&stored_safes, &[sample_safe_log(20)], &static_addrs);
        assert_eq!(mismatch.missing_from_chain.len(), 1);
        assert_eq!(mismatch.missing_from_chain[0].block_number, 21);

        let changed_safe = SerializableLog {
            block_hash: [0x99; 32],
            ..sample_safe_log(20)
        };
        let mismatch = compare_logs(&[sample_safe_log(20)], &[changed_safe], &static_addrs);
        assert_eq!(mismatch.changed.len(), 1);
    }

    #[test]
    fn changed_block_hash_is_reported() {
        let mismatch = compare_logs(&[sample_log(10, 0, 1, 2)], &[sample_log(10, 0, 9, 2)], &static_set());
        insta::assert_yaml_snapshot!("changed_block_hash", mismatch);
    }

    #[test]
    fn changed_transaction_hash_is_reported() {
        let mismatch = compare_logs(&[sample_log(10, 0, 1, 2)], &[sample_log(10, 0, 1, 9)], &static_set());
        insta::assert_yaml_snapshot!("changed_transaction_hash", mismatch);
    }

    #[test]
    fn stored_log_missing_from_chain_is_reported() {
        let mismatch = compare_logs(
            &[sample_log(10, 0, 1, 2), sample_log(11, 0, 3, 4)],
            &[sample_log(10, 0, 1, 2)],
            &static_set(),
        );
        insta::assert_yaml_snapshot!("stored_log_missing_from_chain", mismatch);
    }

    #[test]
    fn canonical_log_missing_from_store_is_reported() {
        let mismatch = compare_logs(
            &[sample_log(10, 0, 1, 2)],
            &[sample_log(10, 0, 1, 2), sample_log(11, 0, 3, 4)],
            &static_set(),
        );
        insta::assert_yaml_snapshot!("canonical_log_missing_from_store", mismatch);
    }

    #[test]
    fn initial_cursor_and_next_cursor_step_cases() {
        let lookback = STARTUP_LOOKBACK_BLOCKS;
        for (last_indexed, finalized, floor, expected) in [
            (0, None, 0, None),
            (500, None, 0, None),
            (0, Some(42), 0, Some(42)),
            (0, Some(50), 99, Some(99)),
            (500, Some(600), 0, Some(500 - lookback)),
            (500, Some(100), 0, Some(100)),
            (100, Some(200), 50, Some(50)),
            (500, Some(600), 400, Some(400)),
        ] {
            assert_eq!(initial_cursor(last_indexed, finalized, floor), expected);
        }

        for (cursor, finalized, last_indexed, expected) in [
            (10, None, 100, CursorStep::Idle),
            (10, Some(10), 100, CursorStep::Idle),
            (10, Some(9), 100, CursorStep::Idle),
            (10, Some(20), 10, CursorStep::Idle),
            (10, Some(15), 100, CursorStep::Verify { from: 11, to: 15 }),
            (10, Some(20), 15, CursorStep::Verify { from: 11, to: 15 }),
            (10, Some(5_000), 5_000, CursorStep::Verify { from: 11, to: 1_010 }),
            (1_010, Some(5_000), 5_000, CursorStep::Verify { from: 1_011, to: 2_010 }),
        ] {
            assert_eq!(next_cursor_step(cursor, finalized, last_indexed), expected);
        }
    }

    #[test]
    fn corroborated_mismatch_keeps_only_shared_entries() {
        let id = |block: u64, log_index: u64| LogIdentity {
            block_number: block,
            block_hash: format!("0x{block:02x}"),
            tx_hash: "0x02".to_string(),
            log_index,
        };
        let disjoint = corroborated_mismatch(
            &VerificationMismatch {
                missing_from_chain: vec![id(1, 0)],
                ..Default::default()
            },
            &VerificationMismatch {
                missing_from_chain: vec![id(2, 0)],
                ..Default::default()
            },
        );
        assert!(disjoint.is_empty());

        let shared_change = ChangedLog {
            stored: id(3, 0),
            canonical: id(4, 0),
        };
        let first = VerificationMismatch {
            missing_from_chain: vec![id(1, 0)],
            changed: vec![shared_change.clone()],
            ..Default::default()
        };
        let second = VerificationMismatch {
            missing_from_chain: vec![id(1, 0)],
            missing_from_store: vec![id(5, 1)],
            changed: vec![shared_change.clone()],
        };
        assert_eq!(
            corroborated_mismatch(&first, &second),
            VerificationMismatch {
                missing_from_chain: vec![id(1, 0)],
                missing_from_store: Vec::new(),
                changed: vec![shared_change],
            }
        );
    }

    fn sample_tip(finalized: Option<u64>) -> ChainTip {
        ChainTip {
            latest: 200,
            safe: None,
            finalized,
            confirmed: 180,
            source: HeadSource::Polling,
            fcr: FcrState::Unavailable,
        }
    }

    mock! {
        CanonicalLogs {}

        #[async_trait]
        impl CanonicalLogFetcher for CanonicalLogs {
            async fn get_logs_for_block_range(
                &self,
                filters: Vec<Filter>,
                from_block: u64,
                to_block: u64,
            ) -> RpcResult<Vec<SerializableLog>>;

            async fn block_hash(&self, block: u64) -> RpcResult<Option<Hash>>;
        }
    }

    fn log_handler(contract_addresses: Vec<Address>) -> MockChainLogHandler {
        let mut handler = MockChainLogHandler::new();
        handler.expect_contract_addresses().return_const(contract_addresses);
        handler
            .expect_contract_addresses_map()
            .return_const(ContractAddresses::default());
        handler.expect_contract_address_topics().return_const(Vec::new());
        handler
    }

    /// Makes every block hash differ from the stored hashes, as after a reorganization.
    fn reorged_block_hashes(rpc: &mut MockCanonicalLogs) {
        rpc.expect_block_hash().returning(|_| Ok(Some(Hash::from([0xee; 32]))));
    }

    struct VerificationHarness {
        db: BlokliDb,
        state: IndexerState,
        tip_tx: watch::Sender<Option<ChainTip>>,
        round_rx: watch::Receiver<u32>,
        task: JoinHandle<()>,
    }

    impl VerificationHarness {
        async fn spawn(
            db: BlokliDb,
            rpc: MockCanonicalLogs,
            contracts: Vec<Address>,
            initial_finalized: Option<u64>,
            last_indexed_at_startup: u64,
            floor: u64,
        ) -> Self {
            let state = IndexerState::default();
            let (tip_tx, tip_rx) = watch::channel(Some(sample_tip(initial_finalized)));
            let (round_tx, round_rx) = watch::channel(0);
            let rpc = Arc::new(rpc);
            let task = tokio::spawn({
                let db = db.clone();
                let state = state.clone();
                async move {
                    run_verification(
                        tip_rx,
                        rpc.as_ref(),
                        VerificationContext {
                            db,
                            logs_handler: Arc::new(log_handler(contracts)),
                            indexer_state: state,
                            last_indexed_at_startup,
                            floor,
                            round_updates: Some(round_tx),
                        },
                    )
                    .await;
                }
            });
            Self {
                db,
                state,
                tip_tx,
                round_rx,
                task,
            }
        }

        async fn wait_round(&mut self, round: u32) {
            timeout(
                Duration::from_secs(1),
                self.round_rx.wait_for(|completed| *completed >= round),
            )
            .await
            .unwrap_or_else(|_| panic!("verification should reach round {round}"))
            .expect("round updates should stay connected");
        }

        fn send_finalized(&self, finalized: Option<u64>) {
            self.tip_tx.send_replace(Some(sample_tip(finalized)));
        }

        async fn finish(self) -> anyhow::Result<()> {
            drop(self.tip_tx);
            self.task.await?;
            Ok(())
        }
    }

    async fn prepare_db(last_indexed: u32, logs: Vec<SerializableLog>) -> anyhow::Result<BlokliDb> {
        let db = BlokliDb::new_in_memory().await?;
        db.set_indexer_state_info(None, last_indexed).await?;
        db.store_logs(logs).await?;
        Ok(db)
    }

    #[tokio::test]
    async fn clean_range_advances_cursor_without_rechecking() -> anyhow::Result<()> {
        let logs = vec![sample_log(11, 0, 1, 2), sample_log(12, 0, 3, 4)];
        let db = prepare_db(12, logs.clone()).await?;
        let calls = Arc::new(AtomicUsize::new(0));
        let counted = calls.clone();

        let mut rpc = MockCanonicalLogs::new();
        rpc.expect_get_logs_for_block_range()
            .withf(|_, from: &u64, to: &u64| *from == 11 && *to == 12)
            .returning(move |_, _, _| {
                counted.fetch_add(1, AtomicOrdering::SeqCst);
                Ok(logs.clone())
            });

        let mut h = VerificationHarness::spawn(db, rpc, Vec::new(), Some(10), 0, 0).await;
        h.wait_round(1).await;
        h.send_finalized(Some(12));
        h.wait_round(2).await;
        h.send_finalized(Some(12));
        h.wait_round(3).await;

        assert_eq!(calls.load(AtomicOrdering::SeqCst), 1);
        assert_eq!(h.db.verification_failed_at().await?, None);
        assert!(!h.state.verification_failed());
        h.finish().await
    }

    #[tokio::test]
    async fn corroborated_mismatch_persists_halt_on_second_tip() -> anyhow::Result<()> {
        let stored = vec![sample_log(11, 0, 1, 2), sample_log(12, 0, 3, 4)];
        let db = prepare_db(12, stored).await?;
        let calls = Arc::new(AtomicUsize::new(0));
        let counted = calls.clone();

        let mut rpc = MockCanonicalLogs::new();
        rpc.expect_get_logs_for_block_range()
            .withf(|_, from: &u64, to: &u64| *from == 11 && *to == 12)
            .returning(move |_, _, _| {
                counted.fetch_add(1, AtomicOrdering::SeqCst);
                Ok(vec![sample_log(11, 0, 1, 2)])
            });
        reorged_block_hashes(&mut rpc);

        let mut h = VerificationHarness::spawn(db.clone(), rpc, vec![static_addr()], Some(10), 0, 0).await;
        h.wait_round(1).await;

        // The first mismatch records a pending state and does not halt yet.
        h.send_finalized(Some(12));
        h.wait_round(2).await;
        assert_eq!(calls.load(AtomicOrdering::SeqCst), 1);
        assert!(!h.state.verification_failed());
        assert_eq!(db.verification_failed_at().await?, None);

        // The next tip update corroborates the same range. The task persists the halt marker and stops.
        h.send_finalized(Some(13));
        timeout(Duration::from_secs(1), h.task)
            .await
            .expect("task should stop after corroborated mismatch")?;

        assert_eq!(calls.load(AtomicOrdering::SeqCst), 2);
        assert!(h.state.verification_failed());
        assert_eq!(db.verification_failed_at().await?, Some(11));
        Ok(())
    }

    #[tokio::test]
    async fn transient_empty_and_conflicting_rechecks_do_not_halt() -> anyhow::Result<()> {
        let logs = vec![sample_log(11, 0, 1, 2), sample_log(12, 0, 3, 4)];

        // Case 1: the first response is empty for a short time. The re-check on the next tip update matches.
        let db = prepare_db(12, logs.clone()).await?;
        let calls = Arc::new(AtomicUsize::new(0));
        let counted = calls.clone();
        let expected = logs.clone();
        let mut rpc = MockCanonicalLogs::new();
        rpc.expect_get_logs_for_block_range().returning(move |_, _, _| {
            if counted.fetch_add(1, AtomicOrdering::SeqCst) == 0 {
                Ok(Vec::new())
            } else {
                Ok(expected.clone())
            }
        });
        reorged_block_hashes(&mut rpc);
        let mut h = VerificationHarness::spawn(db, rpc, Vec::new(), Some(10), 0, 0).await;
        h.wait_round(1).await;
        h.send_finalized(Some(12));
        h.wait_round(2).await;
        h.send_finalized(Some(12));
        h.wait_round(3).await;
        h.send_finalized(Some(12));
        h.wait_round(4).await;
        assert_eq!(calls.load(AtomicOrdering::SeqCst), 2);
        assert!(!h.state.verification_failed());
        assert_eq!(h.db.verification_failed_at().await?, None);
        h.finish().await?;

        // Case 2: two partial responses that do not agree cause a retry. The task does not halt and
        // does not write a halt marker.
        let db = prepare_db(12, logs.clone()).await?;
        let calls = Arc::new(AtomicUsize::new(0));
        let counted = calls.clone();
        let mut rpc = MockCanonicalLogs::new();
        rpc.expect_get_logs_for_block_range().returning(move |_, _, _| {
            match counted.fetch_add(1, AtomicOrdering::SeqCst) {
                0 => Ok(vec![sample_log(11, 0, 1, 2)]),
                1 => Ok(vec![sample_log(12, 0, 3, 4)]),
                _ => Ok(logs.clone()),
            }
        });
        reorged_block_hashes(&mut rpc);
        let mut h = VerificationHarness::spawn(db, rpc, Vec::new(), Some(10), 0, 0).await;
        h.wait_round(1).await;
        h.send_finalized(Some(12));
        h.wait_round(2).await;
        h.send_finalized(Some(12));
        h.wait_round(3).await;
        assert_eq!(calls.load(AtomicOrdering::SeqCst), 2);
        assert!(!h.state.verification_failed());
        assert_eq!(h.db.verification_failed_at().await?, None);

        h.send_finalized(Some(12));
        h.wait_round(4).await;
        h.send_finalized(Some(12));
        h.wait_round(5).await;
        assert_eq!(calls.load(AtomicOrdering::SeqCst), 3);
        assert!(!h.state.verification_failed());
        h.finish().await
    }

    #[tokio::test]
    async fn rpc_error_and_safe_canonical_log_do_not_halt() -> anyhow::Result<()> {
        // After an RPC error, the task tries again on the next tip update.
        let logs = vec![sample_log(11, 0, 1, 2)];
        let db = prepare_db(11, logs.clone()).await?;
        let calls = Arc::new(AtomicUsize::new(0));
        let counted = calls.clone();
        let expected = logs.clone();
        let mut rpc = MockCanonicalLogs::new();
        rpc.expect_get_logs_for_block_range().returning(move |_, _, _| {
            if counted.fetch_add(1, AtomicOrdering::SeqCst) == 0 {
                Err(RpcError::Other("rpc is down".to_string()))
            } else {
                Ok(expected.clone())
            }
        });
        let mut h = VerificationHarness::spawn(db, rpc, Vec::new(), Some(10), 0, 0).await;
        h.wait_round(1).await;
        h.send_finalized(Some(11));
        h.wait_round(2).await;
        assert_eq!(calls.load(AtomicOrdering::SeqCst), 1);
        assert!(!h.state.verification_failed());
        h.send_finalized(Some(11));
        h.wait_round(3).await;
        assert_eq!(calls.load(AtomicOrdering::SeqCst), 2);
        assert!(!h.state.verification_failed());
        h.finish().await?;

        // A canonical Safe log that is missing from the store does not halt verification.
        let base_log = sample_log(11, 0, 1, 2);
        let db = prepare_db(11, vec![base_log.clone()]).await?;
        db.create_safe_contract(None, safe_addr(), Address::default(), Address::default(), 11, 0, 1)
            .await?;
        let canonical = vec![base_log, sample_safe_log(11)];
        let mut rpc = MockCanonicalLogs::new();
        rpc.expect_get_logs_for_block_range()
            .withf(|_, from: &u64, to: &u64| *from == 11 && *to == 11)
            .returning(move |_, _, _| Ok(canonical.clone()));
        let mut h = VerificationHarness::spawn(db, rpc, vec![static_addr()], Some(10), 0, 0).await;
        h.wait_round(1).await;
        h.send_finalized(Some(11));
        h.wait_round(2).await;
        assert!(!h.state.verification_failed());
        h.finish().await
    }

    #[tokio::test]
    async fn empty_logs_from_lagging_backend_do_not_halt() -> anyhow::Result<()> {
        let logs = vec![sample_log(11, 0, 1, 2), sample_log(12, 0, 3, 4)];

        // Case 1: the backend returns an empty log result twice but has the blocks with the stored
        // hashes. The stored logs are canonical, so the range matches and the cursor advances.
        let db = prepare_db(12, logs.clone()).await?;
        let mut rpc = MockCanonicalLogs::new();
        rpc.expect_get_logs_for_block_range()
            .times(1)
            .returning(|_, _, _| Ok(Vec::new()));
        rpc.expect_block_hash()
            .returning(|block| Ok(Some(Hash::from(if block == 11 { [1; 32] } else { [3; 32] }))));
        let mut h = VerificationHarness::spawn(db, rpc, vec![static_addr()], Some(10), 0, 0).await;
        h.wait_round(1).await;
        h.send_finalized(Some(12));
        h.wait_round(2).await;
        h.send_finalized(Some(12));
        h.wait_round(3).await;
        assert!(!h.state.verification_failed());
        assert_eq!(h.db.verification_failed_at().await?, None);
        h.finish().await?;

        // Case 2: the backend returns an empty log result twice and does not have the blocks yet.
        // Each round is skipped and the task does not halt.
        let db = prepare_db(12, logs).await?;
        let mut rpc = MockCanonicalLogs::new();
        rpc.expect_get_logs_for_block_range()
            .returning(|_, _, _| Ok(Vec::new()));
        rpc.expect_block_hash().returning(|_| Ok(None));
        let mut h = VerificationHarness::spawn(db, rpc, vec![static_addr()], Some(10), 0, 0).await;
        h.wait_round(1).await;
        for round in 2..=4 {
            h.send_finalized(Some(12));
            h.wait_round(round).await;
        }
        assert!(!h.state.verification_failed());
        assert_eq!(h.db.verification_failed_at().await?, None);
        h.finish().await
    }

    #[tokio::test]
    async fn startup_lookback_and_empty_index_initialization() -> anyhow::Result<()> {
        // For an existing index, the task verifies the last STARTUP_LOOKBACK_BLOCKS blocks in one fetch.
        let expected_from = 300 - STARTUP_LOOKBACK_BLOCKS + 1;
        let logs = vec![sample_log(expected_from, 0, 1, 2), sample_log(300, 0, 3, 4)];
        let db = prepare_db(300, logs.clone()).await?;
        let calls = Arc::new(AtomicUsize::new(0));
        let counted = calls.clone();
        let expected = logs.clone();
        let mut rpc = MockCanonicalLogs::new();
        rpc.expect_get_logs_for_block_range()
            .withf(move |_, from: &u64, to: &u64| *from == expected_from && *to == 300)
            .returning(move |_, _, _| {
                counted.fetch_add(1, AtomicOrdering::SeqCst);
                Ok(expected.clone())
            });
        let mut h = VerificationHarness::spawn(db, rpc, Vec::new(), Some(310), 300, 0).await;
        h.wait_round(1).await;
        assert_eq!(calls.load(AtomicOrdering::SeqCst), 1);
        assert!(!h.state.verification_failed());
        h.finish().await?;

        // On an empty index, the cursor starts at the first finalized block. The task does not fetch
        // older history.
        let db = prepare_db(0, Vec::new()).await?;
        let mut h = VerificationHarness::spawn(db, MockCanonicalLogs::new(), Vec::new(), None, 0, 0).await;
        h.wait_round(1).await;
        h.send_finalized(Some(50));
        h.wait_round(2).await;
        assert!(!h.state.verification_failed());
        h.finish().await?;

        // The range read returns only the logs inside [from, to].
        let db = prepare_db(
            13,
            vec![
                sample_log(10, 0, 1, 2),
                sample_log(11, 0, 3, 4),
                sample_log(12, 0, 5, 6),
                sample_log(13, 0, 7, 8),
            ],
        )
        .await?;
        let stored = db.get_logs(Some(11), Some(12 - 11)).await?;
        assert_eq!(
            stored.iter().map(|log| log.block_number).collect::<Vec<_>>(),
            vec![11, 12]
        );
        let canonical = vec![sample_log(11, 0, 3, 4), sample_log(12, 0, 5, 6)];
        assert!(compare_logs(&stored, &canonical, &static_set()).is_empty());
        let mismatch = compare_logs(&stored, &[sample_log(11, 0, 3, 4)], &static_set());
        assert_eq!(mismatch.missing_from_chain.len(), 1);
        assert_eq!(mismatch.missing_from_chain[0].block_number, 12);
        Ok(())
    }
}
