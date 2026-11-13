//! In-process caches for expensive read-path queries.
//!
//! Every cache is private to one process: with several API replicas each one keeps its own copy.

use std::{fmt::Display, future::Future, time::Duration};

use blokli_chain_rpc::Eip1559FeeEstimation;
use moka::future::Cache;

/// Upper bound on a single RPC fee lookup, so a hung provider cannot stall every request waiting on the cache entry.
pub const FEE_RPC_TIMEOUT: Duration = Duration::from_secs(5);

/// Caches the raw RPC fee estimates exposed by `chainInfo` for roughly one block.
///
/// Concurrent misses are coalesced into a single RPC call. Only successful lookups are stored, so a failure is
/// retried by the next request. Values are stored as returned by the RPC: gas multiplier and fee floors are applied
/// by the caller afterwards.
#[derive(Clone)]
pub struct FeeEstimateCache {
    gas_price: Cache<(), u128>,
    eip1559: Cache<(), Eip1559FeeEstimation>,
    timeout: Duration,
}

impl FeeEstimateCache {
    /// Creates a cache whose entries live for `ttl` (typically the expected block time).
    pub fn new(ttl: Duration) -> Self {
        Self::with_timeout(ttl, FEE_RPC_TIMEOUT)
    }

    /// Creates a cache with an explicit RPC timeout.
    pub fn with_timeout(ttl: Duration, timeout: Duration) -> Self {
        Self {
            gas_price: Cache::builder().max_capacity(1).time_to_live(ttl).build(),
            eip1559: Cache::builder().max_capacity(1).time_to_live(ttl).build(),
            timeout,
        }
    }

    /// Returns the cached legacy gas price, or awaits `fetch` on a miss.
    pub async fn gas_price<F, E>(&self, fetch: F) -> Result<u128, String>
    where
        F: Future<Output = Result<u128, E>>,
        E: Display,
    {
        get_or_fetch(&self.gas_price, self.timeout, fetch).await
    }

    /// Returns the cached EIP-1559 fee estimate, or awaits `fetch` on a miss.
    pub async fn eip1559_fees<F, E>(&self, fetch: F) -> Result<Eip1559FeeEstimation, String>
    where
        F: Future<Output = Result<Eip1559FeeEstimation, E>>,
        E: Display,
    {
        get_or_fetch(&self.eip1559, self.timeout, fetch).await
    }
}

async fn get_or_fetch<V, F, E>(cache: &Cache<(), V>, timeout: Duration, fetch: F) -> Result<V, String>
where
    V: Clone + Send + Sync + 'static,
    F: Future<Output = Result<V, E>>,
    E: Display,
{
    cache
        .try_get_with((), async {
            match tokio::time::timeout(timeout, fetch).await {
                Ok(Ok(value)) => Ok(value),
                Ok(Err(e)) => Err(e.to_string()),
                Err(_) => Err(format!("timed out after {timeout:?}")),
            }
        })
        .await
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::*;

    const LONG_TTL: Duration = Duration::from_secs(60);

    async fn counted_price(calls: &AtomicUsize, value: u128) -> Result<u128, String> {
        calls.fetch_add(1, Ordering::SeqCst);
        tokio::time::sleep(Duration::from_millis(20)).await;
        Ok(value)
    }

    async fn counted_fees(calls: &AtomicUsize) -> Result<Eip1559FeeEstimation, String> {
        calls.fetch_add(1, Ordering::SeqCst);
        tokio::time::sleep(Duration::from_millis(20)).await;
        Ok(Eip1559FeeEstimation {
            max_fee_per_gas: 10,
            max_priority_fee_per_gas: 2,
        })
    }

    #[tokio::test]
    async fn gas_price_hit_does_not_refetch() {
        let cache = FeeEstimateCache::new(LONG_TTL);
        let calls = AtomicUsize::new(0);

        assert_eq!(cache.gas_price(counted_price(&calls, 7)).await, Ok(7));
        assert_eq!(cache.gas_price(counted_price(&calls, 8)).await, Ok(7));
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn concurrent_misses_share_one_fetch() {
        let cache = FeeEstimateCache::new(LONG_TTL);
        let price_calls = AtomicUsize::new(0);
        let fee_calls = AtomicUsize::new(0);

        let (prices, estimates) = tokio::join!(
            futures::future::join_all((0..16).map(|_| cache.gas_price(counted_price(&price_calls, 5)))),
            futures::future::join_all((0..16).map(|_| cache.eip1559_fees(counted_fees(&fee_calls)))),
        );

        assert!(prices.iter().all(|p| *p == Ok(5)));
        assert!(
            estimates
                .iter()
                .all(|e| e.as_ref().map(|f| f.max_fee_per_gas) == Ok(10))
        );
        assert_eq!(price_calls.load(Ordering::SeqCst), 1);
        assert_eq!(fee_calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn entry_expires_after_ttl() {
        let cache = FeeEstimateCache::new(Duration::from_millis(50));
        let calls = AtomicUsize::new(0);

        assert_eq!(cache.gas_price(counted_price(&calls, 1)).await, Ok(1));
        tokio::time::sleep(Duration::from_millis(120)).await;
        assert_eq!(cache.gas_price(counted_price(&calls, 2)).await, Ok(2));
        assert_eq!(calls.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn errors_are_not_cached() {
        let cache = FeeEstimateCache::new(LONG_TTL);

        let failed = cache.gas_price(async { Err::<u128, _>("rpc down") }).await;
        assert_eq!(failed, Err("rpc down".to_string()));

        assert_eq!(cache.gas_price(async { Ok::<_, String>(9) }).await, Ok(9));
    }

    #[tokio::test]
    async fn hung_fetch_times_out_and_is_not_cached() {
        let cache = FeeEstimateCache::with_timeout(LONG_TTL, Duration::from_millis(30));

        let hung = cache
            .gas_price(async {
                std::future::pending::<()>().await;
                Ok::<u128, String>(0)
            })
            .await;
        assert!(hung.is_err_and(|e| e.contains("timed out")));

        assert_eq!(cache.gas_price(async { Ok::<_, String>(3) }).await, Ok(3));
    }
}
