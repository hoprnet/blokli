//! Background refresh of Safe owners and thresholds from the chain.
//!
//! The refresh runs as its own task for the whole life of the indexer and performs RPC reads only: it
//! never opens a database transaction or runs a query, so it cannot hold a database lock that block
//! processing would wait for, however it is scheduled. The indexing loop owns all database access:
//!
//! * it reports the Safes that need a snapshot (`IndexerState::mark_safe_owner_snapshot_pending`) at the switch to
//!   continuous mode, and block processing reports Safes whose read at discovery failed,
//! * the task reads the reported Safes in batches (Multicall3 where deployed) and sends the results to the indexing
//!   loop, which stores them between blocks with [`store_safe_owner_snapshot`],
//! * a Safe that could not be read, or whose result could not be stored, is reported again, so it is retried with
//!   backoff (and with a long-tail interval for Safes that keep failing) until it succeeds.
//!
//! The load on the RPC node is bounded by the batch size and the number of concurrent batches. The RPC
//! client backs off after rate-limit responses (`RetryBackoffLayer`) but there is no limiter in front of
//! it, so these bounds are what keeps a refresh of many Safes from flooding the node.

use std::{
    collections::HashMap,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};

use blokli_chain_rpc::{HoprIndexerRpcOperations, SafeOwnersSnapshot};
use blokli_db::safe_history::BlokliDbSafeHistoryOperations;
use futures::{SinkExt, StreamExt, channel::mpsc::Sender};
use hopr_types::primitive::prelude::Address;
#[cfg(all(feature = "telemetry", not(test)))]
use hopr_types::telemetry::SimpleGauge;
use tokio::time::Instant;
use tracing::{debug, error, info, warn};

use crate::IndexerState;

/// Maximum number of Safes read in one batch (one Multicall3 call carries two sub-calls per Safe).
pub(crate) const SAFE_SNAPSHOT_BATCH_SIZE: usize = 50;

/// Maximum number of batches read concurrently.
pub(crate) const SAFE_SNAPSHOT_CONCURRENCY: usize = 2;

/// Wait before a Safe whose snapshot could not be stored is reported again.
const STORE_RETRY_DELAY: Duration = Duration::from_secs(5);

/// How many times a batch is re-pinned at a newer block after the node reports pruned state.
const SAFE_SNAPSHOT_REPIN_ATTEMPTS: usize = 2;

#[cfg(all(feature = "telemetry", not(test)))]
lazy_static::lazy_static! {
    static ref METRIC_SAFE_OWNER_SNAPSHOTS_PENDING: Option<SimpleGauge> =
        SimpleGauge::new(
            "blokli_indexer_safe_owner_snapshots_pending",
            "Number of Safes whose owner snapshot could not be read yet and is being retried",
    ).inspect_err(|error| error!(%error, metric = "blokli_indexer_safe_owner_snapshots_pending", "failed to register telemetry metric")).ok();
}

/// Retry timing of Safes that fail to read.
#[derive(Clone, Copy, Debug)]
pub(crate) struct RefreshTimings {
    /// Delay before the first retry; doubles per failure.
    pub retry_min: Duration,
    /// Upper bound of the doubling delay.
    pub retry_max: Duration,
    /// After this many consecutive failures a Safe is only retried every `long_tail`.
    pub long_tail_after: u32,
    /// Retry interval of Safes that keep failing (e.g. no code at the address, reverting `getOwners`).
    pub long_tail: Duration,
}

impl Default for RefreshTimings {
    fn default() -> Self {
        Self {
            retry_min: Duration::from_secs(1),
            retry_max: Duration::from_secs(60),
            long_tail_after: 10,
            long_tail: Duration::from_secs(15 * 60),
        }
    }
}

impl RefreshTimings {
    pub(crate) fn delay_after(&self, failures: u32) -> Duration {
        if failures >= self.long_tail_after {
            return self.long_tail;
        }
        let doublings = failures.saturating_sub(1).min(16);
        self.retry_min.saturating_mul(1 << doublings).min(self.retry_max)
    }
}

/// A Safe owner snapshot read from the chain, waiting to be stored by the indexing loop.
#[derive(Debug)]
pub(crate) struct SafeSnapshotRead {
    pub safe_address: Address,
    pub block: u64,
    pub snapshot: SafeOwnersSnapshot,
}

struct RetryState {
    failures: u32,
    next_attempt: Instant,
}

/// Reads the Safes reported through `state` and sends the results to `results`, forever.
///
/// `latest_processed_block` is the block every read of a round is pinned at (it is read again per
/// batch, so a long refresh does not outlast the state kept by a pruned node).
pub(crate) async fn run_safe_owner_refresh<T>(
    rpc: Arc<T>,
    state: IndexerState,
    latest_processed_block: Arc<AtomicU64>,
    mut results: Sender<SafeSnapshotRead>,
    timings: RefreshTimings,
) where
    T: HoprIndexerRpcOperations + Send + Sync + 'static,
{
    let mut waiting: HashMap<Address, RetryState> = HashMap::new();
    loop {
        // A Safe reported again starts over: whatever failed before is superseded by the new report.
        for safe_address in state.take_pending_safe_owner_snapshots() {
            waiting.insert(
                safe_address,
                RetryState {
                    failures: 0,
                    next_attempt: Instant::now(),
                },
            );
        }

        let now = Instant::now();
        let due = waiting
            .iter()
            .filter(|(_, retry)| retry.next_attempt <= now)
            .map(|(safe_address, _)| *safe_address)
            .collect::<Vec<_>>();

        if !due.is_empty() {
            info!(safes = due.len(), "Reading Safe owners and thresholds from the chain");
            let mut batches = futures::stream::iter(due.chunks(SAFE_SNAPSHOT_BATCH_SIZE).map(<[Address]>::to_vec))
                .map(|batch| read_batch(rpc.as_ref(), latest_processed_block.as_ref(), batch))
                .buffer_unordered(SAFE_SNAPSHOT_CONCURRENCY);
            while let Some(reads) = batches.next().await {
                for read in reads {
                    // From here on the indexing loop owns the Safe: it reports it again if storing fails.
                    waiting.remove(&read.safe_address);
                    if results.send(read).await.is_err() {
                        // The indexing loop is gone.
                        return;
                    }
                }
            }
            drop(batches);

            let now = Instant::now();
            for safe_address in due {
                if let Some(retry) = waiting.get_mut(&safe_address) {
                    retry.failures += 1;
                    retry.next_attempt = now + timings.delay_after(retry.failures);
                    if retry.failures == 1 {
                        warn!(%safe_address, "failed to read Safe owners, retrying with backoff");
                    } else {
                        debug!(%safe_address, failures = retry.failures, "Safe owner read still failing");
                    }
                }
            }
        }

        #[cfg(all(feature = "telemetry", not(test)))]
        if let Some(metric) = METRIC_SAFE_OWNER_SNAPSHOTS_PENDING.as_ref() {
            metric.set(waiting.len() as f64);
        }

        let next_retry = waiting.values().map(|retry| retry.next_attempt).min();
        tokio::select! {
            () = state.safe_owner_snapshot_reported() => {}
            () = async {
                match next_retry {
                    Some(at) => tokio::time::sleep_until(at).await,
                    None => std::future::pending().await,
                }
            } => {}
        }
    }
}

/// Reads one batch of Safes pinned at the latest processed block.
///
/// When the node no longer has the state of the pinned block (pruned or non-archive node, long
/// refresh), the batch is re-pinned at the then-latest block and retried. Safes that cannot be read
/// are left out of the result.
async fn read_batch<T>(rpc: &T, latest_processed_block: &AtomicU64, batch: Vec<Address>) -> Vec<SafeSnapshotRead>
where
    T: HoprIndexerRpcOperations + Sync,
{
    let mut attempt = 0;
    loop {
        let block = latest_processed_block.load(Ordering::Relaxed);
        match rpc.get_safe_owners_and_thresholds(batch.clone(), block).await {
            Ok(snapshots) if snapshots.len() == batch.len() => {
                return batch
                    .into_iter()
                    .zip(snapshots)
                    .filter_map(|(safe_address, snapshot)| match snapshot {
                        Ok(snapshot) => Some(SafeSnapshotRead {
                            safe_address,
                            block,
                            snapshot,
                        }),
                        Err(error) => {
                            debug!(%safe_address, %error, "Safe owner read failed");
                            None
                        }
                    })
                    .collect();
            }
            Ok(snapshots) => {
                warn!(
                    expected = batch.len(),
                    got = snapshots.len(),
                    "Safe owner batch returned an unexpected number of results"
                );
                return Vec::new();
            }
            Err(error) if error.is_state_pruned() && attempt < SAFE_SNAPSHOT_REPIN_ATTEMPTS => {
                attempt += 1;
                warn!(%error, block, attempt, "node no longer has the pinned state, re-pinning the batch");
            }
            Err(error) => {
                warn!(%error, safes = batch.len(), block, "failed to read a batch of Safe owners");
                return Vec::new();
            }
        }
    }
}

/// Stores a Safe owner snapshot read by the refresh task (called by the indexing loop, between blocks).
///
/// * A read pinned before an owner or threshold event that block processing handled since is dropped: it does not
///   contain the event, and the Safe is reported again to be read at a newer block.
/// * A database error is logged and the Safe is reported again.
/// * `apply_safe_owner_snapshot` itself ignores a snapshot older than the stored one.
pub(crate) async fn store_safe_owner_snapshot<Db>(db: &Db, state: &IndexerState, read: SafeSnapshotRead)
where
    Db: BlokliDbSafeHistoryOperations + Sync,
{
    let SafeSnapshotRead {
        safe_address,
        block,
        snapshot,
    } = read;

    if state
        .safe_owner_event_block(safe_address)
        .is_some_and(|event_block| event_block > block)
    {
        debug!(%safe_address, block, "dropping a Safe owner read that predates a processed event");
        state.mark_safe_owner_snapshot_pending(safe_address);
        return;
    }

    match db
        .apply_safe_owner_snapshot(
            None,
            safe_address,
            snapshot.owners,
            snapshot.threshold.to_string(),
            block,
        )
        .await
    {
        Ok(()) => state.clear_safe_owner_events_up_to(safe_address, block),
        Err(error) => {
            error!(%safe_address, %error, "failed to store Safe owner snapshot, retrying shortly");
            // Not immediately: a persistent database error must not turn into a hot read loop.
            let state = state.clone();
            tokio::spawn(async move {
                tokio::time::sleep(STORE_RETRY_DELAY).await;
                state.mark_safe_owner_snapshot_pending(safe_address);
            });
        }
    }
}
