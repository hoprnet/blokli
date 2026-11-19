//! Post-finality verification of indexed logs.
//!
//! A background task re-checks every block that was indexed before it was final once the chain tip
//! tracker publishes that the block is final. It re-runs `eth_getLogs` with the same filter set the
//! indexer uses and compares the result with the logs stored in the logs database on block number,
//! block hash, transaction hash and log index. Any difference means that the logs were indexed from
//! a chain that the canonical chain does not confirm, or that the canonical chain has logs that were
//! never indexed.
//!
//! A range that does not match is compared a second time after a short delay, because a backend
//! can return an empty or partial `eth_getLogs` response without an error. A confirmed mismatch is
//! counted in `blokli_fcr_false_confirmations_total` and recorded in the shared
//! [`VerificationStatus`] flag. The flag is sticky: the indexer stops advancing and readiness
//! reports not ready until an operator restarts the process. A failing RPC or database read is not
//! a mismatch: the same range is retried on the next tip update.
//!
//! The position the task reached is stored in the index database, so a restart resumes where the
//! previous process stopped instead of skipping the blocks that became final while it was down.
//!
//! The set of Safe contract addresses grows at runtime, while the stored logs of a block only
//! contain the addresses known when that block was indexed. A canonical log of a Safe is therefore
//! only reported as missing from the store when the continuous phase of this process indexed the
//! block and the Safe was discovered in an earlier block; the canonical logs of the static
//! contracts are always required.

use std::{
    collections::{BTreeMap, BTreeSet},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use blokli_chain_rpc::{HoprIndexerRpcOperations, tip::ChainTip};
use blokli_db::{
    BlokliDbGeneralModelOperations, api::logs::BlokliDbLogOperations, info::BlokliDbInfoOperations,
    safe_contracts::BlokliDbSafeContractOperations, safe_history::BlokliDbSafeHistoryOperations,
};
#[cfg(all(feature = "telemetry", not(test)))]
use hopr_types::telemetry::SimpleCounter;
use hopr_types::{
    crypto::types::Hash,
    primitive::prelude::{Address, SerializableLog, ToHex},
};
use serde::Serialize;
use tokio::{sync::watch, time::sleep};
use tracing::{debug, error, info, warn};

use crate::{
    IndexerState,
    block::{Indexer, LogFilterPhase},
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

/// Wait before a mismatching range is compared with the canonical chain a second time.
///
/// A load-balanced or lagging backend can answer `eth_getLogs` with an empty or partial result
/// without an error, so a single response does not decide that the stored logs are wrong.
#[cfg(not(test))]
const MISMATCH_RECHECK_DELAY: Duration = Duration::from_secs(3);

/// Test builds re-check a mismatching range without a real wait.
#[cfg(test)]
const MISMATCH_RECHECK_DELAY: Duration = Duration::from_millis(1);

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
    /// The flag is sticky: only a process restart clears it.
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

/// Which canonical logs the store is expected to hold.
///
/// A canonical log can only be in the store when it was requested while its block was indexed. The
/// static contracts are filtered from the first block on. A Safe is filtered only from the block
/// after its discovery, and only once the continuous phase of this process began: the historical
/// discovery phase indexes with no Safe filters at all.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StoreCoverage {
    /// Contract addresses that are configured at startup and never change.
    pub static_addresses: BTreeSet<Address>,
    /// Block in which each Safe first appeared, from the earliest state stored for it.
    pub safe_discovery_blocks: BTreeMap<Address, u64>,
    /// First block indexed by the continuous phase of this process.
    pub continuous_start: Option<u64>,
}

impl StoreCoverage {
    /// Returns whether a canonical log of `address` at `block` must be present in the store.
    pub fn requires_log(&self, address: &Address, block: u64) -> bool {
        if self.static_addresses.contains(address) {
            return true;
        }

        let Some(continuous_start) = self.continuous_start else {
            return false;
        };

        // A Safe discovered in block `d` is not in the filter for `d` itself: the filter is rebuilt for the
        // next block once the discovery is processed, so a log of the Safe in its own discovery block was
        // never requested.
        self.safe_discovery_blocks
            .get(address)
            .is_some_and(|discovery_block| *discovery_block < block && block >= continuous_start)
    }
}

/// Compares the stored logs of a range with the canonical logs of the same range.
///
/// Logs are matched by their position within the chain, that is a block number and a log index.
/// Two logs at the same position differ when their block hash or their transaction hash differs,
/// which is what a reorganization produces. The event payload itself is not compared, because a
/// matching position and block hash pin down the payload.
///
/// `coverage` decides whether a canonical log that is absent from the store can be reported: only
/// logs the store was able to hold when the block was indexed are compared that way.
pub fn compare_logs(
    stored: &[SerializableLog],
    canonical: &[SerializableLog],
    coverage: &StoreCoverage,
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
        if !stored_by_position.contains_key(position)
            && coverage.requires_log(&canonical_log.address, canonical_log.block_number)
        {
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

/// Chooses the block at which an unset cursor starts once a finalized block is known.
///
/// `baseline` is the last block the indexer had committed when the process started. Everything
/// above it may have been indexed while finality was still unknown, so the cursor starts at its
/// minimum with the first finalized block and those blocks are re-checked once they become final.
/// Returns `None` while the finalized block is unknown.
pub fn initial_cursor(baseline: Option<u64>, finalized: Option<u64>) -> Option<u64> {
    finalized.map(|finalized| baseline.map_or(finalized, |last_indexed| last_indexed.min(finalized)))
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
    indexer_state: &IndexerState,
    from: u64,
    to: u64,
) -> Result<VerificationMismatch>
where
    T: HoprIndexerRpcOperations + Sync + Send + 'static,
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
    let filters = Indexer::<T, U, Db>::generate_log_filters(db, logs_handler, LogFilterPhase::Continuous).await?;

    // The canonical fetch uses every currently known filter, which is a superset of the filters the
    // block was indexed with: the set only grows. Token approvals are fetched by the indexer but
    // filtered out before storage, so the compared canonical set is the non-token one.
    let canonical = rpc.get_logs_for_block_range(filters.no_token, from, to).await?;

    // Blocks `from..=to`; the offset counts the blocks stored after `from`.
    let stored = db.get_logs(Some(from), Some(to - from)).await?;

    // Only the contracts that are configured from the start can be expected to be in the store of
    // every block. Safe addresses appear while indexing, so their canonical logs are exempt from
    // the missing-from-store direction wherever the filter could not have contained them yet.
    let token = logs_handler.contract_addresses_map().token;
    let coverage = StoreCoverage {
        static_addresses: logs_handler
            .contract_addresses()
            .into_iter()
            .filter(|address| *address != token)
            .collect::<BTreeSet<_>>(),
        safe_discovery_blocks: db.get_safe_contract_discovery_blocks(None).await?,
        continuous_start: indexer_state.continuous_start_block(),
    };

    Ok(compare_logs(&stored, &canonical, &coverage))
}

/// Verifies indexed ranges once the chain tip tracker reports them as final.
///
/// The task runs until the tip publisher is dropped, the indexer shuts down or a mismatch is found.
/// `baseline` is the committed block at the moment the indexer started, captured before the
/// indexing loop could write a block that the first finalized response would no longer cover.
/// `round_updates` publishes the number of completed rounds so tests can wait for a round instead
/// of sleeping; production passes `None`.
pub(crate) async fn run_verification<T, U, Db>(
    mut tip_updates: watch::Receiver<Option<ChainTip>>,
    rpc: &T,
    db: Db,
    logs_handler: Arc<U>,
    indexer_state: IndexerState,
    baseline: Option<u64>,
    round_updates: Option<watch::Sender<u32>>,
) where
    T: HoprIndexerRpcOperations + Sync + Send + 'static,
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
    let mut cursor_loaded = false;
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
            // The persisted position is read once. A failed read retries the round, because starting
            // over from the finalized block would skip the blocks below the stored position.
            if !cursor_loaded {
                match db.get_last_verified_block(None).await {
                    Ok(persisted) => {
                        cursor = persisted;
                        cursor_loaded = true;
                        failure_streak = 0;
                        if let Some(cursor) = cursor {
                            info!(
                                verified_up_to = cursor,
                                "resuming post-finality verification from the stored position"
                            );
                        }
                    }
                    Err(error) => record_failure(&mut failure_streak, None, &error.to_string()),
                }
            }

            if cursor_loaded {
                let finalized = tip_updates.borrow().and_then(|tip| tip.finalized);

                match cursor {
                    None => {
                        // The baseline is unset when nothing was indexed at process start, so no
                        // block is owed a check. Recording it would start the cursor at the genesis
                        // block and walk the whole chain below the first indexed block.
                        if let Some(start) = initial_cursor(baseline, finalized) {
                            match db.set_last_verified_block(None, start).await {
                                Ok(()) => {
                                    cursor = Some(start);
                                    failure_streak = 0;
                                    info!(
                                        verified_up_to = start,
                                        "post-finality verification started, blocks above the cursor are re-checked \
                                         once they become final"
                                    );
                                }
                                Err(error) => record_failure(&mut failure_streak, None, &error.to_string()),
                            }
                        }
                    }
                    Some(current) => match next_cursor_step(current, finalized, last_indexed) {
                        CursorStep::Idle => {}
                        CursorStep::Verify { from, to } => {
                            // A mismatching range is compared a second time after a short delay:
                            // a load-balanced or lagging backend can return an empty or partial
                            // response without an error, and only a second mismatch means that the
                            // canonical chain really differs.
                            let mut matched = true;

                            match verify_range(rpc, &db, &logs_handler, &indexer_state, from, to).await {
                                Ok(mismatch) if mismatch.is_empty() => {}
                                Ok(mismatch) => {
                                    warn!(
                                        from_block = from,
                                        to_block = to,
                                        missing_from_chain = mismatch.missing_from_chain.len(),
                                        missing_from_store = mismatch.missing_from_store.len(),
                                        changed = mismatch.changed.len(),
                                        examples = ?describe_mismatch(&mismatch),
                                        "post-finality verification found logs that the canonical chain does not \
                                         confirm, re-checking the range"
                                    );

                                    sleep(MISMATCH_RECHECK_DELAY).await;

                                    match verify_range(rpc, &db, &logs_handler, &indexer_state, from, to).await {
                                        Ok(recheck) if recheck.is_empty() => {
                                            debug!(
                                                from_block = from,
                                                to_block = to,
                                                "the re-check matched the stored logs"
                                            );
                                        }
                                        Ok(recheck) => {
                                            error!(
                                                from_block = from,
                                                to_block = to,
                                                missing_from_chain = recheck.missing_from_chain.len(),
                                                missing_from_store = recheck.missing_from_store.len(),
                                                changed = recheck.changed.len(),
                                                examples = ?describe_mismatch(&recheck),
                                                "post-finality verification found logs that the canonical chain does \
                                                 not confirm, halting the indexer"
                                            );
                                            record_false_confirmation();
                                            indexer_state.mark_verification_failed();
                                            return;
                                        }
                                        Err(error) => {
                                            record_failure(&mut failure_streak, Some((from, to)), &error.to_string());
                                            matched = false;
                                        }
                                    }
                                }
                                Err(error) => {
                                    record_failure(&mut failure_streak, Some((from, to)), &error.to_string());
                                    matched = false;
                                }
                            }

                            if matched {
                                // The cursor only moves once its new value is stored: a failed write
                                // re-verifies the same range instead of leaving it unchecked.
                                match db.set_last_verified_block(None, to).await {
                                    Ok(()) => {
                                        cursor = Some(to);
                                        failure_streak = 0;
                                        debug!(
                                            from_block = from,
                                            to_block = to,
                                            "post-finality verification matched the stored logs"
                                        );
                                    }
                                    Err(error) => {
                                        record_failure(&mut failure_streak, Some((from, to)), &error.to_string())
                                    }
                                }
                            }
                        }
                    },
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
        collections::{BTreeMap, BTreeSet},
        pin::Pin,
        sync::{
            Arc,
            atomic::{AtomicUsize, Ordering as AtomicOrdering},
        },
        time::Duration,
    };

    use async_trait::async_trait;
    use blokli_chain_rpc::{
        BlockWithLogs, FilterSet, Log as RpcLog, SafeOwnersSnapshot,
        errors::{Result as RpcResult, RpcError},
        tip::{ChainTip, FcrState, HeadSource},
    };
    use blokli_chain_types::ContractAddresses;
    use blokli_db::{
        BlokliDbGeneralModelOperations, TargetDb, api::logs::BlokliDbLogOperations, db::BlokliDb,
        info::BlokliDbInfoOperations, safe_contracts::BlokliDbSafeContractOperations,
    };
    use futures::Stream;
    use hopr_bindings::exports::alloy::{primitives::B256, rpc::types::Filter};
    use hopr_types::{
        crypto::types::Hash,
        primitive::prelude::{Address, HoprBalance, SerializableLog, XDaiBalance, XHoprBalance},
    };
    use mockall::mock;
    use sea_orm::ConnectionTrait;
    use tokio::sync::watch;

    use super::{CursorStep, StoreCoverage, compare_logs, initial_cursor, next_cursor_step, run_verification};
    use crate::{IndexerState, traits::MockChainLogHandler};

    fn sample_log(block_number: u64, log_index: u64, block_hash: u8, tx_hash: u8) -> SerializableLog {
        SerializableLog {
            address: Address::from([0x11; 20]),
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

    fn static_addresses(addresses: &[Address]) -> BTreeSet<Address> {
        addresses.iter().copied().collect()
    }

    /// Coverage of a chain whose only static contract emitted the sample logs.
    fn sample_coverage() -> StoreCoverage {
        StoreCoverage {
            static_addresses: static_addresses(&[Address::from([0x11; 20])]),
            safe_discovery_blocks: BTreeMap::new(),
            continuous_start: Some(0),
        }
    }

    /// Coverage of a chain that also holds the given Safes, discovered in the given blocks.
    fn coverage_with_safes(safes: &[(Address, u64)], continuous_start: Option<u64>) -> StoreCoverage {
        StoreCoverage {
            static_addresses: static_addresses(&[Address::from([0x11; 20])]),
            safe_discovery_blocks: safes.iter().copied().collect(),
            continuous_start,
        }
    }

    #[test]
    fn identical_log_sets_have_no_mismatch() {
        let logs = vec![sample_log(10, 0, 1, 2), sample_log(10, 1, 3, 4)];

        let mismatch = compare_logs(&logs, &logs, &sample_coverage());

        assert!(mismatch.is_empty());
        assert_eq!(mismatch.len(), 0);
    }

    #[test]
    fn empty_ranges_have_no_mismatch() {
        let mismatch = compare_logs(&[], &[], &sample_coverage());

        assert!(mismatch.is_empty());
    }

    #[test]
    fn changed_block_hash_is_reported() {
        let stored = vec![sample_log(10, 0, 1, 2)];
        let canonical = vec![sample_log(10, 0, 9, 2)];

        let mismatch = compare_logs(&stored, &canonical, &sample_coverage());

        insta::assert_yaml_snapshot!("changed_block_hash", mismatch);
    }

    #[test]
    fn changed_transaction_hash_is_reported() {
        let stored = vec![sample_log(10, 0, 1, 2)];
        let canonical = vec![sample_log(10, 0, 1, 9)];

        let mismatch = compare_logs(&stored, &canonical, &sample_coverage());

        insta::assert_yaml_snapshot!("changed_transaction_hash", mismatch);
    }

    #[test]
    fn stored_log_missing_from_chain_is_reported() {
        let stored = vec![sample_log(10, 0, 1, 2), sample_log(11, 0, 3, 4)];
        let canonical = vec![sample_log(10, 0, 1, 2)];

        let mismatch = compare_logs(&stored, &canonical, &sample_coverage());

        insta::assert_yaml_snapshot!("stored_log_missing_from_chain", mismatch);
    }

    #[test]
    fn canonical_log_missing_from_store_is_reported() {
        let stored = vec![sample_log(10, 0, 1, 2)];
        let canonical = vec![sample_log(10, 0, 1, 2), sample_log(11, 0, 3, 4)];

        let mismatch = compare_logs(&stored, &canonical, &sample_coverage());

        insta::assert_yaml_snapshot!("canonical_log_missing_from_store", mismatch);
    }

    fn sample_safe_log(address: Address, block_number: u64) -> SerializableLog {
        SerializableLog {
            address,
            ..sample_log(block_number, 1, 3, 4)
        }
    }

    /// A canonical log of a Safe that was in the filter for the block, in the continuous phase of
    /// this process, is only accepted when it is stored.
    #[test]
    fn canonical_log_of_a_known_safe_in_a_continuous_block_is_missing_from_store() {
        let safe = Address::from([0x33; 20]);
        let coverage = coverage_with_safes(&[(safe, 10)], Some(5));
        let stored = vec![sample_log(20, 0, 1, 2)];
        let canonical = vec![sample_log(20, 0, 1, 2), sample_safe_log(safe, 20)];

        let mismatch = compare_logs(&stored, &canonical, &coverage);

        assert_eq!(mismatch.missing_from_store.len(), 1);
        assert_eq!(mismatch.missing_from_store[0].block_number, 20);
        assert_eq!(mismatch.missing_from_store[0].log_index, 1);
    }

    #[test]
    fn canonical_log_of_a_safe_discovered_after_the_block_is_exempt() {
        let safe = Address::from([0x33; 20]);
        let coverage = coverage_with_safes(&[(safe, 30)], Some(5));
        let stored = vec![sample_log(20, 0, 1, 2)];
        let canonical = vec![sample_log(20, 0, 1, 2), sample_safe_log(safe, 20)];

        let mismatch = compare_logs(&stored, &canonical, &coverage);

        assert!(
            mismatch.is_empty(),
            "a Safe discovered after indexing must not produce a missing-from-store mismatch: {mismatch:?}"
        );
    }

    /// The filter is rebuilt for the block after the discovery, so the Safe was not requested in
    /// its own discovery block.
    #[test]
    fn canonical_log_of_a_safe_in_its_discovery_block_is_exempt() {
        let safe = Address::from([0x33; 20]);
        let coverage = coverage_with_safes(&[(safe, 20)], Some(5));
        let stored = vec![sample_log(20, 0, 1, 2)];
        let canonical = vec![sample_log(20, 0, 1, 2), sample_safe_log(safe, 20)];

        let in_discovery_block = compare_logs(&stored, &canonical, &coverage);
        assert!(
            in_discovery_block.is_empty(),
            "the discovery block itself was indexed without the Safe filter: {in_discovery_block:?}"
        );

        let after_discovery = compare_logs(
            &stored,
            &[sample_log(20, 0, 1, 2), sample_safe_log(safe, 21)],
            &coverage,
        );
        assert_eq!(after_discovery.missing_from_store.len(), 1);
    }

    #[test]
    fn canonical_log_of_a_safe_below_the_continuous_start_is_exempt() {
        let safe = Address::from([0x33; 20]);
        let coverage = coverage_with_safes(&[(safe, 5)], Some(20));
        let stored = vec![sample_log(10, 0, 1, 2)];
        let canonical = vec![sample_log(10, 0, 1, 2), sample_safe_log(safe, 10)];

        let mismatch = compare_logs(&stored, &canonical, &coverage);

        assert!(
            mismatch.is_empty(),
            "a block the historical discovery phase indexed holds no Safe logs: {mismatch:?}"
        );
    }

    #[test]
    fn canonical_log_of_a_safe_is_exempt_before_the_continuous_phase_began() {
        let safe = Address::from([0x33; 20]);
        let coverage = coverage_with_safes(&[(safe, 5)], None);
        let stored = vec![sample_log(10, 0, 1, 2)];
        let canonical = vec![sample_log(10, 0, 1, 2), sample_safe_log(safe, 10)];

        assert!(compare_logs(&stored, &canonical, &coverage).is_empty());
    }

    /// Static contract logs keep the strict rule, including before the continuous phase began.
    #[test]
    fn canonical_log_of_a_static_contract_is_missing_from_store_without_continuous_coverage() {
        let coverage = coverage_with_safes(&[], None);
        let stored = vec![sample_log(10, 0, 1, 2)];
        let canonical = vec![sample_log(10, 0, 1, 2), sample_log(10, 1, 3, 4)];

        let mismatch = compare_logs(&stored, &canonical, &coverage);

        assert_eq!(mismatch.missing_from_store.len(), 1);
    }

    /// The Safe exemption only applies to the missing-from-store direction.
    #[test]
    fn stored_safe_logs_are_compared_strictly() {
        let safe = Address::from([0x33; 20]);
        let coverage = coverage_with_safes(&[(safe, 5)], Some(0));
        let stored = vec![sample_safe_log(safe, 20), sample_safe_log(safe, 21)];
        let canonical = vec![sample_safe_log(safe, 20)];

        let mismatch = compare_logs(&stored, &canonical, &coverage);

        assert_eq!(mismatch.missing_from_chain.len(), 1);
        assert_eq!(mismatch.missing_from_chain[0].block_number, 21);
    }

    /// Blocks indexed between the process start and the first finalized response are verified from
    /// the watermark captured at start, not from the watermark of the first round.
    #[tokio::test]
    async fn baseline_captured_at_start_bounds_the_initial_cursor() -> anyhow::Result<()> {
        let logs = vec![sample_log(101, 0, 1, 2), sample_log(105, 0, 3, 4)];
        // The indexer advanced to 110 before the verification task ran its first round.
        let db = prepare_db(110, logs.clone()).await?;
        let observed = db.clone();

        // A range of 106..110 would be requested if the cursor started at the finalized block.
        let mut rpc = MockVerificationRpc::new();
        rpc.expect_get_logs_for_block_range()
            .withf(|_, from: &u64, to: &u64| *from == 101 && *to == 105)
            .returning(move |_, _, _| Ok(logs.clone()));

        let indexer_state = IndexerState::default();
        let (tip_tx, tip_rx) = watch::channel(Some(sample_tip(Some(105))));
        let (round_tx, mut round_rx) = watch::channel(0);
        let task = spawn_verification(
            Arc::new(rpc),
            db,
            log_handler(Vec::new()),
            indexer_state.clone(),
            tip_rx,
            Some(100),
            round_tx,
        );

        // The first round sets the cursor to the baseline and the next tip update verifies the
        // range between the baseline and the finalized block.
        wait_for_round(&mut round_rx, 1).await;
        tip_tx.send_replace(Some(sample_tip(Some(105))));
        wait_for_round(&mut round_rx, 2).await;

        assert_eq!(
            observed.get_last_verified_block(None).await?,
            Some(105),
            "the range above the starting watermark is verified"
        );
        assert!(!indexer_state.verification_failed());
        drop(tip_tx);
        task.await?;

        Ok(())
    }

    #[test]
    fn initial_cursor_is_unset_while_finalized_is_unknown() {
        assert_eq!(initial_cursor(None, None), None);
        assert_eq!(initial_cursor(Some(100), None), None);
    }

    #[test]
    fn initial_cursor_starts_at_the_first_finalized_block() {
        assert_eq!(initial_cursor(None, Some(42)), Some(42));
        assert_eq!(initial_cursor(Some(100), Some(42)), Some(42));
    }

    /// Blocks indexed after the starting watermark are re-checked from it.
    #[test]
    fn initial_cursor_starts_at_the_lower_of_the_baseline_and_the_finalized_block() {
        assert_eq!(initial_cursor(Some(100), Some(110)), Some(100));
        assert_eq!(initial_cursor(Some(110), Some(110)), Some(110));
    }

    #[test]
    fn cursor_step_is_idle_while_finalized_is_unknown() {
        assert_eq!(next_cursor_step(10, None, 100), CursorStep::Idle);
    }

    #[test]
    fn cursor_step_verifies_above_the_cursor() {
        assert_eq!(
            next_cursor_step(10, Some(15), 100),
            CursorStep::Verify { from: 11, to: 15 }
        );
    }

    #[test]
    fn cursor_step_stays_idle_when_finalized_is_at_or_below_the_cursor() {
        assert_eq!(next_cursor_step(10, Some(10), 100), CursorStep::Idle);
        assert_eq!(next_cursor_step(10, Some(9), 100), CursorStep::Idle);
    }

    #[test]
    fn cursor_step_is_bounded_by_the_last_indexed_block() {
        assert_eq!(
            next_cursor_step(10, Some(20), 15),
            CursorStep::Verify { from: 11, to: 15 }
        );
        assert_eq!(next_cursor_step(10, Some(20), 10), CursorStep::Idle);
    }

    #[test]
    fn cursor_step_caps_one_round_at_the_maximum_range() {
        assert_eq!(
            next_cursor_step(10, Some(5_000), 5_000),
            CursorStep::Verify { from: 11, to: 1_010 }
        );
        assert_eq!(
            next_cursor_step(1_010, Some(5_000), 5_000),
            CursorStep::Verify { from: 1_011, to: 2_010 }
        );
    }

    fn sample_tip(finalized: Option<u64>) -> ChainTip {
        ChainTip {
            latest: 200,
            safe: None,
            finalized,
            confirmed: 180,
            confirmation_height: finalized,
            source: HeadSource::Polling,
            fcr: FcrState::Unavailable,
        }
    }

    mock! {
        VerificationRpc {}

        #[async_trait]
        impl blokli_chain_rpc::HoprIndexerRpcOperations for VerificationRpc {
            async fn block_number(&self) -> RpcResult<u64>;
            async fn get_transaction_sender(&self, tx_hash: Hash) -> RpcResult<Address>;
            async fn get_transaction_bytes(&self, tx_hash: Hash) -> RpcResult<Vec<u8>>;
            fn try_stream_logs<'a>(
                &'a self,
                start_block_number: u64,
                filters: FilterSet,
                is_synced: bool,
            ) -> RpcResult<Pin<Box<dyn Stream<Item = BlockWithLogs> + Send + 'a>>>;
            async fn get_logs_for_address(
                &self,
                address: Address,
                topics: Vec<B256>,
                from_block: u64,
                to_block: u64,
            ) -> RpcResult<Vec<RpcLog>>;
            async fn get_safe_owners_and_threshold(
                &self,
                safe_address: Address,
                block: u64,
            ) -> RpcResult<SafeOwnersSnapshot>;
            async fn get_xdai_balance(&self, address: Address) -> RpcResult<XDaiBalance>;
            async fn get_hopr_balance(&self, address: Address) -> RpcResult<HoprBalance>;
            async fn get_xhopr_balance(&self, address: Address) -> RpcResult<XHoprBalance>;
            async fn get_hopr_allowance(&self, owner: Address, spender: Address) -> RpcResult<HoprBalance>;
            async fn get_transaction_count(&self, address: Address) -> RpcResult<u64>;
            async fn get_channel_closure_notice_period(&self) -> RpcResult<Duration>;
            async fn get_hopr_module_from_safe(&self, safe_address: Address) -> RpcResult<Option<Address>>;
            fn subscribe_tip(&self) -> Option<watch::Receiver<Option<ChainTip>>>;
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

    /// Waits for the verification task to complete the given round.
    async fn wait_for_round(round_updates: &mut watch::Receiver<u32>, round: u32) {
        tokio::time::timeout(
            Duration::from_secs(1),
            round_updates.wait_for(|completed| *completed >= round),
        )
        .await
        .unwrap_or_else(|_| panic!("verification should reach round {round}"))
        .expect("round updates should stay connected");
    }

    async fn prepare_db(last_indexed: u32, logs: Vec<SerializableLog>) -> anyhow::Result<BlokliDb> {
        let db = BlokliDb::new_in_memory().await?;
        db.set_indexer_state_info(None, last_indexed).await?;
        db.store_logs(logs).await?;
        // The logs are stored unprocessed; processing is irrelevant to verification.
        Ok(db)
    }

    fn spawn_verification(
        rpc: Arc<MockVerificationRpc>,
        db: BlokliDb,
        handler: MockChainLogHandler,
        indexer_state: IndexerState,
        tip_updates: watch::Receiver<Option<ChainTip>>,
        baseline: Option<u64>,
        round_updates: watch::Sender<u32>,
    ) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            run_verification(
                tip_updates,
                rpc.as_ref(),
                db,
                Arc::new(handler),
                indexer_state,
                baseline,
                Some(round_updates),
            )
            .await;
        })
    }

    #[tokio::test]
    async fn clean_range_advances_and_persists_the_cursor_and_leaves_the_flag_unset() -> anyhow::Result<()> {
        let logs = vec![sample_log(11, 0, 1, 2), sample_log(12, 0, 3, 4)];
        let db = prepare_db(12, logs.clone()).await?;
        let observed = db.clone();
        let calls = Arc::new(AtomicUsize::new(0));
        let calls_for_rpc = calls.clone();

        let mut rpc = MockVerificationRpc::new();
        rpc.expect_get_logs_for_block_range()
            .withf(|_, from: &u64, to: &u64| *from == 11 && *to == 12)
            .returning(move |_, _, _| {
                calls_for_rpc.fetch_add(1, AtomicOrdering::SeqCst);
                Ok(logs.clone())
            });

        let indexer_state = IndexerState::default();
        let (tip_tx, tip_rx) = watch::channel(Some(sample_tip(Some(10))));
        let (round_tx, mut round_rx) = watch::channel(0);
        let task = spawn_verification(
            Arc::new(rpc),
            db,
            log_handler(Vec::new()),
            indexer_state.clone(),
            tip_rx,
            None,
            round_tx,
        );

        wait_for_round(&mut round_rx, 1).await;
        tip_tx.send_replace(Some(sample_tip(Some(12))));
        wait_for_round(&mut round_rx, 2).await;

        tip_tx.send_replace(Some(sample_tip(Some(12))));
        wait_for_round(&mut round_rx, 3).await;

        assert_eq!(calls.load(AtomicOrdering::SeqCst), 1, "the cursor should have advanced");
        assert_eq!(
            observed.get_last_verified_block(None).await?,
            Some(12),
            "the advanced cursor is stored so a restart resumes from it"
        );
        assert!(!indexer_state.verification_failed());
        drop(tip_tx);
        task.await?;

        Ok(())
    }

    #[tokio::test]
    async fn mismatch_sets_the_flag_stops_the_task_and_keeps_the_cursor() -> anyhow::Result<()> {
        let stored = vec![sample_log(11, 0, 1, 2), sample_log(12, 0, 3, 4)];
        let db = prepare_db(12, stored).await?;
        let observed = db.clone();
        let calls = Arc::new(AtomicUsize::new(0));
        let calls_for_rpc = calls.clone();

        let mut rpc = MockVerificationRpc::new();
        rpc.expect_get_logs_for_block_range().returning(move |_, _, _| {
            calls_for_rpc.fetch_add(1, AtomicOrdering::SeqCst);
            Ok(vec![sample_log(11, 0, 1, 2)])
        });

        let indexer_state = IndexerState::default();
        let (tip_tx, tip_rx) = watch::channel(Some(sample_tip(Some(10))));
        let (round_tx, mut round_rx) = watch::channel(0);
        let task = spawn_verification(
            Arc::new(rpc),
            db,
            log_handler(Vec::new()),
            indexer_state.clone(),
            tip_rx,
            None,
            round_tx,
        );

        wait_for_round(&mut round_rx, 1).await;
        tip_tx.send_replace(Some(sample_tip(Some(12))));

        tokio::time::timeout(Duration::from_secs(1), task)
            .await
            .expect("the task should stop after a mismatch")?;

        tip_tx.send_replace(Some(sample_tip(Some(13))));

        assert_eq!(
            calls.load(AtomicOrdering::SeqCst),
            2,
            "the range is compared again before the indexer stops"
        );
        assert!(indexer_state.verification_failed());
        assert_eq!(
            observed.get_last_verified_block(None).await?,
            Some(10),
            "a mismatching range does not advance the stored position"
        );

        Ok(())
    }

    /// A single empty response does not halt the indexer: the range is compared again.
    #[tokio::test]
    async fn a_transient_empty_response_is_confirmed_by_a_second_fetch() -> anyhow::Result<()> {
        let logs = vec![sample_log(11, 0, 1, 2), sample_log(12, 0, 3, 4)];
        let db = prepare_db(12, logs.clone()).await?;
        let observed = db.clone();
        let calls = Arc::new(AtomicUsize::new(0));
        let calls_for_rpc = calls.clone();

        let mut rpc = MockVerificationRpc::new();
        rpc.expect_get_logs_for_block_range()
            .withf(|_, from: &u64, to: &u64| *from == 11 && *to == 12)
            .returning(move |_, _, _| {
                let attempt = calls_for_rpc.fetch_add(1, AtomicOrdering::SeqCst);
                if attempt == 0 { Ok(Vec::new()) } else { Ok(logs.clone()) }
            });

        let indexer_state = IndexerState::default();
        let (tip_tx, tip_rx) = watch::channel(Some(sample_tip(Some(10))));
        let (round_tx, mut round_rx) = watch::channel(0);
        let task = spawn_verification(
            Arc::new(rpc),
            db,
            log_handler(Vec::new()),
            indexer_state.clone(),
            tip_rx,
            None,
            round_tx,
        );

        wait_for_round(&mut round_rx, 1).await;
        tip_tx.send_replace(Some(sample_tip(Some(12))));
        wait_for_round(&mut round_rx, 2).await;

        assert_eq!(calls.load(AtomicOrdering::SeqCst), 2, "the range is fetched twice");
        assert!(
            !indexer_state.verification_failed(),
            "an empty first response is not a mismatch"
        );
        assert_eq!(
            observed.get_last_verified_block(None).await?,
            Some(12),
            "the re-check matched, so the cursor advances"
        );
        drop(tip_tx);
        task.await?;

        Ok(())
    }

    /// A second comparison that also differs halts the indexer.
    #[tokio::test]
    async fn a_mismatch_confirmed_by_a_second_fetch_keeps_the_cursor() -> anyhow::Result<()> {
        let stored = vec![sample_log(11, 0, 1, 2), sample_log(12, 0, 3, 4)];
        let db = prepare_db(12, stored).await?;
        let observed = db.clone();
        let calls = Arc::new(AtomicUsize::new(0));
        let calls_for_rpc = calls.clone();

        let mut rpc = MockVerificationRpc::new();
        rpc.expect_get_logs_for_block_range().returning(move |_, _, _| {
            calls_for_rpc.fetch_add(1, AtomicOrdering::SeqCst);
            Ok(vec![sample_log(11, 0, 1, 2)])
        });

        let indexer_state = IndexerState::default();
        let (tip_tx, tip_rx) = watch::channel(Some(sample_tip(Some(10))));
        let (round_tx, mut round_rx) = watch::channel(0);
        let task = spawn_verification(
            Arc::new(rpc),
            db,
            log_handler(Vec::new()),
            indexer_state.clone(),
            tip_rx,
            None,
            round_tx,
        );

        wait_for_round(&mut round_rx, 1).await;
        tip_tx.send_replace(Some(sample_tip(Some(12))));

        tokio::time::timeout(Duration::from_secs(1), task)
            .await
            .expect("the task should stop after the confirmed mismatch")?;

        assert_eq!(calls.load(AtomicOrdering::SeqCst), 2);
        assert!(indexer_state.verification_failed());
        assert_eq!(
            observed.get_last_verified_block(None).await?,
            Some(10),
            "a mismatching range does not advance the stored position"
        );

        Ok(())
    }

    #[tokio::test]
    async fn rpc_error_keeps_the_cursor_and_the_range_is_retried() -> anyhow::Result<()> {
        let logs = vec![sample_log(11, 0, 1, 2)];
        let db = prepare_db(11, logs.clone()).await?;
        let calls = Arc::new(AtomicUsize::new(0));
        let calls_for_rpc = calls.clone();

        let mut rpc = MockVerificationRpc::new();
        rpc.expect_get_logs_for_block_range()
            .withf(|_, from: &u64, to: &u64| *from == 11 && *to == 11)
            .returning(move |_, _, _| {
                let attempt = calls_for_rpc.fetch_add(1, AtomicOrdering::SeqCst);
                if attempt == 0 {
                    Err(RpcError::Other("rpc is down".to_string()))
                } else {
                    Ok(logs.clone())
                }
            });

        let indexer_state = IndexerState::default();
        let (tip_tx, tip_rx) = watch::channel(Some(sample_tip(Some(10))));
        let (round_tx, mut round_rx) = watch::channel(0);
        let task = spawn_verification(
            Arc::new(rpc),
            db,
            log_handler(Vec::new()),
            indexer_state.clone(),
            tip_rx,
            None,
            round_tx,
        );

        wait_for_round(&mut round_rx, 1).await;
        tip_tx.send_replace(Some(sample_tip(Some(11))));
        wait_for_round(&mut round_rx, 2).await;
        assert_eq!(calls.load(AtomicOrdering::SeqCst), 1);
        assert!(!indexer_state.verification_failed());

        tip_tx.send_replace(Some(sample_tip(Some(11))));
        wait_for_round(&mut round_rx, 3).await;

        assert_eq!(
            calls.load(AtomicOrdering::SeqCst),
            2,
            "the same range should be retried"
        );
        assert!(!indexer_state.verification_failed(), "an RPC error is not a mismatch");
        drop(tip_tx);
        task.await?;

        Ok(())
    }

    #[tokio::test]
    async fn canonical_logs_from_a_safe_discovered_in_the_verified_block_are_not_missing_from_store()
    -> anyhow::Result<()> {
        let base_address = Address::from([0x11; 20]);
        let safe_address = Address::from([0x33; 20]);
        let base_log = sample_log(11, 0, 1, 2);
        let db = prepare_db(11, vec![base_log.clone()]).await?;
        // The Safe becomes known only after the block's logs were stored, as the indexer does when
        // it discovers a Safe in a live block.
        db.create_safe_contract(None, safe_address, Address::default(), Address::default(), 11, 0, 1)
            .await?;

        let safe_log = SerializableLog {
            address: safe_address,
            ..sample_log(11, 1, 1, 2)
        };
        let canonical = vec![base_log, safe_log];
        let calls = Arc::new(AtomicUsize::new(0));
        let calls_for_rpc = calls.clone();

        let mut rpc = MockVerificationRpc::new();
        rpc.expect_get_logs_for_block_range()
            .withf(|_, from: &u64, to: &u64| *from == 11 && *to == 11)
            .returning(move |_, _, _| {
                calls_for_rpc.fetch_add(1, AtomicOrdering::SeqCst);
                Ok(canonical.clone())
            });

        let indexer_state = IndexerState::default();
        // The block was indexed by the continuous phase, so only the discovery boundary exempts it.
        indexer_state.set_continuous_start_block(1);
        let (tip_tx, tip_rx) = watch::channel(Some(sample_tip(Some(10))));
        let (round_tx, mut round_rx) = watch::channel(0);
        let task = spawn_verification(
            Arc::new(rpc),
            db,
            log_handler(vec![base_address]),
            indexer_state.clone(),
            tip_rx,
            None,
            round_tx,
        );

        wait_for_round(&mut round_rx, 1).await;
        tip_tx.send_replace(Some(sample_tip(Some(11))));
        wait_for_round(&mut round_rx, 2).await;

        assert!(
            !indexer_state.verification_failed(),
            "a Safe discovered in the verified block must not halt the indexer"
        );
        assert_eq!(calls.load(AtomicOrdering::SeqCst), 1);
        drop(tip_tx);
        task.await?;

        Ok(())
    }

    /// A canonical log of a Safe that was in the filter long before the block must be in the store.
    #[tokio::test]
    async fn canonical_log_of_a_known_safe_in_a_continuous_block_halts_the_indexer() -> anyhow::Result<()> {
        let base_address = Address::from([0x11; 20]);
        let safe_address = Address::from([0x33; 20]);
        let base_log = sample_log(11, 0, 1, 2);
        let db = prepare_db(11, vec![base_log.clone()]).await?;
        db.create_safe_contract(None, safe_address, Address::default(), Address::default(), 10, 0, 0)
            .await?;

        let canonical = vec![
            base_log,
            SerializableLog {
                address: safe_address,
                ..sample_log(11, 1, 1, 2)
            },
        ];
        let mut rpc = MockVerificationRpc::new();
        rpc.expect_get_logs_for_block_range()
            .withf(|_, from: &u64, to: &u64| *from == 11 && *to == 11)
            .returning(move |_, _, _| Ok(canonical.clone()));

        let indexer_state = IndexerState::default();
        indexer_state.set_continuous_start_block(1);
        let (tip_tx, tip_rx) = watch::channel(Some(sample_tip(Some(10))));
        let (round_tx, mut round_rx) = watch::channel(0);
        let task = spawn_verification(
            Arc::new(rpc),
            db,
            log_handler(vec![base_address]),
            indexer_state.clone(),
            tip_rx,
            None,
            round_tx,
        );

        wait_for_round(&mut round_rx, 1).await;
        tip_tx.send_replace(Some(sample_tip(Some(11))));

        tokio::time::timeout(Duration::from_secs(1), task)
            .await
            .expect("the task should stop after a mismatch")?;

        assert!(
            indexer_state.verification_failed(),
            "a missing log of a known Safe is a mismatch"
        );

        Ok(())
    }

    /// Blocks that the historical discovery phase indexed hold no Safe logs at all.
    #[tokio::test]
    async fn canonical_logs_of_a_safe_below_the_continuous_start_are_not_missing_from_store() -> anyhow::Result<()> {
        let base_address = Address::from([0x11; 20]);
        let safe_address = Address::from([0x33; 20]);
        let base_log = sample_log(11, 0, 1, 2);
        let db = prepare_db(11, vec![base_log.clone()]).await?;
        db.create_safe_contract(None, safe_address, Address::default(), Address::default(), 5, 0, 0)
            .await?;

        let canonical = vec![
            base_log,
            SerializableLog {
                address: safe_address,
                ..sample_log(11, 1, 1, 2)
            },
        ];
        let mut rpc = MockVerificationRpc::new();
        rpc.expect_get_logs_for_block_range()
            .withf(|_, from: &u64, to: &u64| *from == 11 && *to == 11)
            .returning(move |_, _, _| Ok(canonical.clone()));

        let indexer_state = IndexerState::default();
        indexer_state.set_continuous_start_block(100);
        let (tip_tx, tip_rx) = watch::channel(Some(sample_tip(Some(10))));
        let (round_tx, mut round_rx) = watch::channel(0);
        let task = spawn_verification(
            Arc::new(rpc),
            db,
            log_handler(vec![base_address]),
            indexer_state.clone(),
            tip_rx,
            None,
            round_tx,
        );

        wait_for_round(&mut round_rx, 1).await;
        tip_tx.send_replace(Some(sample_tip(Some(11))));
        wait_for_round(&mut round_rx, 2).await;

        assert!(!indexer_state.verification_failed());
        drop(tip_tx);
        task.await?;

        Ok(())
    }

    /// A cursor stored by an earlier process resumes the blocks that became final while it was down.
    #[tokio::test]
    async fn stored_cursor_below_the_finalized_block_resumes_the_range_in_between() -> anyhow::Result<()> {
        let logs = vec![sample_log(101, 0, 1, 2), sample_log(110, 0, 3, 4)];
        let db = prepare_db(110, logs.clone()).await?;
        db.set_last_verified_block(None, 100).await?;
        let observed = db.clone();

        let mut rpc = MockVerificationRpc::new();
        rpc.expect_get_logs_for_block_range()
            .withf(|_, from: &u64, to: &u64| *from == 101 && *to == 110)
            .returning(move |_, _, _| Ok(logs.clone()));

        let indexer_state = IndexerState::default();
        let (tip_tx, tip_rx) = watch::channel(Some(sample_tip(Some(110))));
        let (round_tx, mut round_rx) = watch::channel(0);
        let task = spawn_verification(
            Arc::new(rpc),
            db,
            log_handler(Vec::new()),
            indexer_state.clone(),
            tip_rx,
            None,
            round_tx,
        );

        wait_for_round(&mut round_rx, 1).await;

        assert_eq!(
            observed.get_last_verified_block(None).await?,
            Some(110),
            "the range between the stored cursor and the finalized block is verified and stored"
        );
        assert!(!indexer_state.verification_failed());
        drop(tip_tx);
        task.await?;

        Ok(())
    }

    /// Blocks indexed while finality was unknown are verified once it appears.
    #[tokio::test]
    async fn blocks_indexed_while_finality_was_unknown_are_verified() -> anyhow::Result<()> {
        let logs = vec![sample_log(101, 0, 1, 2), sample_log(110, 0, 3, 4)];
        let db = prepare_db(100, logs.clone()).await?;
        let observed = db.clone();

        let mut rpc = MockVerificationRpc::new();
        rpc.expect_get_logs_for_block_range()
            .withf(|_, from: &u64, to: &u64| *from == 101 && *to == 110)
            .returning(move |_, _, _| Ok(logs.clone()));

        let indexer_state = IndexerState::default();
        let (tip_tx, tip_rx) = watch::channel(Some(sample_tip(None)));
        let (round_tx, mut round_rx) = watch::channel(0);
        let task = spawn_verification(
            Arc::new(rpc),
            db,
            log_handler(Vec::new()),
            indexer_state.clone(),
            tip_rx,
            Some(100),
            round_tx,
        );

        wait_for_round(&mut round_rx, 1).await;
        assert_eq!(
            observed.get_last_verified_block(None).await?,
            None,
            "nothing is stored while the cursor is unset"
        );

        // The indexer advances while finality stays unknown.
        observed.set_indexer_state_info(None, 110).await?;
        tip_tx.send_replace(Some(sample_tip(None)));
        wait_for_round(&mut round_rx, 2).await;

        // Finality appears above everything indexed meanwhile, so the cursor starts at the block
        // the first round saw and the blocks above it are verified.
        tip_tx.send_replace(Some(sample_tip(Some(110))));
        wait_for_round(&mut round_rx, 3).await;
        assert_eq!(observed.get_last_verified_block(None).await?, Some(100));

        tip_tx.send_replace(Some(sample_tip(Some(110))));
        wait_for_round(&mut round_rx, 4).await;

        assert_eq!(observed.get_last_verified_block(None).await?, Some(110));
        assert!(!indexer_state.verification_failed());
        drop(tip_tx);
        task.await?;

        Ok(())
    }

    /// An empty index does not pull the cursor down to the genesis block.
    #[tokio::test]
    async fn empty_index_without_finality_starts_the_cursor_at_the_first_finalized_block() -> anyhow::Result<()> {
        let db = prepare_db(0, Vec::new()).await?;
        let observed = db.clone();

        // No expectation is set: a fetched range would fail the test.
        let rpc = MockVerificationRpc::new();

        let indexer_state = IndexerState::default();
        let (tip_tx, tip_rx) = watch::channel(Some(sample_tip(None)));
        let (round_tx, mut round_rx) = watch::channel(0);
        let task = spawn_verification(
            Arc::new(rpc),
            db,
            log_handler(Vec::new()),
            indexer_state.clone(),
            tip_rx,
            None,
            round_tx,
        );

        wait_for_round(&mut round_rx, 1).await;

        tip_tx.send_replace(Some(sample_tip(Some(50))));
        wait_for_round(&mut round_rx, 2).await;
        assert_eq!(observed.get_last_verified_block(None).await?, Some(50));

        tip_tx.send_replace(Some(sample_tip(Some(50))));
        wait_for_round(&mut round_rx, 3).await;

        assert_eq!(observed.get_last_verified_block(None).await?, Some(50));
        assert!(!indexer_state.verification_failed());
        drop(tip_tx);
        task.await?;

        Ok(())
    }

    /// A failed cursor write keeps the stored position and re-verifies the same range.
    #[tokio::test]
    async fn cursor_is_not_advanced_when_the_store_write_fails() -> anyhow::Result<()> {
        let logs = vec![sample_log(11, 0, 1, 2)];
        let db = prepare_db(11, logs.clone()).await?;
        db.set_last_verified_block(None, 10).await?;
        let observed = db.clone();

        // A trigger fails only the cursor write, so the store stays readable.
        db.conn(TargetDb::Index)
            .execute_unprepared(
                "CREATE TRIGGER fail_last_verified_block_write BEFORE UPDATE OF last_verified_block ON chain_info \
                 BEGIN SELECT RAISE(ABORT, 'injected cursor write failure'); END",
            )
            .await?;

        let calls = Arc::new(AtomicUsize::new(0));
        let calls_for_rpc = calls.clone();

        let mut rpc = MockVerificationRpc::new();
        rpc.expect_get_logs_for_block_range()
            .withf(|_, from: &u64, to: &u64| *from == 11 && *to == 11)
            .returning(move |_, _, _| {
                calls_for_rpc.fetch_add(1, AtomicOrdering::SeqCst);
                Ok(logs.clone())
            });

        let indexer_state = IndexerState::default();
        let (tip_tx, tip_rx) = watch::channel(Some(sample_tip(Some(10))));
        let (round_tx, mut round_rx) = watch::channel(0);
        let task = spawn_verification(
            Arc::new(rpc),
            db,
            log_handler(Vec::new()),
            indexer_state.clone(),
            tip_rx,
            None,
            round_tx,
        );

        wait_for_round(&mut round_rx, 1).await;
        tip_tx.send_replace(Some(sample_tip(Some(11))));
        wait_for_round(&mut round_rx, 2).await;

        assert_eq!(calls.load(AtomicOrdering::SeqCst), 1);
        assert_eq!(
            observed.get_last_verified_block(None).await?,
            Some(10),
            "the stored position must not move past a failed write"
        );

        tip_tx.send_replace(Some(sample_tip(Some(11))));
        wait_for_round(&mut round_rx, 3).await;

        assert_eq!(
            calls.load(AtomicOrdering::SeqCst),
            2,
            "the same range is verified again"
        );
        assert!(!indexer_state.verification_failed(), "a failed write is not a mismatch");
        drop(tip_tx);
        task.await?;

        Ok(())
    }

    #[tokio::test]
    async fn stored_logs_are_compared_for_the_verified_range_only() -> anyhow::Result<()> {
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

        // The range read that verification performs for `(10, 12]`.
        let stored = db.get_logs(Some(11), Some(12 - 11)).await?;
        assert_eq!(
            stored.iter().map(|log| log.block_number).collect::<Vec<_>>(),
            vec![11, 12],
            "the read must return exactly the verified range"
        );

        let canonical = vec![sample_log(11, 0, 3, 4), sample_log(12, 0, 5, 6)];
        let mismatch = compare_logs(&stored, &canonical, &sample_coverage());
        assert!(
            mismatch.is_empty(),
            "the stored log outside the range must not be compared: {mismatch:?}"
        );

        let canonical_without_to = vec![sample_log(11, 0, 3, 4)];
        let mismatch = compare_logs(&stored, &canonical_without_to, &sample_coverage());
        assert_eq!(mismatch.missing_from_chain.len(), 1);
        assert_eq!(mismatch.missing_from_chain[0].block_number, 12);

        Ok(())
    }
}
