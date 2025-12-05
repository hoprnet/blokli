//! Extends the [RpcOperations] type with functionality needed by the Indexer component.
//!
//! The functionality required functionality is defined in the [HoprIndexerRpcOperations] trait,
//! which is implemented for [RpcOperations] hereof.
//! The primary goal is to provide a stream of [BlockWithLogs] filtered by the given [LogFilter]
//! as the new matching blocks are mined in the underlying blockchain. The stream also allows to collect
//! historical blockchain data.
//!
//! For details on the Indexer see the `chain-indexer` crate.
use std::pin::Pin;

use alloy::{primitives::B256, providers::Provider, rpc::types::Filter};
use async_stream::stream;
use async_trait::async_trait;
use blokli_chain_types::AlloyAddressExt;
use futures::{Stream, StreamExt, stream::BoxStream};
use hopr_crypto_types::types::Hash;
#[cfg(all(feature = "prometheus", not(test)))]
use hopr_metrics::SimpleGauge;
use hopr_primitive_types::prelude::*;
use rust_stream_ext_concurrent::then_concurrent::StreamThenConcurrentExt;
use tracing::{debug, error, trace, warn};

use crate::{
    BlockWithLogs, FilterSet, HoprIndexerRpcOperations, Log,
    errors::{Result, RpcError, RpcError::FilterIsEmpty},
    rpc::RpcOperations,
    transport::HttpRequestor,
};

#[cfg(all(feature = "prometheus", not(test)))]
lazy_static::lazy_static! {
    static ref METRIC_RPC_CHAIN_HEAD: SimpleGauge =
        SimpleGauge::new(
            "blokli_chain_head_block_number",
            "Current block number of chain head",
    ).unwrap();
}

/// Splits a block range into smaller chunks and applies filters to each chunk.
///
/// This function takes a range of blocks and divides it into smaller sub-ranges
/// for concurrent processing. Each sub-range gets a copy of all the provided filters
/// with the appropriate block range applied.
///
/// # Arguments
/// * `filters` - Vector of log filters to apply to each chunk
/// * `from_block` - Starting block number
/// * `to_block` - Ending block number
/// * `max_chunk_size` - Maximum number of blocks per chunk
///
/// # Returns
/// * `impl Stream<Item = Vec<Filter>>` - Stream of filter vectors, one per chunk
fn split_range<'a>(
    filters: Vec<Filter>,
    from_block: u64,
    to_block: u64,
    max_chunk_size: u64,
) -> BoxStream<'a, Vec<Filter>> {
    assert!(from_block <= to_block, "invalid block range");
    assert!(max_chunk_size > 0, "chunk size must be greater than 0");

    futures::stream::unfold((from_block, to_block), move |(start, to)| {
        if start <= to {
            let end = to_block.min(start + max_chunk_size - 1);
            let ranged_filters = filters
                .iter()
                .cloned()
                .map(|f| f.from_block(start).to_block(end))
                .collect::<Vec<_>>();
            futures::future::ready(Some((ranged_filters, (end + 1, to))))
        } else {
            futures::future::ready(None)
        }
    })
    .boxed()
}

// impl<P: JsonRpcClient + 'static, R: HttpRequestor + 'static> RpcOperations<P, R> {
impl<R: HttpRequestor + 'static + Clone> RpcOperations<R> {
    /// Retrieves logs in the given range (`from_block` and `to_block` are inclusive).
    fn stream_logs(&self, filters: Vec<Filter>, from_block: u64, to_block: u64) -> BoxStream<'_, Result<Log>> {
        let fetch_ranges = split_range(filters, from_block, to_block, self.cfg.max_block_range_fetch_size);

        debug!(
            "polling logs from blocks #{from_block} - #{to_block} (via {:?} chunks)",
            (to_block - from_block) / self.cfg.max_block_range_fetch_size + 1
        );

        fetch_ranges
            .then(move |subrange_filters| async move {
                let mut results = futures::stream::iter(subrange_filters)
                    .then_concurrent(|filter| async move {
                        let prov_clone = self.provider.clone();

                        match prov_clone.get_logs(&filter).await {
                            Ok(logs) => Ok(logs),
                            Err(e) => {
                                error!(
                                    from = ?filter.get_from_block(),
                                    to = ?filter.get_to_block(),
                                    error = %e,
                                    "failed to fetch logs in block subrange"
                                );
                                Err(e)
                            }
                        }
                    })
                    .flat_map(|result| {
                        futures::stream::iter(match result {
                            Ok(logs) => logs.into_iter().map(|log| Ok(Log::try_from(log)?)).collect::<Vec<_>>(),
                            Err(e) => vec![Err(RpcError::from(e))],
                        })
                    })
                    .collect::<Vec<_>>()
                    .await;

                // at this point we need to ensure logs are ordered by block number since that is
                // expected by the indexer
                results.sort_by(|a, b| {
                    if let Ok(a) = a {
                        if let Ok(b) = b {
                            a.block_number.cmp(&b.block_number)
                        } else {
                            std::cmp::Ordering::Greater
                        }
                    } else {
                        std::cmp::Ordering::Less
                    }
                });

                futures::stream::iter(results)
            })
            .flatten()
            .boxed()
    }
}

#[async_trait]
impl<R: HttpRequestor + 'static + Clone> HoprIndexerRpcOperations for RpcOperations<R> {
    async fn block_number(&self) -> Result<u64> {
        self.get_block_number().await
    }

    async fn get_hopr_allowance(&self, owner: Address, spender: Address) -> Result<HoprBalance> {
        self.get_hopr_allowance(owner, spender).await
    }

    async fn get_xdai_balance(&self, address: Address) -> Result<XDaiBalance> {
        self.get_xdai_balance(address).await
    }

    async fn get_hopr_balance(&self, address: Address) -> Result<HoprBalance> {
        self.get_hopr_balance(address).await
    }

    /// Fetches the current transaction count for the Safe at the given address.
    ///
    /// Returns the Safe's transaction count as a `u64`.
    ///
    /// # Examples
    ///
    /// ```
    /// # async fn example(op: &RpcOperations<impl Provider>, safe_address: Address) -> Result<()> {
    /// let count = op.get_safe_transaction_count(safe_address).await?;
    /// assert!(count >= 0);
    /// # Ok(())
    /// # }
    /// ```
    async fn get_safe_transaction_count(&self, safe_address: Address) -> Result<u64> {
        self.get_safe_transaction_count(safe_address).await
    }

    /// Retrieves the sender (signer) address for a given transaction hash.
    ///
    /// # Returns
    ///
    /// The transaction sender's Hopr `Address`.
    ///
    /// # Examples
    ///
    /// ```
    /// # use ethers::types::H256 as Hash;
    /// # async fn example(rpc: &crate::RpcOperations<_>, tx_hash: Hash) {
    /// // In real usage provide a valid `tx_hash` and `rpc`.
    /// // let sender = rpc.get_transaction_sender(tx_hash).await.unwrap();
    /// // assert!(/* sender is a valid Address */);
    /// # }
    /// ```
    async fn get_transaction_sender(&self, tx_hash: Hash) -> Result<Address> {
        let tx = self
            .provider
            .get_transaction_by_hash(B256::from_slice(tx_hash.as_ref()))
            .await?
            .ok_or_else(|| RpcError::TransactionNotFound(tx_hash))?;

        Ok(tx.inner.signer().to_hopr_address())
    }

    /// Produces an incremental stream of completed block log batches starting from a given block.
    ///
    /// The returned stream yields `BlockWithLogs` items in ascending block order as blocks are
    /// finalized and processed. It advances from `start_block_number`, repeatedly querying the
    /// chain head and fetching logs for the inclusive range `[from, latest_block]`, yielding one
    /// completed `BlockWithLogs` per block. The behavior adapts when the indexer is not yet
    /// synced (token-related filters are omitted) and performs retries on transient RPC errors
    /// until a configurable failure threshold is reached.
    ///
    /// # Errors
    ///
    /// Returns `FilterIsEmpty` if `filters.all` is empty.
    ///
    /// # Examples
    ///
    /// ```
    /// // Example usage (pseudo-code):
    /// // let stream = rpc_ops.try_stream_logs(100, my_filters, /* is_synced = */ true).unwrap();
    /// // futures::pin_mut!(stream);
    /// // while let Some(block_with_logs) = stream.next().await {
    /// //     process(block_with_logs);
    /// // }
    /// ```
    fn try_stream_logs<'a>(
        &'a self,
        start_block_number: u64,
        filters: FilterSet,
        is_synced: bool,
    ) -> Result<Pin<Box<dyn Stream<Item = BlockWithLogs> + Send + 'a>>> {
        if filters.all.is_empty() {
            return Err(FilterIsEmpty);
        }

        let log_filters = if !is_synced {
            // Because we are not synced yet, we will not get logs for the token contract.
            // These are only relevant for the indexer if we are synced.
            filters.no_token
        } else {
            filters.all
        };

        Ok(Box::pin(stream! {
            // On first iteration use the given block number as start
            let mut from_block = start_block_number;

            let max_loop_failures = self.cfg.max_indexer_loop_failures;
            let max_rpc_past_blocks = self.cfg.max_indexer_past_blocks;
            let mut count_failures = 0;

            'outer: loop {
                match self.block_number().await {
                    Ok(latest_block) => {
                        if from_block > latest_block {
                            let past_diff = from_block - latest_block;
                            if from_block == start_block_number {
                                // If on first iteration the start block is in the future, just set
                                // it to the latest
                                from_block = latest_block;
                            } else if past_diff <= max_rpc_past_blocks as u64 {
                                // If we came here early (we tolerate only off-by max_rpc_past_blocks), wait some more
                                debug!(last_block = latest_block, start_block = start_block_number, blocks_diff = past_diff, "Indexer premature request. Block not found yet in RPC provider.");
                                futures_timer::Delay::new(past_diff as u32 * self.cfg.expected_block_time / 3).await;
                                continue;
                            } else {
                                // This is a hard-failure on later iterations which is unrecoverable
                                panic!("indexer start block number {from_block} is greater than the chain latest block number {latest_block} (diff {past_diff}) =>
                                possible causes: chain reorg, RPC provider out of sync, corrupted DB =>
                                possible solutions: change the RPC provider, reinitialize the DB");
                            }
                        }


                        #[cfg(all(feature = "prometheus", not(test)))]
                        METRIC_RPC_CHAIN_HEAD.set(latest_block as f64);

                        let mut retrieved_logs = self.stream_logs(log_filters.clone(), from_block, latest_block);

                        trace!(from_block, to_block = latest_block, "processing batch");

                        let mut current_block_log = BlockWithLogs { block_id: from_block, ..Default::default()};

                        loop {
                            match retrieved_logs.next().await {
                                Some(Ok(log)) => {
                                    // This in general should not happen, but handle such a case to be safe
                                    if log.block_number > latest_block {
                                        warn!(%log, latest_block, "got log that has not yet reached the finalized tip");
                                        break;
                                    }

                                    // This should not happen, thus panic.
                                    if current_block_log.block_id > log.block_number {
                                        error!(log_block_id = log.block_number, current_block_log.block_id, "received log from a previous block");
                                        panic!("The on-chain logs are not ordered by block number. This is a critical error.");
                                    }

                                    // This assumes the logs are arriving ordered by blocks when fetching a range
                                    if current_block_log.block_id < log.block_number {
                                        debug!(block = %current_block_log, "completed block, moving to next");
                                        yield current_block_log;

                                        current_block_log = BlockWithLogs::default();
                                        current_block_log.block_id = log.block_number;
                                    }

                                    debug!("retrieved {log}");
                                    current_block_log.logs.insert(log.into());
                                },
                                None => {
                                    break;
                                },
                                Some(Err(e)) => {
                                    error!(error=%e, "failed to process blocks");
                                    count_failures += 1;

                                    if count_failures < max_loop_failures {
                                        // Continue the outer loop, which throws away the current block
                                        // that may be incomplete due to this error.
                                        // We will start at this block again to re-query it.
                                        from_block = current_block_log.block_id;
                                        continue 'outer;
                                    } else {
                                        panic!("!!! Cannot advance the chain indexing due to unrecoverable RPC errors.

                                        The RPC provider does not seem to be working correctly.

                                        The last encountered error was: {e}");
                                    }
                                }
                            }
                        }

                        // Yield everything we've collected until this point
                        debug!(block = %current_block_log, "completed block, processing batch finished");
                        yield current_block_log;
                        from_block = latest_block + 1;
                        count_failures = 0;
                    }

                    Err(e) => error!(error = %e, "failed to obtain current block number from chain")
                }

                futures_timer::Delay::new(self.cfg.expected_block_time).await;
            }
        }))
    }
}

#[cfg(test)]
mod tests {
    use alloy::rpc::types::Filter;
    use anyhow::Context;
    use futures::StreamExt;

    use crate::indexer::split_range;

    fn filter_bounds(filters: &[Filter]) -> anyhow::Result<(u64, u64)> {
        let bounds = filters.iter().try_fold((0, 0), |acc, filter| {
            let to = filter
                .block_option
                .get_from_block()
                .context("a value should be present")?
                .as_number()
                .context("a value should be convertible")?;
            let from = filter
                .block_option
                .get_to_block()
                .context("a value should be present")?
                .as_number()
                .context("a value should be convertible")?;
            let next = (to, from);

            match acc {
                (0, 0) => Ok(next), // First pair becomes the reference
                acc => {
                    if acc != next {
                        anyhow::bail!("range bounds are not equal across all filters");
                    }
                    Ok(acc)
                }
            }
        })?;

        Ok(bounds)
    }

    #[tokio::test]
    async fn test_split_range() -> anyhow::Result<()> {
        let filters = vec![Filter::default()];
        let ranges = split_range(filters.clone(), 0, 10, 2).collect::<Vec<_>>().await;

        assert_eq!(6, ranges.len());
        assert_eq!((0, 1), filter_bounds(&ranges[0])?);
        assert_eq!((2, 3), filter_bounds(&ranges[1])?);
        assert_eq!((4, 5), filter_bounds(&ranges[2])?);
        assert_eq!((6, 7), filter_bounds(&ranges[3])?);
        assert_eq!((8, 9), filter_bounds(&ranges[4])?);
        assert_eq!((10, 10), filter_bounds(&ranges[5])?);

        let ranges = split_range(filters.clone(), 0, 0, 1).collect::<Vec<_>>().await;
        assert_eq!(1, ranges.len());
        assert_eq!((0, 0), filter_bounds(&ranges[0])?);

        let ranges = split_range(filters.clone(), 0, 3, 1).collect::<Vec<_>>().await;
        assert_eq!(4, ranges.len());
        assert_eq!((0, 0), filter_bounds(&ranges[0])?);
        assert_eq!((1, 1), filter_bounds(&ranges[1])?);
        assert_eq!((2, 2), filter_bounds(&ranges[2])?);
        assert_eq!((3, 3), filter_bounds(&ranges[3])?);

        let ranges = split_range(filters.clone(), 0, 3, 10).collect::<Vec<_>>().await;
        assert_eq!(1, ranges.len());
        assert_eq!((0, 3), filter_bounds(&ranges[0])?);

        Ok(())
    }
}
