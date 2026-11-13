//! In-process caches for expensive read-path queries.
//!
//! Every cache is private to one process: with several API replicas each one keeps its own copy.

use std::{fmt::Display, future::Future, time::Duration};

use blokli_chain_rpc::Eip1559FeeEstimation;
use blokli_db_entity::conversions::channel_aggregation::AggregatedChannelStats;
use moka::future::Cache;
use sea_orm::DbErr;

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

/// Maximum number of distinct filter tuples kept per process.
const CHANNEL_STATS_MAX_ENTRIES: u64 = 1024;

/// Safety bound on how long a `channelStats` entry may be served.
///
/// Entries are normally replaced because the indexed block advances (it is part of the key). The bound only covers
/// the case where channel rows change without `last_indexed_block` moving, such as during historical sync, and
/// keeps entries of old blocks from lingering.
const CHANNEL_STATS_MAX_AGE: Duration = Duration::from_secs(60);

/// Full filter tuple of a `channelStats` query together with the indexed block the result was computed for.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ChannelStatsKey {
    /// `chain_info.last_indexed_block` read before the aggregate was computed.
    pub block: i64,
    pub source_key_id: Option<i64>,
    pub destination_key_id: Option<i64>,
    /// Concrete channel id without `0x` prefix, exactly as used by the query.
    pub concrete_channel_id: Option<String>,
    pub status: Option<i16>,
    /// Safe address bytes.
    pub safe_address: Option<Vec<u8>>,
}

/// Caches `channelStats` aggregates per filter tuple and indexed block.
///
/// A result is only reused while `last_indexed_block` is unchanged. Concurrent misses for the same key share one
/// database scan and failures are not stored.
#[derive(Clone)]
pub struct ChannelStatsCache {
    cache: Cache<ChannelStatsKey, AggregatedChannelStats>,
}

impl Default for ChannelStatsCache {
    fn default() -> Self {
        Self::new(CHANNEL_STATS_MAX_AGE)
    }
}

impl ChannelStatsCache {
    /// Creates a cache with an explicit maximum entry age.
    pub fn new(max_age: Duration) -> Self {
        Self {
            cache: Cache::builder()
                .max_capacity(CHANNEL_STATS_MAX_ENTRIES)
                .time_to_live(max_age)
                .build(),
        }
    }

    /// Returns the cached aggregate for `key`, or awaits `fetch` on a miss.
    pub async fn get_or_fetch<F>(&self, key: ChannelStatsKey, fetch: F) -> Result<AggregatedChannelStats, DbErr>
    where
        F: Future<Output = Result<AggregatedChannelStats, DbErr>>,
    {
        self.cache
            .try_get_with(key, fetch)
            .await
            .map_err(|e| DbErr::Custom(e.to_string()))
    }
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

    fn stats(count: u64) -> AggregatedChannelStats {
        AggregatedChannelStats {
            count,
            balance: Default::default(),
        }
    }

    fn key(block: i64) -> ChannelStatsKey {
        ChannelStatsKey {
            block,
            source_key_id: None,
            destination_key_id: None,
            concrete_channel_id: None,
            status: None,
            safe_address: None,
        }
    }

    async fn counted_stats(calls: &AtomicUsize, count: u64) -> Result<AggregatedChannelStats, DbErr> {
        calls.fetch_add(1, Ordering::SeqCst);
        tokio::time::sleep(Duration::from_millis(20)).await;
        Ok(stats(count))
    }

    #[tokio::test]
    async fn stats_hit_reuses_value_for_same_block_and_filters() {
        let cache = ChannelStatsCache::default();
        let calls = AtomicUsize::new(0);

        assert_eq!(
            cache
                .get_or_fetch(key(1), counted_stats(&calls, 1))
                .await
                .unwrap()
                .count,
            1
        );
        assert_eq!(
            cache
                .get_or_fetch(key(1), counted_stats(&calls, 2))
                .await
                .unwrap()
                .count,
            1
        );
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn stats_advanced_block_misses() {
        let cache = ChannelStatsCache::default();
        let calls = AtomicUsize::new(0);

        assert_eq!(
            cache
                .get_or_fetch(key(1), counted_stats(&calls, 1))
                .await
                .unwrap()
                .count,
            1
        );
        assert_eq!(
            cache
                .get_or_fetch(key(2), counted_stats(&calls, 2))
                .await
                .unwrap()
                .count,
            2
        );
        assert_eq!(calls.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn stats_each_filter_field_has_its_own_entry() {
        let cache = ChannelStatsCache::default();
        let calls = AtomicUsize::new(0);

        let variants = [
            key(1),
            ChannelStatsKey {
                source_key_id: Some(1),
                ..key(1)
            },
            ChannelStatsKey {
                destination_key_id: Some(1),
                ..key(1)
            },
            ChannelStatsKey {
                concrete_channel_id: Some("ab".into()),
                ..key(1)
            },
            ChannelStatsKey {
                status: Some(1),
                ..key(1)
            },
            ChannelStatsKey {
                safe_address: Some(vec![1; 20]),
                ..key(1)
            },
            ChannelStatsKey {
                source_key_id: Some(1),
                status: Some(1),
                ..key(1)
            },
        ];
        for (i, k) in variants.iter().enumerate() {
            let got = cache
                .get_or_fetch(k.clone(), counted_stats(&calls, i as u64))
                .await
                .unwrap();
            assert_eq!(got.count, i as u64);
        }
        assert_eq!(calls.load(Ordering::SeqCst), variants.len());
    }

    #[tokio::test]
    async fn stats_concurrent_misses_share_one_fetch() {
        let cache = ChannelStatsCache::default();
        let calls = AtomicUsize::new(0);

        let results =
            futures::future::join_all((0..16).map(|_| cache.get_or_fetch(key(1), counted_stats(&calls, 4)))).await;

        assert!(results.iter().all(|r| r.as_ref().is_ok_and(|s| s.count == 4)));
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn stats_errors_are_not_cached() {
        let cache = ChannelStatsCache::default();

        let failed = cache
            .get_or_fetch(key(1), async { Err(DbErr::Custom("db down".into())) })
            .await;
        assert!(failed.is_err());

        assert_eq!(
            cache.get_or_fetch(key(1), async { Ok(stats(6)) }).await.unwrap().count,
            6
        );
    }

    #[tokio::test]
    async fn stats_entry_expires_after_max_age() {
        let cache = ChannelStatsCache::new(Duration::from_millis(50));
        let calls = AtomicUsize::new(0);

        assert_eq!(
            cache
                .get_or_fetch(key(1), counted_stats(&calls, 1))
                .await
                .unwrap()
                .count,
            1
        );
        tokio::time::sleep(Duration::from_millis(120)).await;
        assert_eq!(
            cache
                .get_or_fetch(key(1), counted_stats(&calls, 2))
                .await
                .unwrap()
                .count,
            2
        );
    }
}
