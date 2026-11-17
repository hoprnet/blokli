//! Post-finality verification of indexed logs.
//!
//! A background task re-checks every block that was indexed before it was final once the chain tip
//! tracker publishes that the block is final. It re-runs `eth_getLogs` with the same filter set the
//! indexer uses and compares the result with the logs stored in the logs database on block number,
//! block hash, transaction hash and log index. Any difference means that the logs were indexed from
//! a chain that the canonical chain does not confirm, or that the canonical chain has logs that were
//! never indexed.
//!
//! A mismatch is reported once, counted in `blokli_fcr_false_confirmations_total`, and recorded in
//! the shared [`VerificationStatus`] flag. The flag is sticky: the indexer stops advancing and
//! readiness reports not ready until an operator restarts the process. A failing RPC or database
//! read is not a mismatch: the same range is retried on the next tip update.
//!
//! The set of Safe contract addresses grows at runtime, while the stored logs of a block only
//! contain the addresses known when that block was indexed. Canonical logs emitted by a Safe that
//! was discovered later are therefore never reported as missing from the store.

use std::{
    collections::{BTreeMap, BTreeSet},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
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
use tokio::sync::watch;
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

/// Compares the stored logs of a range with the canonical logs of the same range.
///
/// Logs are matched by their position within the chain, that is a block number and a log index.
/// Two logs at the same position differ when their block hash or their transaction hash differs,
/// which is what a reorganization produces. The event payload itself is not compared, because a
/// matching position and block hash pin down the payload.
///
/// `static_addresses` lists the contract addresses that do not change at runtime. A canonical log
/// is only reported as missing from the store when it was emitted by one of them: a log from a Safe
/// that was discovered after the block was indexed could not have been stored for that block.
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
    /// The cursor starts at the given finalized block; nothing indexed above it is verified yet.
    Initialised(u64),
    /// Verify the inclusive range and advance the cursor to `to` once it matches.
    Verify {
        /// First block of the range.
        from: u64,
        /// Last block of the range.
        to: u64,
    },
}

/// Plans the next post-finality verification round.
///
/// The cursor is initialised to the first finalized block observed after startup and only ever
/// advances over blocks that the indexer has already committed. `last_indexed` bounds the range
/// from above, so a finalized tag that moves faster than the indexer never asks for logs that were
/// not stored yet, and a round covers at most [`MAX_VERIFICATION_RANGE`] blocks. The cursor is only
/// advanced by the caller after the range matched, so a finalized block moving backwards never moves
/// it backwards.
pub fn next_cursor_step(cursor: Option<u64>, finalized: Option<u64>, last_indexed: u64) -> CursorStep {
    match (cursor, finalized) {
        (_, None) => CursorStep::Idle,
        (None, Some(finalized)) => CursorStep::Initialised(finalized),
        (Some(cursor), Some(finalized)) => {
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

fn record_read_failure(failure_streak: &mut u32, range: Option<(u64, u64)>, error: &str) {
    if *failure_streak == 0 {
        warn!(
            ?range,
            error, "post-finality verification read failed, retrying the same range on the next tip update"
        );
    } else {
        debug!(
            ?range,
            error, "post-finality verification read failed again, retrying the same range on the next tip update"
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
    // the missing-from-store direction.
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
/// The task runs until the tip publisher is dropped, the indexer shuts down or a mismatch is found.
/// `round_updates` publishes the number of completed rounds so tests can wait for a round instead
/// of sleeping; production passes `None`.
pub(crate) async fn run_verification<T, U, Db>(
    mut tip_updates: watch::Receiver<Option<ChainTip>>,
    rpc: &T,
    db: Db,
    logs_handler: Arc<U>,
    indexer_state: IndexerState,
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
    let mut failure_streak: u32 = 0;
    let mut rounds: u32 = 0;

    loop {
        // The committed block is read before the finalized one of this round: a block committed
        // while this round is being planned then falls into the next round instead of being
        // compared against a canonical set that was fetched before it existed.
        let last_indexed = match db.get_indexer_state_info(None).await {
            Ok(info) => Some(u64::from(info.latest_block_number)),
            Err(error) => {
                record_read_failure(&mut failure_streak, None, &error.to_string());
                None
            }
        };

        if let Some(last_indexed) = last_indexed {
            let finalized = tip_updates.borrow().and_then(|tip| tip.finalized);

            match next_cursor_step(cursor, finalized, last_indexed) {
                CursorStep::Idle => {}
                CursorStep::Initialised(finalized) => {
                    cursor = Some(finalized);
                    failure_streak = 0;
                    info!(
                        verified_up_to = finalized,
                        "post-finality verification started, blocks above the finalized block are re-checked once \
                         they become final"
                    );
                }
                CursorStep::Verify { from, to } => match verify_range(rpc, &db, &logs_handler, from, to).await {
                    Ok(mismatch) if mismatch.is_empty() => {
                        cursor = Some(to);
                        failure_streak = 0;
                        debug!(
                            from_block = from,
                            to_block = to,
                            "post-finality verification matched the stored logs"
                        );
                    }
                    Ok(mismatch) => {
                        error!(
                            from_block = from,
                            to_block = to,
                            missing_from_chain = mismatch.missing_from_chain.len(),
                            missing_from_store = mismatch.missing_from_store.len(),
                            changed = mismatch.changed.len(),
                            examples = ?describe_mismatch(&mismatch),
                            "post-finality verification found logs that the canonical chain does not confirm, \
                             halting the indexer"
                        );
                        record_false_confirmation();
                        indexer_state.mark_verification_failed();
                        return;
                    }
                    Err(error) => record_read_failure(&mut failure_streak, Some((from, to)), &error.to_string()),
                },
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
        api::logs::BlokliDbLogOperations, db::BlokliDb, info::BlokliDbInfoOperations,
        safe_contracts::BlokliDbSafeContractOperations,
    };
    use futures::Stream;
    use hopr_bindings::exports::alloy::{primitives::B256, rpc::types::Filter};
    use hopr_types::{
        crypto::types::Hash,
        primitive::prelude::{Address, HoprBalance, SerializableLog, XDaiBalance, XHoprBalance},
    };
    use mockall::mock;
    use tokio::sync::watch;

    use super::{CursorStep, compare_logs, next_cursor_step, run_verification};
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

    fn sample_static_addresses() -> BTreeSet<Address> {
        static_addresses(&[Address::from([0x11; 20])])
    }

    #[test]
    fn identical_log_sets_have_no_mismatch() {
        let logs = vec![sample_log(10, 0, 1, 2), sample_log(10, 1, 3, 4)];

        let mismatch = compare_logs(&logs, &logs, &sample_static_addresses());

        assert!(mismatch.is_empty());
        assert_eq!(mismatch.len(), 0);
    }

    #[test]
    fn empty_ranges_have_no_mismatch() {
        let mismatch = compare_logs(&[], &[], &sample_static_addresses());

        assert!(mismatch.is_empty());
    }

    #[test]
    fn changed_block_hash_is_reported() {
        let stored = vec![sample_log(10, 0, 1, 2)];
        let canonical = vec![sample_log(10, 0, 9, 2)];

        let mismatch = compare_logs(&stored, &canonical, &sample_static_addresses());

        insta::assert_yaml_snapshot!("changed_block_hash", mismatch);
    }

    #[test]
    fn changed_transaction_hash_is_reported() {
        let stored = vec![sample_log(10, 0, 1, 2)];
        let canonical = vec![sample_log(10, 0, 1, 9)];

        let mismatch = compare_logs(&stored, &canonical, &sample_static_addresses());

        insta::assert_yaml_snapshot!("changed_transaction_hash", mismatch);
    }

    #[test]
    fn stored_log_missing_from_chain_is_reported() {
        let stored = vec![sample_log(10, 0, 1, 2), sample_log(11, 0, 3, 4)];
        let canonical = vec![sample_log(10, 0, 1, 2)];

        let mismatch = compare_logs(&stored, &canonical, &sample_static_addresses());

        insta::assert_yaml_snapshot!("stored_log_missing_from_chain", mismatch);
    }

    #[test]
    fn canonical_log_missing_from_store_is_reported() {
        let stored = vec![sample_log(10, 0, 1, 2)];
        let canonical = vec![sample_log(10, 0, 1, 2), sample_log(11, 0, 3, 4)];

        let mismatch = compare_logs(&stored, &canonical, &sample_static_addresses());

        insta::assert_yaml_snapshot!("canonical_log_missing_from_store", mismatch);
    }

    #[test]
    fn canonical_log_from_a_safe_outside_the_static_set_is_not_missing_from_store() {
        let stored = vec![sample_log(10, 0, 1, 2)];
        let safe_log = SerializableLog {
            address: Address::from([0x33; 20]),
            ..sample_log(11, 0, 3, 4)
        };
        let canonical = vec![sample_log(10, 0, 1, 2), safe_log];

        let mismatch = compare_logs(&stored, &canonical, &sample_static_addresses());

        assert!(
            mismatch.is_empty(),
            "a Safe discovered after indexing must not produce a missing-from-store mismatch: {mismatch:?}"
        );
    }

    #[test]
    fn cursor_step_is_idle_while_finalized_is_unknown() {
        assert_eq!(next_cursor_step(None, None, 100), CursorStep::Idle);
        assert_eq!(next_cursor_step(Some(10), None, 100), CursorStep::Idle);
    }

    #[test]
    fn cursor_step_initialises_on_the_first_finalized_block() {
        assert_eq!(next_cursor_step(None, Some(42), 100), CursorStep::Initialised(42));
    }

    #[test]
    fn cursor_step_verifies_above_the_cursor() {
        assert_eq!(
            next_cursor_step(Some(10), Some(15), 100),
            CursorStep::Verify { from: 11, to: 15 }
        );
    }

    #[test]
    fn cursor_step_stays_idle_when_finalized_is_at_or_below_the_cursor() {
        assert_eq!(next_cursor_step(Some(10), Some(10), 100), CursorStep::Idle);
        assert_eq!(next_cursor_step(Some(10), Some(9), 100), CursorStep::Idle);
    }

    #[test]
    fn cursor_step_is_bounded_by_the_last_indexed_block() {
        assert_eq!(
            next_cursor_step(Some(10), Some(20), 15),
            CursorStep::Verify { from: 11, to: 15 }
        );
        assert_eq!(next_cursor_step(Some(10), Some(20), 10), CursorStep::Idle);
    }

    #[test]
    fn cursor_step_caps_one_round_at_the_maximum_range() {
        assert_eq!(
            next_cursor_step(Some(10), Some(5_000), 5_000),
            CursorStep::Verify { from: 11, to: 1_010 }
        );
        assert_eq!(
            next_cursor_step(Some(1_010), Some(5_000), 5_000),
            CursorStep::Verify { from: 1_011, to: 2_010 }
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
        round_updates: watch::Sender<u32>,
    ) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            run_verification(
                tip_updates,
                rpc.as_ref(),
                db,
                Arc::new(handler),
                indexer_state,
                Some(round_updates),
            )
            .await;
        })
    }

    #[tokio::test]
    async fn clean_range_advances_the_cursor_and_leaves_the_flag_unset() -> anyhow::Result<()> {
        let logs = vec![sample_log(11, 0, 1, 2), sample_log(12, 0, 3, 4)];
        let db = prepare_db(12, logs.clone()).await?;
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
            round_tx,
        );

        wait_for_round(&mut round_rx, 1).await;
        tip_tx.send_replace(Some(sample_tip(Some(12))));
        wait_for_round(&mut round_rx, 2).await;

        tip_tx.send_replace(Some(sample_tip(Some(12))));
        wait_for_round(&mut round_rx, 3).await;

        assert_eq!(calls.load(AtomicOrdering::SeqCst), 1, "the cursor should have advanced");
        assert!(!indexer_state.verification_failed());
        drop(tip_tx);
        task.await?;

        Ok(())
    }

    #[tokio::test]
    async fn mismatch_sets_the_flag_stops_the_task_and_keeps_the_cursor() -> anyhow::Result<()> {
        let stored = vec![sample_log(11, 0, 1, 2), sample_log(12, 0, 3, 4)];
        let db = prepare_db(12, stored).await?;
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
            1,
            "the task should stop after a mismatch"
        );
        assert!(indexer_state.verification_failed());

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
    async fn canonical_logs_from_a_safe_discovered_after_indexing_are_not_missing_from_store() -> anyhow::Result<()> {
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
        let (tip_tx, tip_rx) = watch::channel(Some(sample_tip(Some(10))));
        let (round_tx, mut round_rx) = watch::channel(0);
        let task = spawn_verification(
            Arc::new(rpc),
            db,
            log_handler(vec![base_address]),
            indexer_state.clone(),
            tip_rx,
            round_tx,
        );

        wait_for_round(&mut round_rx, 1).await;
        tip_tx.send_replace(Some(sample_tip(Some(11))));
        wait_for_round(&mut round_rx, 2).await;

        assert!(
            !indexer_state.verification_failed(),
            "a Safe discovered after indexing must not halt the indexer"
        );
        assert_eq!(calls.load(AtomicOrdering::SeqCst), 1);
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
        let mismatch = compare_logs(&stored, &canonical, &sample_static_addresses());
        assert!(
            mismatch.is_empty(),
            "the stored log outside the range must not be compared: {mismatch:?}"
        );

        let canonical_without_to = vec![sample_log(11, 0, 3, 4)];
        let mismatch = compare_logs(&stored, &canonical_without_to, &sample_static_addresses());
        assert_eq!(mismatch.missing_from_chain.len(), 1);
        assert_eq!(mismatch.missing_from_chain[0].block_number, 12);

        Ok(())
    }
}
