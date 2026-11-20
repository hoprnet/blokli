//! Extends the [RpcOperations] type with functionality needed by the Indexer component.
//!
//! The functionality required functionality is defined in the [HoprIndexerRpcOperations] trait,
//! which is implemented for [RpcOperations] hereof.
//! The primary goal is to provide a stream of [BlockWithLogs] filtered by the given [LogFilter]
//! as the new matching blocks are mined in the underlying blockchain. The stream also allows to collect
//! historical blockchain data.
//!
//! For details on the Indexer see the `chain-indexer` crate.
use std::{
    cmp::Ordering,
    future::Future,
    pin::Pin,
    time::{Duration, Instant},
};

use async_stream::stream;
use async_trait::async_trait;
use blokli_chain_types::AlloyAddressExt;
use futures::{Stream, StreamExt, stream::BoxStream};
use hopr_bindings::exports::alloy::{
    eips::{BlockId, eip2718::Encodable2718},
    primitives::{Address as AlloyAddress, B256, Bytes, U256, address},
    providers::Provider,
    rpc::{
        client::BatchRequest,
        types::{Filter, Log as AlloyLog, Transaction as AlloyTransaction},
    },
    sol_types::SolCall,
};
#[cfg(all(feature = "telemetry", not(test)))]
use hopr_types::telemetry::SimpleGauge;
use hopr_types::{crypto::types::Hash, primitive::prelude::*};
use rust_stream_ext_concurrent::then_concurrent::StreamThenConcurrentExt;
use tokio::sync::watch;
use tracing::{debug, error, trace, warn};

use crate::{
    BlockWithLogs, FilterSet, HoprIndexerRpcOperations, Log, SafeOwnersSnapshot,
    errors::{Result, RpcError, RpcError::FilterIsEmpty},
    rpc::{
        HoprModule, Multicall3, RpcOperations, SafeSingleton,
        SafeSingleton::{getOwnersCall, getThresholdCall},
        is_log_block_range_limit_error,
    },
    tip::ChainTip,
    transport::HttpRequestor,
};

/// Canonical Multicall3 deployment (the same address on Gnosis Chain and most EVM chains).
const MULTICALL3_ADDRESS: AlloyAddress = address!("cA11bde05977b3631167028862bE2a173976CA11");

/// How long a Multicall3 detection result is reused before the code at the address is checked again.
const MULTICALL3_DETECTION_TTL: Duration = Duration::from_secs(600);

/// Maximum number of concurrent reads when Multicall3 is not available.
const SAFE_READ_CONCURRENCY: usize = 8;

/// Waits for the confirmed tip to reach `from_block`.
///
/// Returns `true` when the wait timed out. A head driver that stopped publishing leaves the watch
/// sender alive, so the wait needs its own limit instead of relying on the channel to close.
async fn wait_for_confirmed_tip(
    tip_rx: &mut watch::Receiver<Option<ChainTip>>,
    from_block: u64,
    wait_limit: Duration,
) -> bool {
    tokio::time::timeout(
        wait_limit,
        tip_rx.wait_for(|tip| tip.as_ref().is_some_and(|tip| tip.confirmed >= from_block)),
    )
    .await
    .is_err()
}

fn snapshot_from_returns(owners_return: &[u8], threshold_return: &[u8]) -> Result<SafeOwnersSnapshot> {
    let owners = getOwnersCall::abi_decode_returns(owners_return)
        .map_err(|e| RpcError::Other(format!("cannot decode getOwners: {e}")))?;
    let threshold = getThresholdCall::abi_decode_returns(threshold_return)
        .map_err(|e| RpcError::Other(format!("cannot decode getThreshold: {e}")))?;
    Ok(SafeOwnersSnapshot {
        owners: owners.into_iter().map(|owner| owner.to_hopr_address()).collect(),
        threshold: u64::try_from(threshold)
            .map_err(|_| RpcError::Other(format!("Safe threshold {threshold} exceeds maximum u64 value")))?,
    })
}

#[cfg(all(feature = "telemetry", not(test)))]
lazy_static::lazy_static! {
    static ref METRIC_RPC_CHAIN_HEAD: Option<SimpleGauge> =
        SimpleGauge::new(
            "blokli_chain_head_block_number",
            "Current block number of chain head",
    ).inspect_err(|error| error!(%error, metric = "blokli_chain_head_block_number", "failed to register telemetry metric")).ok();
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
#[cfg(test)]
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

async fn fetch_subrange_logs_concurrently<F, Fut, E>(filters: Vec<Filter>, fetch_logs: F) -> Vec<Result<Log>>
where
    F: FnMut(Filter) -> Fut,
    Fut: Future<Output = std::result::Result<Vec<AlloyLog>, E>>,
    E: Into<RpcError>,
{
    let mut results = futures::stream::iter(filters)
        .then_concurrent(fetch_logs, None)
        .flat_map(|result| {
            futures::stream::iter(match result {
                Ok(logs) => logs
                    .into_iter()
                    .map(|log| Log::try_from(log).map_err(RpcError::from))
                    .collect::<Vec<_>>(),
                Err(error) => vec![Err(error.into())],
            })
        })
        .collect::<Vec<_>>()
        .await;

    results.sort_by(|a, b| match (a, b) {
        (Ok(a), Ok(b)) => a.cmp(b),
        (Err(_), Ok(_)) => Ordering::Less,
        (Ok(_), Err(_)) => Ordering::Greater,
        (Err(_), Err(_)) => Ordering::Equal,
    });

    results
}

/// Returns the RPC error of a failed `eth_getLogs` request among the results, preferring a known block range limit
/// error. Log conversion errors are not request failures and are ignored.
fn find_log_fetch_error(results: &[Result<Log>]) -> Option<&RpcError> {
    let fetch_errors = || {
        results.iter().filter_map(|result| match result {
            Err(error @ RpcError::AlloyRpcError(_)) => Some(error),
            _ => None,
        })
    };

    fetch_errors()
        .find(|error| is_log_block_range_limit_error(error))
        .or_else(|| fetch_errors().next())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct AdaptiveLogSubrange {
    end_block: u64,
    requested_span: u64,
    attempted_limit: u64,
}

// impl<P: JsonRpcClient + 'static, R: HttpRequestor + 'static> RpcOperations<P, R> {
impl<R: HttpRequestor + 'static + Clone> RpcOperations<R> {
    fn next_adaptive_subrange(&self, next_block: u64, to_block: u64) -> AdaptiveLogSubrange {
        let attempted_limit = self.log_block_range_limit();
        let end_block = to_block.min(next_block.saturating_add(attempted_limit.saturating_sub(1)));
        let requested_span = end_block - next_block + 1;

        AdaptiveLogSubrange {
            end_block,
            requested_span,
            attempted_limit,
        }
    }

    /// Retrieves logs in the given range (`from_block` and `to_block` are inclusive).
    fn stream_logs(&self, filters: Vec<Filter>, from_block: u64, to_block: u64) -> BoxStream<'_, Result<Log>> {
        stream! {
            let mut next_block = from_block;
            let mut last_failed_range = None;

            while next_block <= to_block {
                let subrange = self.next_adaptive_subrange(next_block, to_block);
                let ranged_filters = filters
                    .iter()
                    .cloned()
                    .map(|filter| filter.from_block(next_block).to_block(subrange.end_block))
                    .collect::<Vec<_>>();

                debug!(
                    from_block = next_block,
                    to_block = subrange.end_block,
                    attempted_block_range = subrange.attempted_limit,
                    "polling logs from block subrange"
                );

                let results = fetch_subrange_logs_concurrently(ranged_filters, |filter| async move {
                    let prov_clone = self.provider.clone();

                    match prov_clone.get_logs(&filter).await {
                        Ok(logs) => Ok(logs),
                        Err(error) => {
                            error!(
                                from = ?filter.get_from_block(),
                                to = ?filter.get_to_block(),
                                error = %error,
                                "failed to fetch logs in block subrange"
                            );
                            let rpc_error = RpcError::from(error);
                            Err(rpc_error)
                        }
                    }
                })
                .await;

                match find_log_fetch_error(&results) {
                    Some(error) => {
                        let failed_range = Some((next_block, subrange.end_block));
                        let repeated = last_failed_range == failed_range;
                        last_failed_range = failed_range;
                        if self.record_log_block_range_failure(subrange.requested_span, subrange.attempted_limit, repeated, error) {
                            continue;
                        }
                    }
                    None => self.record_log_block_range_success(subrange.requested_span, subrange.attempted_limit),
                }

                for result in results {
                    yield result;
                }

                next_block = subrange.end_block + 1;
            }
        }
        .boxed()
    }
}

impl<R: HttpRequestor + 'static + Clone> RpcOperations<R> {
    /// Whether Multicall3 is deployed at its canonical address, detected by the code at that address.
    ///
    /// The result is reused for [`MULTICALL3_DETECTION_TTL`], so a refresh of many batches checks once and
    /// a later refresh checks again. A failed check is an error for the caller (the batch is retried
    /// later) and is not cached; a chain without Multicall3 (e.g. a fresh Anvil) reads with individual calls.
    async fn multicall_deployed(&self) -> Result<bool> {
        if let Ok(cached) = self.multicall_support.lock() {
            if let Some((deployed, detected_at)) = *cached {
                if detected_at.elapsed() < MULTICALL3_DETECTION_TTL {
                    return Ok(deployed);
                }
            }
        }
        let deployed = !self.provider.get_code_at(MULTICALL3_ADDRESS).await?.is_empty();
        if let Ok(mut cached) = self.multicall_support.lock() {
            *cached = Some((deployed, Instant::now()));
        }
        Ok(deployed)
    }
}

#[async_trait]
impl<R: HttpRequestor + 'static + Clone> HoprIndexerRpcOperations for RpcOperations<R> {
    async fn block_number(&self) -> Result<u64> {
        self.get_block_number().await
    }

    fn cached_confirmed_block(&self) -> Option<u64> {
        self.tip_tracker().current().map(|tip| tip.confirmed)
    }

    fn cached_header_timestamp(&self, block: u64) -> Option<u64> {
        self.tip_tracker().header_timestamp(block)
    }

    /// Retrieves the sender (signer) address for a given transaction hash.
    ///
    /// # Returns
    ///
    /// The transaction sender's Hopr `Address`.
    ///
    /// # Examples
    ///
    /// ```ignore
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

    async fn get_transaction_bytes(&self, tx_hash: Hash) -> Result<Vec<u8>> {
        let tx = self
            .provider
            .get_transaction_by_hash(B256::from_slice(tx_hash.as_ref()))
            .await?
            .ok_or_else(|| RpcError::TransactionNotFound(tx_hash))?;

        Ok(tx.inner.encoded_2718())
    }

    /// Sends all `eth_getTransactionByHash` lookups as a single JSON-RPC batch request.
    ///
    /// Only one HTTP round-trip is paid for the whole set, which is what makes this worthwhile on
    /// a distant endpoint. If the batch request itself fails, every hash is reported with that
    /// same transport error; per-hash failures inside a successful batch stay independent.
    async fn get_transaction_bytes_batch(&self, tx_hashes: &[Hash]) -> Vec<Result<Vec<u8>>> {
        if tx_hashes.is_empty() {
            return Vec::new();
        }

        let mut batch = BatchRequest::new(self.provider.client());
        let mut waiters = Vec::with_capacity(tx_hashes.len());

        for tx_hash in tx_hashes {
            let params = (B256::from_slice(tx_hash.as_ref()),);
            match batch.add_call::<_, Option<AlloyTransaction>>("eth_getTransactionByHash", &params) {
                Ok(waiter) => waiters.push(Ok(waiter)),
                Err(error) => waiters.push(Err(RpcError::from(error))),
            }
        }

        if let Err(error) = batch.send().await {
            error!(count = tx_hashes.len(), %error, "batched transaction lookup failed");
            return tx_hashes
                .iter()
                .map(|_| Err(RpcError::Other(format!("batched transaction lookup failed: {error}"))))
                .collect();
        }

        let mut results = Vec::with_capacity(tx_hashes.len());
        for (tx_hash, waiter) in tx_hashes.iter().zip(waiters) {
            results.push(match waiter {
                Ok(waiter) => match waiter.await {
                    Ok(Some(tx)) => Ok(tx.inner.encoded_2718()),
                    Ok(None) => Err(RpcError::TransactionNotFound(*tx_hash)),
                    Err(error) => Err(RpcError::from(error)),
                },
                Err(error) => Err(error),
            });
        }

        results
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
            let tracker = self.tip_tracker();
            tracker.ensure_started();
            let mut tip_rx = tracker.subscribe();

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
                                let wait_limit = self
                                    .cfg
                                    .expected_block_time
                                    .saturating_mul(u32::try_from(past_diff).unwrap_or(u32::MAX))
                                    / 3;
                                let _ = tokio::time::timeout(
                                    wait_limit,
                                    tip_rx.wait_for(|tip| tip.as_ref().is_some_and(|tip| tip.confirmed >= from_block)),
                                )
                                .await;
                                continue;
                            } else {
                                // This is a hard-failure on later iterations which is unrecoverable
                                panic!("indexer start block number {from_block} is greater than the chain latest block number {latest_block} (diff {past_diff}) =>
                                possible causes: chain reorg, RPC provider out of sync, corrupted DB =>
                                possible solutions: change the RPC provider, reinitialize the DB");
                            }
                        }


                        #[cfg(all(feature = "telemetry", not(test)))]
                        if let Some(metric) = METRIC_RPC_CHAIN_HEAD.as_ref() {
                            metric.set(latest_block as f64);
                        }

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

                                    debug!(hex = %log, "retrieved log");
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

                        let wait_limit = self
                            .cfg
                            .expected_block_time
                            .max(self.cfg.head_poll_interval)
                            .saturating_mul(3);
                        if wait_for_confirmed_tip(&mut tip_rx, from_block, wait_limit).await {
                            debug!(
                                from_block,
                                ?wait_limit,
                                "no confirmed tip update within the wait limit, re-reading the chain tip"
                            );
                        }
                    }

                    Err(e) => {
                        error!(error = %e, "failed to obtain current block number from chain");
                        tokio::time::sleep(self.cfg.head_poll_interval).await;
                    }
                }
            }
        }))
    }

    async fn get_logs_for_address(
        &self,
        address: Address,
        topics: Vec<B256>,
        from_block: u64,
        to_block: u64,
    ) -> Result<Vec<Log>> {
        let mut logs = Vec::new();
        let mut next_block = from_block;
        let mut last_failed_range = None;

        while next_block <= to_block {
            let subrange = self.next_adaptive_subrange(next_block, to_block);
            let filter = Filter::new()
                .address(AlloyAddress::from_hopr_address(address))
                .event_signature(topics.clone())
                .from_block(next_block)
                .to_block(subrange.end_block);

            match self.provider.get_logs(&filter).await {
                Ok(chunk_logs) => {
                    self.record_log_block_range_success(subrange.requested_span, subrange.attempted_limit);

                    let mut converted_logs = chunk_logs
                        .into_iter()
                        .map(Log::try_from)
                        .collect::<std::result::Result<Vec<_>, _>>()?;
                    logs.append(&mut converted_logs);
                    next_block = subrange.end_block + 1;
                }
                Err(error) => {
                    let rpc_error = RpcError::from(error);

                    let failed_range = Some((next_block, subrange.end_block));
                    let repeated = last_failed_range == failed_range;
                    last_failed_range = failed_range;

                    if self.record_log_block_range_failure(
                        subrange.requested_span,
                        subrange.attempted_limit,
                        repeated,
                        &rpc_error,
                    ) {
                        continue;
                    }

                    return Err(rpc_error);
                }
            }
        }

        logs.sort();
        Ok(logs)
    }

    async fn get_safe_owners_and_threshold(&self, safe_address: Address, block: u64) -> Result<SafeOwnersSnapshot> {
        let safe_contract = SafeSingleton::new(AlloyAddress::from_hopr_address(safe_address), self.provider.clone());
        let block_id = BlockId::number(block);

        let owners_call = safe_contract.getOwners().block(block_id);
        let threshold_call = safe_contract.getThreshold().block(block_id);
        let (owners, threshold) = futures::try_join!(async { owners_call.call().await }, async {
            threshold_call.call().await
        })?;

        Ok(SafeOwnersSnapshot {
            owners: owners.into_iter().map(|owner| owner.to_hopr_address()).collect(),
            threshold: u64::try_from(threshold)
                .map_err(|_| RpcError::Other(format!("Safe threshold {threshold} exceeds maximum u64 value")))?,
        })
    }

    async fn get_safe_owners_and_thresholds(
        &self,
        safe_addresses: Vec<Address>,
        block: u64,
    ) -> Result<Vec<Result<SafeOwnersSnapshot>>> {
        if safe_addresses.is_empty() {
            return Ok(Vec::new());
        }

        if !self.multicall_deployed().await? {
            debug!("Multicall3 not deployed, reading Safe owners with individual calls");
            let mut snapshots = futures::stream::iter(safe_addresses.into_iter().enumerate())
                .map(|(index, safe_address)| async move {
                    (index, self.get_safe_owners_and_threshold(safe_address, block).await)
                })
                .buffer_unordered(SAFE_READ_CONCURRENCY)
                .collect::<Vec<_>>()
                .await;
            snapshots.sort_by_key(|(index, _)| *index);
            return Ok(snapshots.into_iter().map(|(_, snapshot)| snapshot).collect());
        }

        let owners_call: Bytes = getOwnersCall {}.abi_encode().into();
        let threshold_call: Bytes = getThresholdCall {}.abi_encode().into();
        let calls = safe_addresses
            .iter()
            .flat_map(|safe_address| {
                let target = AlloyAddress::from_hopr_address(*safe_address);
                [owners_call.clone(), threshold_call.clone()].map(|call_data| Multicall3::Call3 {
                    target,
                    allowFailure: true,
                    callData: call_data,
                })
            })
            .collect::<Vec<_>>();

        let results = Multicall3::new(MULTICALL3_ADDRESS, self.provider.clone())
            .aggregate3(calls)
            .block(BlockId::number(block))
            .call()
            .await?;
        if results.len() != safe_addresses.len() * 2 {
            return Err(RpcError::Other(format!(
                "Multicall3 returned {} results for {} calls",
                results.len(),
                safe_addresses.len() * 2
            )));
        }

        // Two results per Safe, in call order: getOwners then getThreshold.
        let mut results = results.into_iter();
        let mut snapshots = Vec::with_capacity(safe_addresses.len());
        for index in 0..safe_addresses.len() {
            let (Some(owners), Some(threshold)) = (results.next(), results.next()) else {
                break;
            };
            snapshots.push(if owners.success && threshold.success {
                snapshot_from_returns(&owners.returnData, &threshold.returnData)
            } else {
                Err(RpcError::MulticallFailure(
                    index,
                    "getOwners or getThreshold reverted".to_string(),
                ))
            });
        }
        Ok(snapshots)
    }

    fn subscribe_tip(&self) -> Option<watch::Receiver<Option<ChainTip>>> {
        self.tip_tracker().ensure_started();
        Some(self.tip_tracker().subscribe())
    }

    async fn get_logs_for_block_range(
        &self,
        filters: Vec<Filter>,
        from_block: u64,
        to_block: u64,
    ) -> Result<Vec<SerializableLog>> {
        let mut logs = Vec::new();
        let mut log_stream = self.stream_logs(filters, from_block, to_block);

        while let Some(result) = log_stream.next().await {
            logs.push(SerializableLog::from(result?));
        }

        logs.sort_by_key(|log| (log.block_number, log.tx_index, log.log_index));
        Ok(logs)
    }

    async fn get_xdai_balance(&self, address: Address) -> Result<XDaiBalance> {
        self.get_xdai_balance(address).await
    }

    async fn get_hopr_balance(&self, address: Address) -> Result<HoprBalance> {
        self.get_hopr_balance(address).await
    }

    async fn get_xhopr_balance(&self, address: Address) -> Result<XHoprBalance> {
        self.get_xhopr_balance(address).await
    }

    async fn get_hopr_allowance(&self, owner: Address, spender: Address) -> Result<HoprBalance> {
        self.get_hopr_allowance(owner, spender).await
    }

    async fn get_transaction_count(&self, address: Address) -> Result<u64> {
        self.get_transaction_count(address).await
    }

    async fn get_channel_closure_notice_period(&self) -> Result<Duration> {
        self.get_channel_closure_notice_period().await
    }

    async fn get_hopr_module_from_safe(&self, safe_address: Address) -> Result<Option<Address>> {
        // Safe linked list start pointer (as per Safe docs)
        const START_POINTER: AlloyAddress =
            AlloyAddress::new([0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1]);
        const PAGE_SIZE: u64 = 10;

        let safe_alloy_addr = AlloyAddress::from_hopr_address(safe_address);
        let safe_contract = SafeSingleton::new(safe_alloy_addr, self.provider.clone());

        // Pagination cursor - starts at sentinel value
        let mut cursor = START_POINTER;

        loop {
            // Get modules from the Safe contract (one page at a time)
            let result = safe_contract
                .getModulesPaginated(cursor, U256::from(PAGE_SIZE))
                .call()
                .await?;

            let modules = result.array;

            debug!(
                safe_address = %safe_address,
                module_count = modules.len(),
                cursor = %cursor,
                next = %result.next,
                "Retrieved modules page from Safe contract"
            );

            // Check each module to see if it's a HOPR node management module
            for module_addr in modules {
                // Skip zero address or start pointer
                if module_addr == AlloyAddress::ZERO || module_addr == START_POINTER {
                    continue;
                }

                let module_contract = HoprModule::new(module_addr, self.provider.clone());

                // Try to call isHoprNodeManagementModule - if it returns true, this is our module
                match module_contract.isHoprNodeManagementModule().call().await {
                    Ok(is_hopr_module) if is_hopr_module => {
                        let hopr_addr = module_addr.to_hopr_address();
                        debug!(
                            safe_address = %safe_address,
                            module_address = %hopr_addr,
                            "Found HOPR node management module"
                        );
                        return Ok(Some(hopr_addr));
                    }
                    Ok(_) => {
                        // Not a HOPR module, continue checking
                        trace!(
                            module_address = %module_addr,
                            "Module is not a HOPR node management module"
                        );
                    }
                    Err(e) => {
                        // This module doesn't implement the interface, skip it
                        trace!(
                            module_address = %module_addr,
                            error = %e,
                            "Module does not implement isHoprNodeManagementModule"
                        );
                    }
                }
            }

            // Check if we've reached the end of the list
            // Safe returns START_POINTER as next when there are no more pages
            if result.next == START_POINTER {
                break;
            }

            // Guard against infinite loop (cursor unchanged)
            if result.next == cursor {
                break;
            }

            // Move to next page
            cursor = result.next;
        }

        // No HOPR module found
        debug!(
            safe_address = %safe_address,
            "No HOPR node management module found in Safe"
        );
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use std::{
        sync::{Arc, Mutex},
        time::{Duration, Instant},
    };

    use anyhow::Context;
    use async_trait::async_trait;
    use blokli_chain_types::{AlloyAddressExt as _, ContractAddresses};
    use futures::StreamExt;
    use hopr_bindings::exports::alloy::{
        primitives::{Address as AlloyAddress, B256, Log as AlloyPrimitiveLog, LogData, U256 as TestU256},
        rpc::{
            client::ClientBuilder,
            types::{Filter, Log as AlloyLog},
        },
        transports::http::ReqwestTransport,
    };
    use hopr_types::{crypto::prelude::Hash, primitive::prelude::Address};
    use serde_json::json;
    use tokio::{sync::watch, time::sleep};
    use url::Url;

    use super::{
        SafeSingleton::{getOwnersCall, getThresholdCall},
        SolCall, snapshot_from_returns, wait_for_confirmed_tip,
    };
    use crate::{
        FilterSet, HoprIndexerRpcOperations,
        errors::{HttpRequestError, Result as RpcResult, RpcError},
        indexer::{fetch_subrange_logs_concurrently, split_range},
        rpc::{RpcOperations, RpcOperationsConfig},
        tip::{
            ChainTip, ChainTipTracker, ConfirmationConfig, ConfirmationMode, FcrState, HeadSource, TipObservation,
            TipSource,
        },
        transport::HttpRequestor,
    };

    #[derive(Clone, Debug, Default)]
    struct TestRequestor;

    #[async_trait]
    impl HttpRequestor for TestRequestor {
        async fn http_get(&self, _url: &str) -> std::result::Result<Box<[u8]>, HttpRequestError> {
            unreachable!("gas oracle should not be used in indexer log streaming tests")
        }
    }

    fn create_test_rpc_operations(server_url: &str) -> anyhow::Result<RpcOperations<TestRequestor>> {
        create_test_rpc_operations_with_max_range(server_url, 10)
    }

    fn create_test_rpc_operations_with_max_range(
        server_url: &str,
        max_block_range_fetch_size: u64,
    ) -> anyhow::Result<RpcOperations<TestRequestor>> {
        let transport_client = ReqwestTransport::new(Url::parse(server_url)?);
        let rpc_client = ClientBuilder::default().transport(transport_client.clone(), transport_client.guess_local());
        let cfg = RpcOperationsConfig {
            contract_addrs: ContractAddresses::default(),
            gas_oracle_url: None,
            tx_polling_interval: Duration::from_secs(1),
            max_block_range_fetch_size,
            ..RpcOperationsConfig::default()
        };

        RpcOperations::new(rpc_client, TestRequestor, cfg, Some(true)).map_err(Into::into)
    }

    fn empty_logs_response() -> String {
        json!({
            "jsonrpc": "2.0",
            "id": 1,
            "result": []
        })
        .to_string()
    }

    fn range_limit_error_response() -> String {
        json!({
            "jsonrpc": "2.0",
            "id": 1,
            "error": {
                "code": -32005,
                "message": "eth_getLogs block range limit exceeded"
            }
        })
        .to_string()
    }

    fn mock_get_logs_range(
        server: &mut mockito::ServerGuard,
        from_block: u64,
        to_block: u64,
        body: String,
    ) -> mockito::Mock {
        mock_get_logs_range_with_status(server, from_block, to_block, 200, body)
    }

    fn mock_get_logs_range_with_status(
        server: &mut mockito::ServerGuard,
        from_block: u64,
        to_block: u64,
        status: usize,
        body: String,
    ) -> mockito::Mock {
        server
            .mock("POST", "/")
            .match_body(mockito::Matcher::AllOf(vec![
                mockito::Matcher::Regex(r#""method"\s*:\s*"eth_getLogs""#.to_string()),
                mockito::Matcher::Regex(format!(r#""fromBlock"\s*:\s*"0x{from_block:x}""#)),
                mockito::Matcher::Regex(format!(r#""toBlock"\s*:\s*"0x{to_block:x}""#)),
            ]))
            .with_status(status)
            .with_body(body)
            .expect(1)
            .create()
    }

    /// Error returned by Ankr for a too-wide `eth_getLogs` range, which is not in the list of known range errors.
    fn unrecognized_range_error_response() -> String {
        json!({
            "jsonrpc": "2.0",
            "id": 1,
            "error": {
                "code": -32062,
                "message": "Block range is too large"
            }
        })
        .to_string()
    }

    fn mock_block_number_batch(server: &mut mockito::ServerGuard, latest: u64) -> mockito::Mock {
        server
            .mock("POST", "/")
            .match_body(mockito::Matcher::AllOf(vec![
                mockito::Matcher::Regex(r#""method"\s*:\s*"eth_getBlockByNumber""#.to_string()),
                mockito::Matcher::Regex(r#""(latest|safe|finalized)""#.to_string()),
            ]))
            .with_body_from_request(move |request| {
                let body: serde_json::Value =
                    serde_json::from_slice(request.body().expect("request body")).expect("valid JSON request body");
                let responses = body
                    .as_array()
                    .expect("a batched block request is a JSON array")
                    .iter()
                    .map(|call| {
                        json!({
                            "jsonrpc": "2.0",
                            "id": call["id"].clone(),
                            "result": {
                                "number": format!("{latest:#x}"),
                                "timestamp": "0x1"
                            }
                        })
                    })
                    .collect::<Vec<_>>();

                serde_json::to_vec(&responses).expect("batch response should serialize")
            })
            .expect_at_least(1)
            .create()
    }

    struct SharedTipSource(Arc<Mutex<TipObservation>>);

    #[async_trait]
    impl TipSource for SharedTipSource {
        async fn observe(&self) -> RpcResult<TipObservation> {
            Ok(*self.0.lock().expect("shared observation lock should not be poisoned"))
        }
    }

    fn single_log_response(block_number: u64) -> String {
        json!({
            "jsonrpc": "2.0",
            "id": 1,
            "result": [{
                "address": format!("{:#x}", AlloyAddress::ZERO),
                "topics": [format!("{:#x}", B256::ZERO)],
                "data": "0x010203",
                "blockNumber": format!("{block_number:#x}"),
                "transactionHash": format!("{:#x}", B256::ZERO),
                "transactionIndex": "0x0",
                "blockHash": format!("{:#x}", B256::ZERO),
                "logIndex": "0x0",
                "removed": false
            }]
        })
        .to_string()
    }

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

    /// Builds RPC operations over a bare transport, so a batch response reaches the code under
    /// test exactly as the server sent it, item errors included.
    fn create_test_rpc_operations_for_batches(server_url: &str) -> anyhow::Result<RpcOperations<TestRequestor>> {
        let transport_client = ReqwestTransport::new(Url::parse(server_url)?);
        let rpc_client = ClientBuilder::default().transport(transport_client.clone(), transport_client.guess_local());
        let cfg = RpcOperationsConfig {
            contract_addrs: ContractAddresses::default(),
            gas_oracle_url: None,
            tx_polling_interval: Duration::from_secs(1),
            ..RpcOperationsConfig::default()
        };

        RpcOperations::new(rpc_client, TestRequestor, cfg, Some(true)).map_err(Into::into)
    }

    #[tokio::test]
    async fn test_get_transaction_bytes_batch_maps_every_hash_in_input_order() -> anyhow::Result<()> {
        let mut server = mockito::Server::new_async().await;

        let found = Hash::create(&[b"found"]);
        let missing = Hash::create(&[b"missing"]);
        let failing = Hash::create(&[b"failing"]);

        // Answer the batch by echoing back the ids the client chose, so each response lands on the
        // waiter it belongs to no matter how the client numbers the calls. The order of the
        // responses is deliberately reversed: JSON-RPC allows it, and results must still be keyed
        // by id rather than by position.
        let found_param = format!("{:?}", B256::from_slice(found.as_ref()));
        let missing_param = format!("{:?}", B256::from_slice(missing.as_ref()));
        let mock = server
            .mock("POST", "/")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body_from_request(move |request| {
                let body: serde_json::Value = serde_json::from_slice(request.body().unwrap()).unwrap();
                let calls = body.as_array().expect("a batch request is a JSON array");

                let responses = calls
                    .iter()
                    .map(|call| {
                        let id = call["id"].clone();
                        let param = call["params"][0].as_str().unwrap_or_default().to_string();

                        if param == found_param {
                            json!({"jsonrpc":"2.0","id":id,"result":{
                                "type":"0x2",
                                "chainId":"0x64",
                                "nonce":"0x0",
                                "gas":"0x5208",
                                "maxFeePerGas":"0x1",
                                "maxPriorityFeePerGas":"0x1",
                                "to":"0x0000000000000000000000000000000000000001",
                                "value":"0x0",
                                "accessList":[],
                                "input":"0x",
                                "r":"0x1","s":"0x1","yParity":"0x0","v":"0x0",
                                "hash":"0x0000000000000000000000000000000000000000000000000000000000000001",
                                "blockHash":"0x0000000000000000000000000000000000000000000000000000000000000002",
                                "blockNumber":"0x1",
                                "transactionIndex":"0x0",
                                "from":"0x0000000000000000000000000000000000000002"
                            }})
                        } else if param == missing_param {
                            // A hash the chain does not know about must map to TransactionNotFound.
                            json!({"jsonrpc":"2.0","id":id,"result":serde_json::Value::Null})
                        } else {
                            // A non-retryable item error must stay confined to its own hash.
                            json!({"jsonrpc":"2.0","id":id,"error":{"code":-32000,"message":"no such transaction"}})
                        }
                    })
                    .rev()
                    .collect::<Vec<_>>();

                serde_json::to_vec(&responses).unwrap()
            })
            .expect(1)
            .create();

        let rpc = create_test_rpc_operations_for_batches(&server.url())?;
        let results = rpc.get_transaction_bytes_batch(&[found, missing, failing]).await;

        mock.assert();
        assert_eq!(results.len(), 3, "one result per input hash");
        assert!(
            results[0].as_ref().is_ok_and(|bytes| !bytes.is_empty()),
            "the found transaction must survive the failures sharing its batch: {:?}",
            results[0]
        );
        assert!(
            matches!(results[1], Err(RpcError::TransactionNotFound(hash)) if hash == missing),
            "a null result must map to TransactionNotFound: {:?}",
            results[1]
        );
        assert!(results[2].is_err(), "the erroring hash must report its own failure");

        Ok(())
    }

    #[test]
    fn test_next_adaptive_subrange_uses_current_limit() -> anyhow::Result<()> {
        let rpc = create_test_rpc_operations_with_max_range("http://localhost:8545", 3)?;

        let subrange = rpc.next_adaptive_subrange(5, 9);
        assert_eq!(subrange.end_block, 7);
        assert_eq!(subrange.requested_span, 3);
        assert_eq!(subrange.attempted_limit, 3);

        let final_subrange = rpc.next_adaptive_subrange(8, 9);
        assert_eq!(final_subrange.end_block, 9);
        assert_eq!(final_subrange.requested_span, 2);
        assert_eq!(final_subrange.attempted_limit, 3);

        Ok(())
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

    #[tokio::test]
    async fn test_fetch_subrange_logs_concurrently_sorts_results_after_concurrent_completion() -> anyhow::Result<()> {
        fn create_test_log(block_number: Option<u64>, tx_index: Option<u64>, log_index: Option<u64>) -> AlloyLog {
            AlloyLog {
                inner: AlloyPrimitiveLog {
                    address: AlloyAddress::ZERO,
                    data: LogData::new_unchecked(vec![B256::ZERO], vec![1, 2, 3].into()),
                },
                block_hash: Some(B256::ZERO),
                block_number,
                block_timestamp: None,
                transaction_hash: Some(B256::ZERO),
                transaction_index: tx_index,
                log_index,
                removed: false,
            }
        }

        let early_filter = Filter::new().from_block(1_u64).to_block(1_u64);
        let late_filter = Filter::new().from_block(2_u64).to_block(2_u64);
        let results = fetch_subrange_logs_concurrently(vec![late_filter, early_filter], |filter| async move {
            let from_block = filter.get_from_block().expect("from block should be present");

            match from_block {
                1 => {
                    sleep(Duration::from_millis(20)).await;
                    Ok::<_, RpcError>(vec![create_test_log(Some(7), Some(3), Some(9))])
                }
                2 => {
                    sleep(Duration::from_millis(1)).await;
                    Ok::<_, RpcError>(vec![
                        create_test_log(Some(5), Some(1), Some(4)),
                        create_test_log(Some(5), Some(0), Some(2)),
                    ])
                }
                _ => Err(RpcError::Other("unexpected filter".to_string())),
            }
        })
        .await;

        assert_eq!(results.len(), 3);
        assert_eq!(results[0].as_ref().expect("first log should succeed").tx_index, 0);
        assert_eq!(results[1].as_ref().expect("second log should succeed").tx_index, 1);
        assert_eq!(results[2].as_ref().expect("third log should succeed").block_number, 7);

        Ok(())
    }

    #[tokio::test]
    async fn test_fetch_subrange_logs_concurrently_surfaces_fetch_errors() {
        let results = fetch_subrange_logs_concurrently(vec![Filter::new()], |_filter| async {
            Err::<Vec<AlloyLog>, _>(RpcError::Other("boom".to_string()))
        })
        .await;

        assert!(matches!(results.as_slice(), [Err(RpcError::Other(message))] if message == "boom"));
    }

    #[tokio::test]
    async fn test_fetch_subrange_logs_concurrently_surfaces_conversion_errors() {
        let results = fetch_subrange_logs_concurrently(vec![Filter::new()], |_filter| async {
            Ok::<_, RpcError>(vec![AlloyLog {
                inner: AlloyPrimitiveLog {
                    address: AlloyAddress::ZERO,
                    data: LogData::new_unchecked(vec![B256::ZERO], vec![1, 2, 3].into()),
                },
                block_hash: Some(B256::ZERO),
                block_number: None,
                block_timestamp: None,
                transaction_hash: Some(B256::ZERO),
                transaction_index: Some(0),
                log_index: Some(0),
                removed: false,
            }])
        })
        .await;

        assert!(matches!(results.as_slice(), [Err(RpcError::LogConversionError(_))]));
    }

    #[tokio::test]
    async fn test_stream_logs_fetches_logs_via_provider() -> anyhow::Result<()> {
        let mut server = mockito::Server::new_async().await;
        let rpc = create_test_rpc_operations(&server.url())?;

        let response = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "result": [{
                "address": format!("{:#x}", AlloyAddress::ZERO),
                "topics": [format!("{:#x}", B256::ZERO)],
                "data": "0x010203",
                "blockNumber": "0x5",
                "transactionHash": format!("{:#x}", B256::ZERO),
                "transactionIndex": "0x1",
                "blockHash": format!("{:#x}", B256::ZERO),
                "logIndex": "0x2",
                "removed": false
            }]
        });

        let mock = server
            .mock("POST", "/")
            .match_body(mockito::Matcher::PartialJson(json!({"method": "eth_getLogs"})))
            .with_status(200)
            .with_body(response.to_string())
            .expect(1)
            .create();

        let results = rpc.stream_logs(vec![Filter::new()], 5, 5).collect::<Vec<_>>().await;

        mock.assert();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].as_ref().expect("log fetch should succeed").block_number, 5);

        Ok(())
    }

    #[tokio::test]
    async fn test_stream_logs_discovers_block_range_limit_from_request_failures() -> anyhow::Result<()> {
        let mut server = mockito::Server::new_async().await;
        let rpc = create_test_rpc_operations_with_max_range(&server.url(), 8)?;
        let range_limit_error = range_limit_error_response();
        let empty_logs = empty_logs_response();

        let failed_cap = mock_get_logs_range(&mut server, 0, 7, range_limit_error.clone());
        let failed_midpoint = mock_get_logs_range(&mut server, 0, 3, range_limit_error);
        let working_low = mock_get_logs_range(&mut server, 0, 1, empty_logs.clone());
        let working_limit = mock_get_logs_range(&mut server, 2, 4, empty_logs.clone());
        let stable_reuse = mock_get_logs_range(&mut server, 5, 7, empty_logs);

        let results = rpc.stream_logs(vec![Filter::new()], 0, 7).collect::<Vec<_>>().await;

        failed_cap.assert();
        failed_midpoint.assert();
        working_low.assert();
        working_limit.assert();
        stable_reuse.assert();
        assert!(results.is_empty());
        assert_eq!(rpc.log_block_range_limit(), 3);

        Ok(())
    }

    #[tokio::test]
    async fn test_get_logs_for_address_uses_adaptive_block_range_limit() -> anyhow::Result<()> {
        let mut server = mockito::Server::new_async().await;
        let rpc = create_test_rpc_operations_with_max_range(&server.url(), 3)?;
        let first_range = mock_get_logs_range(&mut server, 5, 7, empty_logs_response());
        let second_range = mock_get_logs_range(&mut server, 8, 9, empty_logs_response());

        let logs = rpc
            .get_logs_for_address(Address::from([0_u8; 20]), vec![B256::ZERO], 5, 9)
            .await?;

        first_range.assert();
        second_range.assert();
        assert!(logs.is_empty());
        assert_eq!(rpc.log_block_range_limit(), 3);

        Ok(())
    }

    #[tokio::test]
    async fn test_stream_logs_surfaces_provider_errors() -> anyhow::Result<()> {
        let mut server = mockito::Server::new_async().await;
        let rpc = create_test_rpc_operations(&server.url())?;

        let mock = server
            .mock("POST", "/")
            .match_body(mockito::Matcher::PartialJson(json!({"method": "eth_getLogs"})))
            .with_status(500)
            .with_body("{}")
            .expect(1)
            .create();

        let results = rpc.stream_logs(vec![Filter::new()], 5, 5).collect::<Vec<_>>().await;

        mock.assert();
        assert!(matches!(results.as_slice(), [Err(RpcError::AlloyRpcError(_))]));
        // A failed single-block request without a range limit error says nothing about the block range.
        assert_eq!(rpc.log_block_range_limit(), 10);

        Ok(())
    }

    #[tokio::test]
    async fn test_stream_logs_shrinks_block_range_on_unrecognized_errors() -> anyhow::Result<()> {
        let mut server = mockito::Server::new_async().await;
        let rpc = create_test_rpc_operations_with_max_range(&server.url(), 8)?;

        let gateway_timeout = mock_get_logs_range_with_status(&mut server, 0, 7, 504, "gateway timeout".into());
        let unknown_error = mock_get_logs_range(&mut server, 0, 3, unrecognized_range_error_response());
        let working_low = mock_get_logs_range(&mut server, 0, 1, empty_logs_response());
        let working_limit = mock_get_logs_range(&mut server, 2, 4, empty_logs_response());
        let stable_reuse = mock_get_logs_range(&mut server, 5, 7, single_log_response(6));

        let results = rpc.stream_logs(vec![Filter::new()], 0, 7).collect::<Vec<_>>().await;

        gateway_timeout.assert();
        unknown_error.assert();
        working_low.assert();
        working_limit.assert();
        stable_reuse.assert();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].as_ref().expect("log fetch should succeed").block_number, 6);
        assert_eq!(rpc.log_block_range_limit(), 3);

        Ok(())
    }

    #[tokio::test]
    async fn test_stream_logs_keeps_block_range_on_failed_partial_span() -> anyhow::Result<()> {
        let mut server = mockito::Server::new_async().await;
        let rpc = create_test_rpc_operations_with_max_range(&server.url(), 8)?;
        let full_range = mock_get_logs_range(&mut server, 0, 7, empty_logs_response());
        rpc.stream_logs(vec![Filter::new()], 0, 7).collect::<Vec<_>>().await;
        full_range.assert();

        // Near the chain head only two new blocks are requested; a transient error is retried once unchanged and
        // says nothing about the range.
        let gateway_timeout = mock_get_logs_range_with_status(&mut server, 8, 9, 504, "gateway timeout".into());
        let retried = mock_get_logs_range(&mut server, 8, 9, single_log_response(9));

        let results = rpc.stream_logs(vec![Filter::new()], 8, 9).collect::<Vec<_>>().await;

        gateway_timeout.assert();
        retried.assert();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].as_ref().expect("log fetch should succeed").block_number, 9);
        assert_eq!(rpc.log_block_range_limit(), 8);

        Ok(())
    }

    #[tokio::test]
    async fn test_stream_logs_shrinks_failed_partial_span_beyond_proven_range() -> anyhow::Result<()> {
        let mut server = mockito::Server::new_async().await;
        let rpc = create_test_rpc_operations_with_max_range(&server.url(), 8)?;

        // Only six blocks remain below the ceiling of eight, and that never-tested range is rejected.
        let rejected = mock_get_logs_range(&mut server, 0, 5, unrecognized_range_error_response());
        let first_half = mock_get_logs_range(&mut server, 0, 2, empty_logs_response());
        let second_half = mock_get_logs_range(&mut server, 3, 5, single_log_response(4));

        let results = rpc.stream_logs(vec![Filter::new()], 0, 5).collect::<Vec<_>>().await;

        rejected.assert();
        first_half.assert();
        second_half.assert();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].as_ref().expect("log fetch should succeed").block_number, 4);

        Ok(())
    }

    /// A rejected tail within the proven range is confirmed and shrunk inside the RPC layer, so the indexer never
    /// retries the batch and its failure budget is not charged, whatever chunk boundaries a replay would use.
    #[test_log::test(tokio::test)]
    async fn test_try_stream_logs_shrinks_rejected_tail_without_indexer_retry() -> anyhow::Result<()> {
        let mut server = mockito::Server::new_async().await;
        let rpc = create_test_rpc_operations_with_max_range(&server.url(), 8)?;

        // Chain head 12 with the default depth of 3 makes block 9 the confirmed tip.
        let block_number = mock_block_number_batch(&mut server, 12);
        let replayed = mock_get_logs_range(&mut server, 0, 7, empty_logs_response());
        let rejected_tail = mock_get_logs_range(&mut server, 8, 9, unrecognized_range_error_response()).expect(2);
        let first_block = mock_get_logs_range(&mut server, 8, 8, empty_logs_response());
        let second_block = mock_get_logs_range(&mut server, 9, 9, single_log_response(9));

        let filters = FilterSet {
            all: vec![Filter::new()],
            token: vec![],
            no_token: vec![Filter::new()],
        };
        let blocks = rpc.try_stream_logs(0, filters, true)?.take(2).collect::<Vec<_>>().await;

        block_number.assert();
        replayed.assert();
        rejected_tail.assert();
        first_block.assert();
        second_block.assert();
        let block_ids_with_log_counts = blocks
            .iter()
            .map(|block| (block.block_id, block.logs.len()))
            .collect::<Vec<_>>();
        assert_eq!(block_ids_with_log_counts, vec![(0, 0), (9, 1)]);

        Ok(())
    }

    #[tokio::test]
    async fn test_get_logs_for_address_shrinks_block_range_on_unrecognized_errors() -> anyhow::Result<()> {
        let mut server = mockito::Server::new_async().await;
        let rpc = create_test_rpc_operations_with_max_range(&server.url(), 4)?;
        let too_large = mock_get_logs_range_with_status(&mut server, 0, 3, 413, "request entity too large".into());
        let first_range = mock_get_logs_range(&mut server, 0, 1, empty_logs_response());
        let second_range = mock_get_logs_range(&mut server, 2, 3, single_log_response(3));

        let logs = rpc
            .get_logs_for_address(Address::from([0_u8; 20]), vec![B256::ZERO], 0, 3)
            .await?;

        too_large.assert();
        first_range.assert();
        second_range.assert();
        assert_eq!(logs.len(), 1);
        assert_eq!(logs[0].block_number, 3);

        Ok(())
    }

    #[tokio::test]
    async fn test_get_logs_for_address_surfaces_single_block_errors() -> anyhow::Result<()> {
        let mut server = mockito::Server::new_async().await;
        let rpc = create_test_rpc_operations_with_max_range(&server.url(), 4)?;
        let failing = mock_get_logs_range_with_status(&mut server, 7, 7, 500, "{}".into());

        let result = rpc
            .get_logs_for_address(Address::from([0_u8; 20]), vec![B256::ZERO], 7, 7)
            .await;

        failing.assert();
        assert!(matches!(result, Err(RpcError::AlloyRpcError(_))));
        assert_eq!(rpc.log_block_range_limit(), 4);

        Ok(())
    }

    /// Before the adaptive range reacted to unrecognized errors, the indexer stream re-requested the rejected range
    /// until `max_indexer_loop_failures` was reached and then panicked.
    #[test_log::test(tokio::test)]
    async fn test_try_stream_logs_keeps_indexing_when_block_range_is_rejected() -> anyhow::Result<()> {
        let mut server = mockito::Server::new_async().await;
        let rpc = create_test_rpc_operations_with_max_range(&server.url(), 8)?;

        // Chain head 10 with the default depth of 3 makes block 7 the confirmed tip.
        let block_number = mock_block_number_batch(&mut server, 10);
        let gateway_timeout = mock_get_logs_range_with_status(&mut server, 0, 7, 504, "gateway timeout".into());
        let unknown_error = mock_get_logs_range(&mut server, 0, 3, unrecognized_range_error_response());
        let working_low = mock_get_logs_range(&mut server, 0, 1, empty_logs_response());
        let working_limit = mock_get_logs_range(&mut server, 2, 4, empty_logs_response());
        let stable_reuse = mock_get_logs_range(&mut server, 5, 7, single_log_response(6));

        let filters = FilterSet {
            all: vec![Filter::new()],
            token: vec![],
            no_token: vec![Filter::new()],
        };
        let blocks = rpc.try_stream_logs(0, filters, true)?.take(2).collect::<Vec<_>>().await;

        block_number.assert();
        gateway_timeout.assert();
        unknown_error.assert();
        working_low.assert();
        working_limit.assert();
        stable_reuse.assert();
        let block_ids_with_log_counts = blocks
            .iter()
            .map(|block| (block.block_id, block.logs.len()))
            .collect::<Vec<_>>();
        assert_eq!(block_ids_with_log_counts, vec![(0, 0), (6, 1)]);

        Ok(())
    }
    #[test]
    fn test_snapshot_from_multicall_returns() {
        let owner = AlloyAddress::from([0x11; 20]);
        let owners_return = getOwnersCall::abi_encode_returns(&vec![owner]);
        let threshold_return = getThresholdCall::abi_encode_returns(&TestU256::from(2_u64));

        let snapshot = snapshot_from_returns(&owners_return, &threshold_return).expect("decodes");
        assert_eq!(snapshot.owners, vec![owner.to_hopr_address()]);
        assert_eq!(snapshot.threshold, 2);

        // The empty return data of a reverted or non-Safe target does not decode.
        assert!(snapshot_from_returns(&[], &threshold_return).is_err());
    }

    #[test]
    fn test_pruned_state_errors_are_detected() {
        assert!(RpcError::Other("missing trie node abc (path )".into()).is_state_pruned());
        assert!(RpcError::Other("State already discarded for block 5".into()).is_state_pruned());
        assert!(!RpcError::Other("execution reverted".into()).is_state_pruned());
    }

    /// The stream waits for the tracker to publish the next confirmed tip instead of sleeping for a
    /// full expected block time.
    #[test_log::test(tokio::test)]
    async fn test_try_stream_logs_advances_on_tip_update() -> anyhow::Result<()> {
        let mut server = mockito::Server::new_async().await;
        let mut rpc = create_test_rpc_operations(&server.url())?;

        let observation = Arc::new(Mutex::new(TipObservation {
            latest: 8,
            latest_timestamp: Some(1),
            safe: None,
            finalized: None,
        }));
        let tracker = ChainTipTracker::new(
            Arc::new(SharedTipSource(observation.clone())),
            ConfirmationConfig {
                mode: ConfirmationMode::Depth,
                depth: 3,
                max_safe_gap: 32,
            },
            Duration::from_secs(3600),
            Duration::from_secs(60),
        );
        rpc.replace_tip_tracker(Arc::new(tracker));

        let first_round = mock_get_logs_range(&mut server, 0, 5, empty_logs_response());
        let second_round = mock_get_logs_range(&mut server, 6, 6, empty_logs_response());

        let filters = FilterSet {
            all: vec![Filter::new()],
            token: vec![],
            no_token: vec![Filter::new()],
        };
        let mut blocks = rpc.try_stream_logs(0, filters, true)?;
        let started = Instant::now();

        let first = tokio::time::timeout(Duration::from_secs(2), blocks.next())
            .await
            .context("the first block should arrive without waiting for the expected block time")?
            .context("the stream should yield the first block")?;
        assert_eq!(first.block_id, 0);

        // While the confirmed tip is still 5, the next item must wait for the watch channel instead
        // of returning from a cache read.
        let next_block = blocks.next();
        futures::pin_mut!(next_block);
        assert!(
            futures::poll!(next_block.as_mut()).is_pending(),
            "the stream must wait for a tip update before yielding block 6"
        );

        *observation
            .lock()
            .expect("shared observation lock should not be poisoned") = TipObservation {
            latest: 9,
            latest_timestamp: Some(2),
            safe: None,
            finalized: None,
        };
        rpc.tip_tracker().refresh().await?;

        let second = tokio::time::timeout(Duration::from_secs(2), next_block)
            .await
            .context("a tip update should advance the stream without waiting for the expected block time")?
            .context("the stream should yield the next block")?;
        assert_eq!(second.block_id, 6);
        assert!(
            started.elapsed() < Duration::from_secs(10),
            "the stream should advance on the tip update, not after the 60 second expected block time"
        );

        first_round.assert();
        second_round.assert();

        Ok(())
    }

    /// A head lag sample never fetches a missing header from the endpoint.
    #[test_log::test(tokio::test)]
    async fn test_head_lag_reads_only_cached_headers() -> anyhow::Result<()> {
        let mut server = mockito::Server::new_async().await;
        let rpc = create_test_rpc_operations(&server.url())?;

        let latest = mock_block_number_batch(&mut server, 8);
        let header = server
            .mock("POST", "/")
            .match_body(mockito::Matcher::Regex("\"0x5\"".into()))
            .expect(0)
            .create();

        rpc.tip_tracker().refresh().await?;
        assert_eq!(
            rpc.cached_header_timestamp(8),
            Some(1),
            "a cached height must not cost an RPC call"
        );
        assert_eq!(
            rpc.cached_header_timestamp(5),
            None,
            "an unobserved height has no sample"
        );
        latest.assert();
        header.assert();

        Ok(())
    }

    /// The bounded tip wait leaves the stream again once it times out, and a tip update wakes it
    /// before the limit is reached.
    #[tokio::test]
    async fn test_confirmed_tip_wait_times_out_and_resumes_on_a_tip_update() {
        let tip = |latest: u64, confirmed: u64| ChainTip {
            latest,
            safe: None,
            finalized: None,
            confirmed,
            source: HeadSource::Polling,
            fcr: FcrState::Unavailable,
        };
        let (tip_tx, mut tip_rx) = watch::channel(Some(tip(8, 5)));

        assert!(
            wait_for_confirmed_tip(&mut tip_rx, 6, Duration::from_millis(20)).await,
            "the wait must time out while the tip stays at 5"
        );

        tip_tx.send_replace(Some(tip(9, 6)));
        assert!(
            !wait_for_confirmed_tip(&mut tip_rx, 6, Duration::from_secs(1)).await,
            "a tip update must complete the wait before the limit"
        );
    }
}
