//! Extends the [RpcOperations] type with functionality needed by the Indexer component.
//!
//! The functionality required functionality is defined in the [HoprIndexerRpcOperations] trait,
//! which is implemented for [RpcOperations] hereof.
//! The primary goal is to provide a stream of [BlockWithLogs] filtered by the given [LogFilter]
//! as the new matching blocks are mined in the underlying blockchain. The stream also allows to collect
//! historical blockchain data.
//!
//! For details on the Indexer see the `chain-indexer` crate.
use std::{cmp::Ordering, future::Future, pin::Pin, time::Duration};

use async_stream::stream;
use async_trait::async_trait;
use blokli_chain_types::AlloyAddressExt;
use futures::{Stream, StreamExt, stream::BoxStream};
use hopr_bindings::exports::alloy::{
    eips::eip2718::Encodable2718,
    primitives::{Address as AlloyAddress, B256, U256},
    providers::Provider,
    rpc::types::{Filter, Log as AlloyLog},
};
#[cfg(all(feature = "telemetry", not(test)))]
use hopr_types::telemetry::SimpleGauge;
use hopr_types::{crypto::types::Hash, primitive::prelude::*};
use rust_stream_ext_concurrent::then_concurrent::StreamThenConcurrentExt;
use tracing::{debug, error, trace, warn};

use crate::{
    BlockWithLogs, FilterSet, HoprIndexerRpcOperations, Log,
    errors::{Result, RpcError, RpcError::FilterIsEmpty},
    rpc::{HoprModule, RpcOperations, SafeSingleton, is_log_block_range_limit_error},
    transport::HttpRequestor,
};

#[cfg(all(feature = "telemetry", not(test)))]
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

fn contains_log_block_range_limit_error(results: &[Result<Log>]) -> Option<&RpcError> {
    results.iter().find_map(|result| match result {
        Err(error) if is_log_block_range_limit_error(error) => Some(error),
        _ => None,
    })
}

fn contains_rpc_fetch_error(results: &[Result<Log>]) -> bool {
    results
        .iter()
        .any(|result| matches!(result, Err(RpcError::AlloyRpcError(_))))
}

// impl<P: JsonRpcClient + 'static, R: HttpRequestor + 'static> RpcOperations<P, R> {
impl<R: HttpRequestor + 'static + Clone> RpcOperations<R> {
    /// Retrieves logs in the given range (`from_block` and `to_block` are inclusive).
    fn stream_logs(&self, filters: Vec<Filter>, from_block: u64, to_block: u64) -> BoxStream<'_, Result<Log>> {
        stream! {
            let mut next_block = from_block;

            while next_block <= to_block {
                let attempted_limit = self.log_block_range_limit();
                let end_block = to_block.min(next_block.saturating_add(attempted_limit.saturating_sub(1)));
                let requested_span = end_block - next_block + 1;
                let ranged_filters = filters
                    .iter()
                    .cloned()
                    .map(|filter| filter.from_block(next_block).to_block(end_block))
                    .collect::<Vec<_>>();

                debug!(
                    from_block = next_block,
                    to_block = end_block,
                    attempted_block_range = attempted_limit,
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

                if let Some(error) = contains_log_block_range_limit_error(&results) {
                    let update = self.record_log_block_range_failure(requested_span, error);
                    if update.retry {
                        continue;
                    }
                } else if !contains_rpc_fetch_error(&results) {
                    self.record_log_block_range_success(requested_span, attempted_limit);
                }

                for result in results {
                    yield result;
                }

                next_block = end_block + 1;
            }
        }
        .boxed()
    }
}

#[async_trait]
impl<R: HttpRequestor + 'static + Clone> HoprIndexerRpcOperations for RpcOperations<R> {
    async fn block_number(&self) -> Result<u64> {
        self.get_block_number().await
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


                        #[cfg(all(feature = "telemetry", not(test)))]
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
                    }

                    Err(e) => error!(error = %e, "failed to obtain current block number from chain")
                }

                futures_timer::Delay::new(self.cfg.expected_block_time).await;
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

        while next_block <= to_block {
            let attempted_limit = self.log_block_range_limit();
            let end_block = to_block.min(next_block.saturating_add(attempted_limit.saturating_sub(1)));
            let requested_span = end_block - next_block + 1;
            let filter = Filter::new()
                .address(AlloyAddress::from_hopr_address(address))
                .event_signature(topics.clone())
                .from_block(next_block)
                .to_block(end_block);

            match self.provider.get_logs(&filter).await {
                Ok(chunk_logs) => {
                    self.record_log_block_range_success(requested_span, attempted_limit);

                    let mut converted_logs = chunk_logs
                        .into_iter()
                        .map(Log::try_from)
                        .collect::<std::result::Result<Vec<_>, _>>()?;
                    logs.append(&mut converted_logs);
                    next_block = end_block + 1;
                }
                Err(error) => {
                    let rpc_error = RpcError::from(error);

                    if is_log_block_range_limit_error(&rpc_error) {
                        let update = self.record_log_block_range_failure(requested_span, &rpc_error);
                        if update.retry {
                            continue;
                        }
                    }

                    return Err(rpc_error);
                }
            }
        }

        logs.sort();
        Ok(logs)
    }

    async fn get_xdai_balance(&self, address: Address) -> Result<XDaiBalance> {
        self.get_xdai_balance(address).await
    }

    async fn get_hopr_balance(&self, address: Address) -> Result<HoprBalance> {
        self.get_hopr_balance(address).await
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
    use std::time::Duration;

    use anyhow::Context;
    use async_trait::async_trait;
    use blokli_chain_types::ContractAddresses;
    use futures::StreamExt;
    use hopr_bindings::exports::alloy::{
        primitives::{Address as AlloyAddress, B256, Log as AlloyPrimitiveLog, LogData},
        rpc::{
            client::ClientBuilder,
            types::{Filter, Log as AlloyLog},
        },
        transports::http::ReqwestTransport,
    };
    use hopr_types::primitive::prelude::Address;
    use serde_json::json;
    use tokio::time::sleep;
    use url::Url;

    use crate::{
        HoprIndexerRpcOperations,
        errors::{HttpRequestError, RpcError},
        indexer::{fetch_subrange_logs_concurrently, split_range},
        rpc::{RpcOperations, RpcOperationsConfig},
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
        server
            .mock("POST", "/")
            .match_body(mockito::Matcher::AllOf(vec![
                mockito::Matcher::Regex(r#""method"\s*:\s*"eth_getLogs""#.to_string()),
                mockito::Matcher::Regex(format!(r#""fromBlock"\s*:\s*"0x{from_block:x}""#)),
                mockito::Matcher::Regex(format!(r#""toBlock"\s*:\s*"0x{to_block:x}""#)),
            ]))
            .with_status(200)
            .with_body(body)
            .expect(1)
            .create()
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

        Ok(())
    }
}
