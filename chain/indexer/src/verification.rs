//! Post-finality verification of indexed logs.
//!
//! A background task re-checks indexed blocks once the chain tip tracker publishes that they are
//! final. It re-runs `eth_getLogs` with the same filter set the indexer uses and compares the
//! result with the logs stored in the logs database on block number, block hash, transaction hash
//! and log index. Any difference means that the logs were indexed from a chain that the canonical
//! chain does not confirm, or that the canonical chain has logs that were never indexed.
//!
//! A range that does not match is re-checked on the next tip update, because a backend can return
//! an empty or partial `eth_getLogs` response without an error. A corroborated mismatch is counted
//! in `blokli_fcr_false_confirmations_total`, persisted in `chain_info.verification_failed_at` and
//! recorded in the shared [`VerificationStatus`] flag. The flag is sticky: the indexer stops
//! advancing and readiness reports not ready until an operator restores or rebuilds the index. A
//! failing RPC or database read is not a mismatch: the same range is retried on the next tip
//! update.
//!
//! On startup with an existing index, the in-memory cursor starts [`STARTUP_LOOKBACK_BLOCKS`]
//! blocks behind the committed watermark so blocks indexed right before a clean restart that
//! finalized while the daemon was down are verified in one range fetch.
//!
//! The set of Safe contract addresses grows at runtime, while the stored logs of a block only
//! contain the addresses known when that block was indexed. Stored Safe logs are still compared
//! strictly against the canonical chain (`missing_from_chain` and `changed`), while
//! `missing_from_store` is checked only for the static contract addresses.

use std::{
    collections::{BTreeMap, BTreeSet},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

use async_trait::async_trait;
use blokli_chain_rpc::{HoprIndexerRpcOperations, errors::Result as RpcResult, tip::ChainTip};
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

/// Maximum number of example entries attached to the reported mismatch.
const MAX_MISMATCH_EXAMPLES: usize = 10;

/// Maximum number of blocks verified in one round.
///
/// A larger backlog is verified over several tip updates, so one round never fetches an unbounded
/// amount of logs from the RPC endpoint.
const MAX_VERIFICATION_RANGE: u64 = 1000;

/// Blocks behind the startup watermark re-checked when resuming an existing index.
///
/// Covers blocks indexed shortly before a clean restart that finalized while the daemon was down,
/// without persisting a moving verification cursor on every epoch.
const STARTUP_LOOKBACK_BLOCKS: u64 = 256;

/// Minimal RPC surface needed by post-finality verification.
#[async_trait]
pub(crate) trait CanonicalLogFetcher: Send + Sync {
    /// Fetches canonical logs matching `filters` in the inclusive block range `[from_block, to_block]`.
    async fn get_logs_for_block_range(
        &self,
        filters: Vec<Filter>,
        from_block: u64,
        to_block: u64,
    ) -> RpcResult<Vec<SerializableLog>>;
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
}

/// Shared, sticky flag recording whether post-finality verification found a mismatch.
///
/// It is owned by [`IndexerState`], so the indexer, the verification task and the API readiness
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
    /// The flag never clears in this process. Startup restores it from a durable verification halt.
    pub fn mark_failed(&self) {
        self.failed.store(true, Ordering::SeqCst);
    }
}

/// Identity of a log inside the chain.
///
/// Verification compares logs on this identity only: the event payload follows from the position in
/// the chain and the hashes that pin it to a block and a transaction. Both hashes are rendered as
/// hexadecimal strings so a mismatch can be reported and inspected directly.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LogIdentity {
    /// Block the log was emitted in.
    pub block_number: u64,
    /// Hash of that block.
    pub block_hash: String,
    /// Hash of the transaction that emitted the log.
    pub tx_hash: String,
    /// Index of the log within its block.
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
    /// Identity of the log as stored by the indexer.
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

/// Position of a log within the chain.
fn log_position(log: &SerializableLog) -> (u64, u64) {
    (log.block_number, log.log_index)
}

/// Compares the stored logs of a range with the canonical logs of the same range.
///
/// Logs are matched by their position within the chain, that is a block number and a log index.
/// Two logs at the same position differ when their block hash or their transaction hash differs,
/// which is what a reorganization produces. The event payload itself is not compared, because a
/// matching position and block hash pin down the payload.
///
/// `static_addresses` are the contract addresses filtered from the start of indexing: canonical
/// logs absent from the store are reported only for those addresses so runtime-discovered Safe
/// contracts never trigger false missing-log halts, while all stored logs (including Safe logs)
/// are still checked for `missing_from_chain` and `changed`.
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

/// Outcome of planning one post-finality verification round.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CursorStep {
    /// Nothing to verify: the finalized block is unknown or did not move past the cursor.
    Idle,
    /// Verify the inclusive range and advance the cursor to `to` once it matches.
    Verify {
        /// First block of the range.
        from: u64,
        /// Last block of the range.
        to: u64,
    },
}

/// Chooses the block at which the in-memory cursor starts once a finalized block is known.
///
/// When resuming an existing index (`last_indexed_at_startup > 0`), the cursor starts
/// [`STARTUP_LOOKBACK_BLOCKS`] blocks behind the committed watermark (capped at `finalized`, and
/// raised to at least `floor`), so blocks indexed right before a restart that finalized while the
/// daemon was down are verified in one range fetch. On an empty index (`last_indexed_at_startup ==
/// 0`), the cursor starts at `finalized.max(floor)` without walking older history. Returns `None`
/// while the finalized block is unknown.
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
/// Two comparisons of the same range are fetched a few seconds apart. A backend that answers with
/// a partial result produces differences that the other comparison does not show, so only a
/// difference that both comparisons report counts. Order follows `first`.
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

/// Plans the next post-finality verification round of a cursor that is already set.
///
/// The cursor only ever advances over blocks that the indexer has already committed. `last_indexed`
/// bounds the range from above, so a finalized tag that moves faster than the indexer never asks for
/// logs that were not stored yet, and a round covers at most [`MAX_VERIFICATION_RANGE`] blocks. The
/// cursor is only advanced by the caller after the range matched, so a finalized block moving
/// backwards never moves it backwards.
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

fn describe_log(log: &LogIdentity, kind: &str) -> String {
    format!(
        "{kind}: block {} log {} block hash {} tx hash {}",
        log.block_number, log.log_index, log.block_hash, log.tx_hash,
    )
}

/// Summarises the differences of a mismatch, capped at [`MAX_MISMATCH_EXAMPLES`] entries.
fn describe_mismatch(mismatch: &VerificationMismatch) -> Vec<String> {
    let mut examples = Vec::new();

    for log in &mismatch.missing_from_chain {
        if examples.len() >= MAX_MISMATCH_EXAMPLES {
            return examples;
        }
        examples.push(describe_log(log, "missing from chain"));
    }

    for log in &mismatch.missing_from_store {
        if examples.len() >= MAX_MISMATCH_EXAMPLES {
            return examples;
        }
        examples.push(describe_log(log, "missing from store"));
    }

    for change in &mismatch.changed {
        if examples.len() >= MAX_MISMATCH_EXAMPLES {
            return examples;
        }
        examples.push(format!(
            "changed: block {} log {}, stored block hash {} tx hash {}, canonical block hash {} tx hash {}",
            change.stored.block_number,
            change.stored.log_index,
            change.stored.block_hash,
            change.stored.tx_hash,
            change.canonical.block_hash,
            change.canonical.tx_hash,
        ));
    }

    examples
}

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

fn record_false_confirmation() {
    #[cfg(all(feature = "telemetry", not(test)))]
    if let Some(metric) = METRIC_FCR_FALSE_CONFIRMATIONS_TOTAL.as_ref() {
        metric.increment();
    }
}

/// Re-fetches the canonical logs of a range and compares them with the stored ones.
async fn verify_range<T, U, Db>(
    rpc: &T,
    db: &Db,
    logs_handler: &Arc<U>,
    from: u64,
    to: u64,
) -> Result<VerificationMismatch>
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
    let filters = generate_log_filters(db, logs_handler.as_ref(), LogFilterPhase::Continuous).await?;

    // The canonical fetch uses every currently known filter, which is a superset of the filters the
    // block was indexed with: the set only grows. Token approvals are fetched by the indexer but
    // filtered out before storage, so the compared canonical set is the non-token one.
    let canonical = rpc.get_logs_for_block_range(filters.no_token, from, to).await?;

    // Blocks `from..=to`; the offset counts the blocks stored after `from`.
    let stored = db.get_logs(Some(from), Some(to - from)).await?;

    // Only static contracts configured from startup are required in the missing-from-store
    // direction; runtime-discovered Safe contracts are still checked strictly when stored.
    let token = logs_handler.contract_addresses_map().token;
    let static_addresses = logs_handler
        .contract_addresses()
        .into_iter()
        .filter(|address| *address != token)
        .collect::<BTreeSet<_>>();

    Ok(compare_logs(&stored, &canonical, &static_addresses))
}

/// Verifies indexed ranges once the chain tip tracker reports them as final.
///
/// The task runs until the tip publisher is dropped, the indexer shuts down or a corroborated
/// mismatch is found. `last_indexed_at_startup` is the committed watermark before live indexing
/// starts, and `floor` is the block below the configured start block. `round_updates` publishes the
/// number of completed rounds so tests can wait for a round instead of sleeping; production passes
/// `None`.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn run_verification<T, U, Db>(
    mut tip_updates: watch::Receiver<Option<ChainTip>>,
    rpc: &T,
    db: Db,
    logs_handler: Arc<U>,
    indexer_state: IndexerState,
    last_indexed_at_startup: u64,
    floor: u64,
    round_updates: Option<watch::Sender<u32>>,
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
    let mut cursor: Option<u64> = None;
    let mut pending_mismatch: Option<((u64, u64), VerificationMismatch)> = None;
    let mut failure_streak: u32 = 0;
    let mut rounds: u32 = 0;

    loop {
        // The committed block is read before the finalized one of this round: a block committed
        // while this round is being planned then falls into the next round instead of being
        // compared against a canonical set that was fetched before it existed.
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
                match verify_range(rpc, &db, &logs_handler, from, to).await {
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
                            // response without an error; keep the cursor unchanged and re-check
                            // the same range on the next tip update.
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
    use hopr_types::primitive::prelude::{Address, SerializableLog};
    use mockall::mock;
    use tokio::sync::watch;

    use super::{
        CanonicalLogFetcher, ChangedLog, CursorStep, LogIdentity, STARTUP_LOOKBACK_BLOCKS, VerificationMismatch,
        compare_logs, corroborated_mismatch, initial_cursor, next_cursor_step, run_verification,
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

        // Canonical Safe log missing from store is exempt.
        let stored = vec![sample_log(20, 0, 1, 2)];
        let canonical = vec![sample_log(20, 0, 1, 2), sample_safe_log(20)];
        assert!(compare_logs(&stored, &canonical, &static_addrs).is_empty());

        // Stored Safe logs are still checked strictly for missing_from_chain and changed.
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

    struct VerificationHarness {
        db: BlokliDb,
        state: IndexerState,
        tip_tx: watch::Sender<Option<ChainTip>>,
        round_rx: watch::Receiver<u32>,
        task: tokio::task::JoinHandle<()>,
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
                        db,
                        Arc::new(log_handler(contracts)),
                        state,
                        last_indexed_at_startup,
                        floor,
                        Some(round_tx),
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
            tokio::time::timeout(
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

        let mut h = VerificationHarness::spawn(db.clone(), rpc, vec![static_addr()], Some(10), 0, 0).await;
        h.wait_round(1).await;

        // First mismatch records pending state without halting yet.
        h.send_finalized(Some(12));
        h.wait_round(2).await;
        assert_eq!(calls.load(AtomicOrdering::SeqCst), 1);
        assert!(!h.state.verification_failed());
        assert_eq!(db.verification_failed_at().await?, None);

        // Next tip update corroborates the same range, persists the halt marker, and stops the task.
        h.send_finalized(Some(13));
        tokio::time::timeout(Duration::from_secs(1), h.task)
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

        // Case 1: transient empty first response, clean recheck on next tip update.
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

        // Case 2: two conflicting partial responses retry without halting or writing a halt marker.
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
        // RPC error retries on the next tip update.
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

        // Canonical Safe log missing from store does not halt verification.
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
    async fn startup_lookback_and_empty_index_initialization() -> anyhow::Result<()> {
        // Resuming an existing index verifies the last STARTUP_LOOKBACK_BLOCKS blocks in one fetch.
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

        // Empty index starts at the first finalized block without fetching older history.
        let db = prepare_db(0, Vec::new()).await?;
        let mut h = VerificationHarness::spawn(db, MockCanonicalLogs::new(), Vec::new(), None, 0, 0).await;
        h.wait_round(1).await;
        h.send_finalized(Some(50));
        h.wait_round(2).await;
        assert!(!h.state.verification_failed());
        h.finish().await?;

        // Range read only returns logs inside [from, to].
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
