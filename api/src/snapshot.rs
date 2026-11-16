//! Admission control and sharing for the snapshots that subscriptions start with
//!
//! A subscription first emits the state at the moment it starts (its snapshot) and then follows
//! live events. Loading a snapshot is by far the most expensive thing a subscription does, and
//! clients tend to subscribe together: after a restart every client reconnects at once. Two
//! mechanisms keep such a burst from exhausting the database connection pool:
//!
//! - [`SnapshotLimiter`] bounds how many snapshots are loaded at the same time, so that a burst queues instead of
//!   taking every connection.
//! - [`SnapshotCache`] loads a snapshot once per [`SnapshotPoint`] and hands the same rows to every subscription that
//!   starts at that point.

use std::{
    future::Future,
    sync::{Arc, Mutex, PoisonError},
    time::Duration,
};

use async_graphql::Result;
use blokli_api_types::{Account, ChannelUpdate, QueryFailedError};
use futures::{
    FutureExt,
    future::{BoxFuture, Shared},
};
use tokio::sync::{OwnedSemaphorePermit, Semaphore};

use crate::{errors, metrics};

/// Label under which rejected snapshot loads are counted in `blokli_errors_total`
const SNAPSHOT_QUEUE_TIMEOUT_ERROR_TYPE: &str = "snapshot_queue_timeout";

/// Bounds how many subscription snapshots are loaded from the database at the same time
#[derive(Debug, Clone)]
pub struct SnapshotLimiter {
    permits: Arc<Semaphore>,
    queue_timeout: Duration,
}

/// The right to load one snapshot; the next queued load starts when it is dropped
#[derive(Debug)]
pub struct SnapshotPermit(#[allow(dead_code)] OwnedSemaphorePermit);

impl SnapshotLimiter {
    /// Creates a limiter that lets `max_concurrent` snapshots load at once and makes any further
    /// one wait up to `queue_timeout` for its turn.
    ///
    /// A limit of zero would admit nothing, so it is treated as one.
    pub fn new(max_concurrent: usize, queue_timeout: Duration) -> Self {
        Self {
            permits: Arc::new(Semaphore::new(max_concurrent.max(1))),
            queue_timeout,
        }
    }

    /// Waits for a turn to load a snapshot.
    ///
    /// # Errors
    ///
    /// Returns a timeout error if no turn became free within the queue timeout
    pub async fn acquire(&self) -> std::result::Result<SnapshotPermit, QueryFailedError> {
        match tokio::time::timeout(self.queue_timeout, self.permits.clone().acquire_owned()).await {
            Ok(Ok(permit)) => Ok(SnapshotPermit(permit)),
            // The semaphore is never closed, so only the timeout can end the wait without a permit
            Ok(Err(_)) | Err(_) => {
                metrics::increment_errors(SNAPSHOT_QUEUE_TIMEOUT_ERROR_TYPE);
                Err(errors::snapshot_queue_timed_out(self.queue_timeout))
            }
        }
    }
}

impl Default for SnapshotLimiter {
    fn default() -> Self {
        Self::new(8, Duration::from_secs(10))
    }
}

/// The point of the indexed chain a snapshot describes
///
/// Two subscriptions that start at the same point see the same state, which is what makes their
/// snapshot shareable. The whole watermark identifies a point: the block alone does not, because
/// after a reorganization the indexer reaches the same block number again with different content,
/// and within one block a snapshot can be taken after any transaction or log.
///
/// Points are ordered by epoch, then block, transaction index and log index, so a point after a
/// reorganization is always newer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct SnapshotPoint {
    /// Number of reorganizations the indexer had handled
    pub reorg_epoch: u64,
    /// Last indexed block
    pub block: i64,
    /// Last indexed transaction index within the block
    pub tx_index: i64,
    /// Last indexed log index within the transaction
    pub log_index: i64,
}

/// A snapshot load that any number of callers can wait for
type SharedLoad<T> = Shared<BoxFuture<'static, Result<Arc<Vec<T>>>>>;

/// The load of the newest point asked for, if any
type LatestLoad<T> = Option<(SnapshotPoint, SharedLoad<T>)>;

/// Loads a snapshot once per [`SnapshotPoint`] and shares its rows
///
/// Only the snapshot of the newest point is kept: subscriptions start at the current point, so
/// an older one is not asked for again.
#[derive(Clone)]
pub struct SnapshotCache<T> {
    latest: Arc<Mutex<LatestLoad<T>>>,
}

impl<T> Default for SnapshotCache<T> {
    fn default() -> Self {
        Self {
            latest: Arc::new(Mutex::new(None)),
        }
    }
}

impl<T> SnapshotCache<T>
where
    T: Send + Sync + 'static,
{
    /// Returns the snapshot at `point`, running `load` only if no load for that point exists yet.
    ///
    /// Callers that ask for the same point while it is loading wait for that one load. A failed
    /// load fails all of them and is forgotten, so the next caller loads again. A load that failed
    /// without a caller awaiting it is replaced as soon as the next caller asks for its point, so
    /// nobody receives an error that was never observed. A caller whose point is older than the
    /// cached one loads for itself and leaves the cache alone.
    ///
    /// The load runs to completion even if every caller goes away, so that it never sits
    /// half-finished on a database connection.
    ///
    /// # Errors
    ///
    /// Returns the error of the load
    pub async fn get_or_load<F>(&self, point: SnapshotPoint, load: F) -> Result<Arc<Vec<T>>>
    where
        F: Future<Output = Result<Vec<T>>> + Send + 'static,
    {
        let (shared, is_cached) = {
            let mut latest = self.latest.lock().unwrap_or_else(PoisonError::into_inner);
            match latest.as_ref() {
                Some((cached_point, shared)) if *cached_point == point => {
                    if matches!(shared.peek(), Some(Err(_))) {
                        let shared = Self::spawn_load(load);
                        *latest = Some((point, shared.clone()));
                        (shared, true)
                    } else {
                        (shared.clone(), true)
                    }
                }
                Some((cached_point, _)) if *cached_point > point => (Self::spawn_load(load), false),
                _ => {
                    let shared = Self::spawn_load(load);
                    *latest = Some((point, shared.clone()));
                    (shared, true)
                }
            }
        };

        let result = shared.clone().await;

        if result.is_err() && is_cached {
            let mut latest = self.latest.lock().unwrap_or_else(PoisonError::into_inner);
            if latest.as_ref().is_some_and(|(_, cached)| cached.ptr_eq(&shared)) {
                *latest = None;
            }
        }

        result
    }

    fn spawn_load<F>(load: F) -> SharedLoad<T>
    where
        F: Future<Output = Result<Vec<T>>> + Send + 'static,
    {
        let task = tokio::spawn(load);
        let shared = async move {
            match task.await {
                Ok(rows) => rows.map(Arc::new),
                Err(error) => Err(async_graphql::Error::new(errors::messages::internal_error(
                    "snapshot load",
                    error,
                ))),
            }
        }
        .boxed()
        .shared();
        // Keep the shared load polled to completion even when no caller waits for it, so that a
        // finished result is available to the cache.
        let _driver = tokio::spawn(shared.clone());
        shared
    }
}

/// Everything subscriptions share to load their snapshots: one limiter and one cache per
/// snapshot that many subscriptions ask for unfiltered
#[derive(Clone, Default)]
pub struct SubscriptionSnapshots {
    /// Limits concurrent snapshot loads
    pub limiter: SnapshotLimiter,
    /// Snapshot of all non-closed channels with their accounts
    pub channel_graph: SnapshotCache<ChannelUpdate>,
    /// Snapshot of all accounts
    pub accounts: SnapshotCache<Account>,
}

impl SubscriptionSnapshots {
    /// Creates the shared snapshot state with the given load limits and empty caches
    pub fn new(max_concurrent: usize, queue_timeout: Duration) -> Self {
        Self {
            limiter: SnapshotLimiter::new(max_concurrent, queue_timeout),
            channel_graph: SnapshotCache::default(),
            accounts: SnapshotCache::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use tokio::sync::Notify;

    use super::*;
    use crate::errors::codes;

    const POINT: SnapshotPoint = SnapshotPoint {
        reorg_epoch: 0,
        block: 100,
        tx_index: 0,
        log_index: 0,
    };

    /// A load that counts how often it ran and returns `rows`
    fn counted_load(loads: &Arc<AtomicUsize>, rows: Vec<u32>) -> impl Future<Output = Result<Vec<u32>>> + use<> {
        let loads = loads.clone();
        async move {
            loads.fetch_add(1, Ordering::SeqCst);
            tokio::task::yield_now().await;
            Ok(rows)
        }
    }

    #[tokio::test]
    async fn limiter_never_runs_more_loads_than_its_limit() {
        let limiter = SnapshotLimiter::new(2, Duration::from_secs(5));
        let running = Arc::new(AtomicUsize::new(0));
        let most_running = Arc::new(AtomicUsize::new(0));

        let tasks: Vec<_> = (0..10)
            .map(|_| {
                let (limiter, running, most_running) = (limiter.clone(), running.clone(), most_running.clone());
                tokio::spawn(async move {
                    let _permit = limiter.acquire().await.expect("a turn should become free");
                    let now_running = running.fetch_add(1, Ordering::SeqCst) + 1;
                    most_running.fetch_max(now_running, Ordering::SeqCst);
                    tokio::time::sleep(Duration::from_millis(5)).await;
                    running.fetch_sub(1, Ordering::SeqCst);
                })
            })
            .collect();
        for task in tasks {
            task.await.expect("load task should not panic");
        }

        assert_eq!(most_running.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn limiter_rejects_a_load_that_waited_for_the_whole_queue_timeout() {
        let limiter = SnapshotLimiter::new(1, Duration::from_millis(20));
        let _held = limiter.acquire().await.expect("the first load should be admitted");

        let error = limiter
            .acquire()
            .await
            .expect_err("the queue timeout should end the wait");

        assert_eq!(error.code, codes::TIMEOUT);
    }

    #[tokio::test]
    async fn limiter_admits_the_next_load_when_a_permit_is_dropped() {
        let limiter = SnapshotLimiter::new(1, Duration::from_millis(20));

        drop(limiter.acquire().await.expect("the first load should be admitted"));

        assert!(limiter.acquire().await.is_ok());
    }

    #[tokio::test]
    async fn limiter_with_a_limit_of_zero_still_admits_one_load() {
        let limiter = SnapshotLimiter::new(0, Duration::from_millis(20));

        assert!(limiter.acquire().await.is_ok());
    }

    #[tokio::test]
    async fn cache_loads_once_for_callers_at_the_same_point() -> anyhow::Result<()> {
        let cache = SnapshotCache::<u32>::default();
        let loads = Arc::new(AtomicUsize::new(0));

        let callers: Vec<_> = (0..20)
            .map(|_| {
                let (cache, loads) = (cache.clone(), loads.clone());
                tokio::spawn(async move { cache.get_or_load(POINT, counted_load(&loads, vec![1, 2, 3])).await })
            })
            .collect();
        for caller in callers {
            let rows = caller.await?.map_err(|e| anyhow::anyhow!(e.message))?;
            assert_eq!(*rows, vec![1, 2, 3]);
        }

        assert_eq!(loads.load(Ordering::SeqCst), 1);
        Ok(())
    }

    #[tokio::test]
    async fn cache_reuses_a_finished_load_at_the_same_point() {
        let cache = SnapshotCache::<u32>::default();
        let loads = Arc::new(AtomicUsize::new(0));

        let first = cache.get_or_load(POINT, counted_load(&loads, vec![1])).await;
        let second = cache.get_or_load(POINT, counted_load(&loads, vec![2])).await;

        assert_eq!(first.ok().as_deref(), Some(&vec![1]));
        assert_eq!(second.ok().as_deref(), Some(&vec![1]));
        assert_eq!(loads.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn cache_loads_again_at_a_newer_block() {
        let cache = SnapshotCache::<u32>::default();
        let loads = Arc::new(AtomicUsize::new(0));
        let newer = SnapshotPoint { block: 101, ..POINT };

        let first = cache.get_or_load(POINT, counted_load(&loads, vec![1])).await;
        let second = cache.get_or_load(newer, counted_load(&loads, vec![2])).await;

        assert_eq!(first.ok().as_deref(), Some(&vec![1]));
        assert_eq!(second.ok().as_deref(), Some(&vec![2]));
        assert_eq!(loads.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn cache_does_not_share_a_load_across_transactions_in_one_block() {
        let cache = SnapshotCache::<u32>::default();
        let loads = Arc::new(AtomicUsize::new(0));
        let later_transaction = SnapshotPoint {
            tx_index: POINT.tx_index + 1,
            ..POINT
        };

        let first = cache.get_or_load(POINT, counted_load(&loads, vec![1])).await;
        let second = cache
            .get_or_load(later_transaction, counted_load(&loads, vec![2]))
            .await;

        assert_eq!(first.ok().as_deref(), Some(&vec![1]));
        assert_eq!(second.ok().as_deref(), Some(&vec![2]));
        assert_eq!(loads.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn cache_does_not_share_a_load_across_logs_in_one_transaction() {
        let cache = SnapshotCache::<u32>::default();
        let loads = Arc::new(AtomicUsize::new(0));
        let later_log = SnapshotPoint {
            log_index: POINT.log_index + 1,
            ..POINT
        };

        let first = cache.get_or_load(POINT, counted_load(&loads, vec![1])).await;
        let second = cache.get_or_load(later_log, counted_load(&loads, vec![2])).await;

        assert_eq!(first.ok().as_deref(), Some(&vec![1]));
        assert_eq!(second.ok().as_deref(), Some(&vec![2]));
        assert_eq!(loads.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn cache_does_not_share_a_block_across_a_reorganization() {
        let cache = SnapshotCache::<u32>::default();
        let loads = Arc::new(AtomicUsize::new(0));
        // After a reorganization the indexer is back at an earlier block of the new fork
        let after_reorg = SnapshotPoint {
            reorg_epoch: 1,
            block: 90,
            ..POINT
        };
        let same_block_on_new_fork = SnapshotPoint {
            reorg_epoch: 1,
            block: 100,
            ..POINT
        };

        let old_fork = cache.get_or_load(POINT, counted_load(&loads, vec![1])).await;
        let rolled_back = cache.get_or_load(after_reorg, counted_load(&loads, vec![2])).await;
        let new_fork = cache
            .get_or_load(same_block_on_new_fork, counted_load(&loads, vec![3]))
            .await;

        assert_eq!(old_fork.ok().as_deref(), Some(&vec![1]));
        assert_eq!(rolled_back.ok().as_deref(), Some(&vec![2]));
        assert_eq!(new_fork.ok().as_deref(), Some(&vec![3]));
    }

    #[tokio::test]
    async fn cache_keeps_the_newer_snapshot_when_asked_for_an_older_point() {
        let cache = SnapshotCache::<u32>::default();
        let loads = Arc::new(AtomicUsize::new(0));
        let older = SnapshotPoint { block: 99, ..POINT };

        let newest = cache.get_or_load(POINT, counted_load(&loads, vec![1])).await;
        let late = cache.get_or_load(older, counted_load(&loads, vec![2])).await;
        let newest_again = cache.get_or_load(POINT, counted_load(&loads, vec![3])).await;

        assert_eq!(newest.ok().as_deref(), Some(&vec![1]));
        assert_eq!(late.ok().as_deref(), Some(&vec![2]));
        assert_eq!(newest_again.ok().as_deref(), Some(&vec![1]));
        assert_eq!(loads.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn cache_fails_every_waiter_of_a_failed_load_and_forgets_it() {
        let cache = SnapshotCache::<u32>::default();
        let loads = Arc::new(AtomicUsize::new(0));
        let release = Arc::new(Notify::new());

        let failing_load = |loads: &Arc<AtomicUsize>, release: &Arc<Notify>| {
            let (loads, release) = (loads.clone(), release.clone());
            async move {
                loads.fetch_add(1, Ordering::SeqCst);
                release.notified().await;
                Err(async_graphql::Error::new("connection pool timed out"))
            }
        };

        let first = tokio::spawn({
            let (cache, load) = (cache.clone(), failing_load(&loads, &release));
            async move { cache.get_or_load(POINT, load).await }
        });
        let second = tokio::spawn({
            let (cache, load) = (cache.clone(), failing_load(&loads, &release));
            async move { cache.get_or_load(POINT, load).await }
        });
        // Let both callers reach the shared load before it fails
        while loads.load(Ordering::SeqCst) == 0 {
            tokio::task::yield_now().await;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
        release.notify_waiters();

        let first = first.await.expect("caller should not panic");
        let second = second.await.expect("caller should not panic");
        assert_eq!(
            first.err().map(|e| e.message),
            Some("connection pool timed out".to_string())
        );
        assert_eq!(
            second.err().map(|e| e.message),
            Some("connection pool timed out".to_string())
        );
        assert_eq!(loads.load(Ordering::SeqCst), 1);

        let retried = cache.get_or_load(POINT, counted_load(&loads, vec![9])).await;
        assert_eq!(retried.ok().as_deref(), Some(&vec![9]));
        assert_eq!(loads.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn cache_finishes_a_load_whose_only_caller_went_away() {
        let cache = SnapshotCache::<u32>::default();
        let loads = Arc::new(AtomicUsize::new(0));
        let release = Arc::new(Notify::new());

        let caller = tokio::spawn({
            let (cache, loads, release) = (cache.clone(), loads.clone(), release.clone());
            async move {
                cache
                    .get_or_load(POINT, async move {
                        loads.fetch_add(1, Ordering::SeqCst);
                        release.notified().await;
                        Ok(vec![5])
                    })
                    .await
            }
        });
        while loads.load(Ordering::SeqCst) == 0 {
            tokio::task::yield_now().await;
        }
        caller.abort();
        assert!(caller.await.is_err());

        release.notify_waiters();
        let rows = cache.get_or_load(POINT, counted_load(&loads, vec![6])).await;

        assert_eq!(rows.ok().as_deref(), Some(&vec![5]));
        assert_eq!(loads.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn cache_loads_again_when_a_failed_load_was_never_awaited() -> anyhow::Result<()> {
        let cache = SnapshotCache::<u32>::default();
        let loads = Arc::new(AtomicUsize::new(0));
        let release = Arc::new(Notify::new());

        let caller = tokio::spawn({
            let (cache, loads, release) = (cache.clone(), loads.clone(), release.clone());
            async move {
                cache
                    .get_or_load(POINT, async move {
                        loads.fetch_add(1, Ordering::SeqCst);
                        release.notified().await;
                        Err(async_graphql::Error::new("connection pool timed out"))
                    })
                    .await
            }
        });
        while loads.load(Ordering::SeqCst) == 0 {
            tokio::task::yield_now().await;
        }
        caller.abort();
        assert!(caller.await.is_err());

        release.notify_one();
        // Let the spawned load finish while no caller waits for it
        tokio::time::sleep(Duration::from_millis(10)).await;

        let rows = cache.get_or_load(POINT, counted_load(&loads, vec![6])).await;

        assert_eq!(rows.ok().as_deref(), Some(&vec![6]));
        assert_eq!(loads.load(Ordering::SeqCst), 2);
        Ok(())
    }
}
