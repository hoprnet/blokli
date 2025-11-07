//! This crate contains types and traits that ensure correct interfacing with Ethereum RPC providers.
//!
//! The most important trait is [HoprRpcOperations] which allows to send arbitrary on-chain transactions
//! and also to perform the selection of HOPR-related smart contract operations.
//! Secondly, the [HoprIndexerRpcOperations] is a trait that contains all operations required by the
//! Indexer to subscribe to the block with logs from the chain.
//!
//! Both of these traits implemented and realized via the [RpcOperations](rpc::RpcOperations) type,
//! so this represents the main entry point to all RPC related operations.

extern crate core;

use std::{
    cmp::Ordering,
    collections::BTreeSet,
    fmt::{Display, Formatter},
    pin::Pin,
    time::Duration,
};

use alloy::{primitives::B256, providers::PendingTransaction, rpc::types::TransactionRequest};
use async_trait::async_trait;
use errors::LogConversionError;
use futures::Stream;
use hopr_crypto_types::types::Hash;
use hopr_internal_types::prelude::WinningProbability;
use hopr_primitive_types::prelude::*;
use serde::{Deserialize, Serialize};

use crate::{RetryAction::NoRetry, errors::Result};

pub mod client;
pub mod errors;
pub mod indexer;
pub mod rpc;
pub mod transport;

#[cfg(feature = "runtime-tokio")]
pub use crate::transport::ReqwestClient;

/// A type containing selected fields from  the `eth_getLogs` RPC calls.
///
/// This is further restricted to already mined blocks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Log {
    /// Contract address
    pub address: Address,
    /// Topics
    pub topics: Vec<Hash>,
    /// Raw log data
    pub data: Box<[u8]>,
    /// Transaction index
    pub tx_index: u64,
    /// Corresponding block number
    pub block_number: u64,
    /// Corresponding block hash
    pub block_hash: Hash,
    /// Corresponding transaction hash
    pub tx_hash: Hash,
    /// Log index
    pub log_index: U256,
    /// Removed flag
    pub removed: bool,
}

impl TryFrom<alloy::rpc::types::Log> for Log {
    type Error = LogConversionError;

    fn try_from(value: alloy::rpc::types::Log) -> std::result::Result<Self, Self::Error> {
        Ok(Self {
            address: value.address().into(),
            topics: value.topics().iter().map(|t| Hash::from(t.0)).collect(),
            data: Box::from(value.data().data.as_ref()),
            tx_index: value
                .transaction_index
                .ok_or(LogConversionError::MissingTransactionIndex)?,
            block_number: value.block_number.ok_or(LogConversionError::MissingBlockNumber)?,
            block_hash: value.block_hash.ok_or(LogConversionError::MissingBlockHash)?.0.into(),
            log_index: value.log_index.ok_or(LogConversionError::MissingLogIndex)?.into(),
            tx_hash: value
                .transaction_hash
                .ok_or(LogConversionError::MissingTransactionHash)?
                .0
                .into(),
            removed: value.removed,
        })
    }
}

impl From<Log> for alloy::rpc::types::RawLog {
    fn from(value: Log) -> Self {
        alloy::rpc::types::RawLog {
            address: value.address.into(),
            topics: value.topics.into_iter().map(|h| B256::from_slice(h.as_ref())).collect(),
            data: value.data.into(),
        }
    }
}

impl From<SerializableLog> for Log {
    fn from(value: SerializableLog) -> Self {
        let topics = value
            .topics
            .into_iter()
            .map(|topic| topic.into())
            .collect::<Vec<Hash>>();

        Self {
            address: value.address,
            topics,
            data: Box::from(value.data.as_ref()),
            tx_index: value.tx_index,
            block_number: value.block_number,
            block_hash: value.block_hash.into(),
            log_index: value.log_index.into(),
            tx_hash: value.tx_hash.into(),
            removed: value.removed,
        }
    }
}

impl From<Log> for SerializableLog {
    fn from(value: Log) -> Self {
        SerializableLog {
            address: value.address,
            topics: value.topics.into_iter().map(|t| t.into()).collect(),
            data: value.data.into_vec(),
            tx_index: value.tx_index,
            block_number: value.block_number,
            block_hash: value.block_hash.into(),
            tx_hash: value.tx_hash.into(),
            log_index: value.log_index.as_u64(),
            removed: value.removed,
            // These fields stay empty for logs coming from the chain and will be populated by the
            // indexer when processing the log.
            processed: None,
            processed_at: None,
            checksum: None,
        }
    }
}

impl Display for Log {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "log #{} in tx #{} in block #{} of address {} with {} topics",
            self.log_index,
            self.tx_index,
            self.block_number,
            self.address,
            self.topics.len()
        )
    }
}

impl Ord for Log {
    fn cmp(&self, other: &Self) -> Ordering {
        let blocks = self.block_number.cmp(&other.block_number);
        if blocks == Ordering::Equal {
            let tx_indices = self.tx_index.cmp(&other.tx_index);
            if tx_indices == Ordering::Equal {
                self.log_index.cmp(&other.log_index)
            } else {
                tx_indices
            }
        } else {
            blocks
        }
    }
}

impl PartialOrd<Self> for Log {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// Represents a set of categorized blockchain log filters for optimized indexer performance.
///
/// This structure organizes filters into different categories to enable selective log
/// processing based on the indexer's operational state. During initial synchronization,
/// the indexer uses `no_token` filters to exclude irrelevant token events, significantly
/// reducing processing time and storage requirements. During normal operation, it uses
/// `all` filters for complete event coverage.
///
/// The `token` filters specifically target token-related events for the node's safe address.
#[derive(Debug, Clone, Default)]
pub struct FilterSet {
    /// holds all filters for the indexer
    pub all: Vec<alloy::rpc::types::Filter>,
    /// holds only the token contract related filters
    pub token: Vec<alloy::rpc::types::Filter>,
    /// holds only filters not related to the token contract
    pub no_token: Vec<alloy::rpc::types::Filter>,
}

/// Indicates what retry action should be taken, as result of a `RetryPolicy` implementation.
pub enum RetryAction {
    /// Request should not be retried
    NoRetry,
    /// Request should be retried after the given duration has elapsed.
    RetryAfter(Duration),
}

/// Simple retry policy trait
pub trait RetryPolicy<E> {
    /// Indicates whether a client should retry the request given the last error, current number of retries
    /// of this request and the number of other requests being retried by the client at this time.
    fn is_retryable_error(&self, _err: &E, _retry_number: u32, _retry_queue_size: u32) -> RetryAction {
        NoRetry
    }
}

/// Common configuration for all native `HttpPostRequestor`s
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, smart_default::SmartDefault)]
pub struct HttpPostRequestorConfig {
    /// Timeout for HTTP POST request
    ///
    /// Defaults to 30 seconds.
    #[default(Duration::from_secs(30))]
    pub http_request_timeout: Duration,

    /// Maximum number of HTTP redirects to follow
    ///
    /// Defaults to 3
    #[default(3)]
    pub max_redirects: u8,

    /// Maximum number of requests per second.
    /// If set to Some(0) or `None`, there will be no limit.
    ///
    /// Defaults to 10
    #[default(Some(10))]
    pub max_requests_per_sec: Option<u32>,
}

/// Represents the on-chain status for the Node Safe module.
#[derive(Clone, Debug, Copy, PartialEq, Eq)]
pub struct NodeSafeModuleStatus {
    pub is_node_included_in_module: bool,
    pub is_module_enabled_in_safe: bool,
    pub is_safe_owner_of_module: bool,
}

impl NodeSafeModuleStatus {
    /// Determines if the node passes all status checks.
    pub fn should_pass(&self) -> bool {
        self.is_node_included_in_module && self.is_module_enabled_in_safe && self.is_safe_owner_of_module
    }
}

/// Trait defining a general set of operations an RPC provider
/// must provide to the HOPR node.
#[async_trait]
pub trait HoprRpcOperations {
    /// Retrieves the timestamp from the given block number.
    async fn get_timestamp(&self, block_number: u64) -> Result<Option<u64>>;

    /// Retrieves on-chain xdai balance of the given address.
    async fn get_xdai_balance(&self, address: Address) -> Result<XDaiBalance>;

    /// Retrieves on-chain wxHOPR token balance of the given address.
    async fn get_hopr_balance(&self, address: Address) -> Result<HoprBalance>;

    /// Retrieves the wxHOPR token allowance for the given owner and spender.
    async fn get_hopr_allowance(&self, owner: Address, spender: Address) -> Result<HoprBalance>;

    /// Retrieves the minimum incoming ticket winning probability by directly
    /// calling the network's winning probability oracle.
    async fn get_minimum_network_winning_probability(&self) -> Result<WinningProbability>;

    /// Retrieves the minimum ticket prices by directly calling the network's
    /// ticket price oracle.
    async fn get_minimum_network_ticket_price(&self) -> Result<HoprBalance>;

    /// Retrieves the safe address of the given node address from the registry.
    async fn get_safe_from_node_safe_registry(&self, node: Address) -> Result<Address>;

    /// Retrieves the notice period of channel closure from the Channels contract.
    async fn get_channel_closure_notice_period(&self) -> Result<Duration>;

    /// Sends transaction to the RPC provider, does not await confirmation.
    async fn send_transaction(&self, tx: TransactionRequest) -> Result<PendingTransaction>;

    /// Sends transaction to the RPC provider, awaits confirmation.
    async fn send_transaction_with_confirm(&self, tx: TransactionRequest) -> Result<Hash>;
}

/// Structure containing filtered logs that all belong to the same block.
#[derive(Debug, Clone, Default)]
pub struct BlockWithLogs {
    /// Block number
    pub block_id: u64,
    /// Filtered logs belonging to this block.
    pub logs: BTreeSet<SerializableLog>,
}

impl Display for BlockWithLogs {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "block #{} with {} logs", self.block_id, self.logs.len())
    }
}

impl BlockWithLogs {
    /// Returns `true` if no logs are contained within this block.
    pub fn is_empty(&self) -> bool {
        self.logs.is_empty()
    }

    /// Returns the number of logs within this block.
    pub fn len(&self) -> usize {
        self.logs.len()
    }
}

/// Trait with RPC provider functionality required by the Indexer.
#[async_trait]
pub trait HoprIndexerRpcOperations {
    /// Retrieves the latest block number.
    async fn block_number(&self) -> Result<u64>;

    /// Queries the HOPR token allowance between owner and spender addresses.
    ///
    /// This method queries the HOPR token contract to determine how many tokens
    /// the owner has approved the spender to transfer on their behalf.
    ///
    /// # Arguments
    /// * `owner` - The address that owns the tokens and grants the allowance
    /// * `spender` - The address that is approved to spend the tokens
    ///
    /// # Returns
    /// * `Result<HoprBalance>` - The current allowance amount
    async fn get_hopr_allowance(&self, owner: Address, spender: Address) -> Result<HoprBalance>;

    /// Queries the xDAI (native token) balance for a specific address.
    ///
    /// This method queries the current xDAI balance of the specified address
    /// from the blockchain.
    ///
    /// # Arguments
    /// * `address` - The Ethereum address to query the balance for
    ///
    /// # Returns
    /// * `Result<XDaiBalance>` - The current xDAI balance
    async fn get_xdai_balance(&self, address: Address) -> Result<XDaiBalance>;

    /// Queries the HOPR token balance for a specific address.
    ///
    /// This method directly queries the HOPR token contract to get the current
    /// token balance of the specified address.
    ///
    /// # Arguments
    /// * `address` - The Ethereum address to query the balance for
    ///
    /// # Returns
    /// * `Result<HoprBalance>` - The current HOPR token balance
    async fn get_hopr_balance(&self, address: Address) -> Result<HoprBalance>;

    /// Streams blockchain logs using selective filtering based on synchronization state.
    ///
    /// This method intelligently selects which log filters to use based on whether
    /// the indexer is currently syncing historical data or processing live events.
    /// During initial sync, it uses `no_token` filters to exclude irrelevant token
    /// events. When synced, it uses all filters to capture complete event data.
    ///
    /// # Arguments
    /// * `start_block_number` - Starting block number for log retrieval
    /// * `filters` - Set of categorized filters (all, token, no_token)
    /// * `is_synced` - Whether the indexer has completed initial synchronization
    ///
    /// # Returns
    /// * `impl Stream<Item = Result<Log>>` - Stream of blockchain logs
    ///
    /// # Behavior
    /// * When `is_synced` is `false`: Uses `filter_set.no_token` to reduce log volume
    /// * When `is_synced` is `true`: Uses `filter_set.all` for complete coverage
    fn try_stream_logs<'a>(
        &'a self,
        start_block_number: u64,
        filters: FilterSet,
        is_synced: bool,
    ) -> Result<Pin<Box<dyn Stream<Item = BlockWithLogs> + Send + 'a>>>;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_log(block_number: u64, tx_index: u64, log_index: u64) -> Log {
        Log {
            address: Address::default(),
            topics: vec![Hash::default()],
            data: vec![1, 2, 3].into(),
            tx_index,
            block_number,
            block_hash: Hash::default(),
            log_index: log_index.into(),
            tx_hash: Hash::default(),
            removed: false,
        }
    }

    #[test]
    fn test_log_conversion_to_raw_log() {
        let log = create_test_log(100, 5, 10);

        let raw_log: alloy::rpc::types::RawLog = log.clone().into();

        // Verify the raw log has the expected topics and data
        assert_eq!(raw_log.topics.len(), 1);
        assert_eq!(raw_log.data.len(), 3);
    }

    #[test]
    fn test_log_conversion_from_alloy_success() {
        let alloy_log = alloy::rpc::types::Log {
            inner: alloy::primitives::Log {
                address: alloy::primitives::Address::ZERO,
                data: alloy::primitives::LogData::new_unchecked(
                    vec![alloy::primitives::B256::ZERO],
                    vec![1, 2, 3].into(),
                ),
            },
            block_hash: Some(alloy::primitives::B256::ZERO),
            block_number: Some(200),
            block_timestamp: None,
            transaction_hash: Some(alloy::primitives::B256::ZERO),
            transaction_index: Some(3),
            log_index: Some(7),
            removed: false,
        };

        let result: std::result::Result<Log, LogConversionError> = alloy_log.try_into();
        assert!(result.is_ok());

        let log = result.unwrap();
        assert_eq!(log.block_number, 200);
        assert_eq!(log.tx_index, 3);
        assert_eq!(log.log_index, primitive_types::U256::from(7));
        assert_eq!(log.removed, false);
    }

    #[test]
    fn test_log_conversion_missing_block_number_fails() {
        let alloy_log = alloy::rpc::types::Log {
            inner: alloy::primitives::Log {
                address: alloy::primitives::Address::ZERO,
                data: alloy::primitives::LogData::new_unchecked(
                    vec![alloy::primitives::B256::ZERO],
                    vec![1, 2, 3].into(),
                ),
            },
            block_hash: Some(alloy::primitives::B256::ZERO),
            block_number: None, // Missing block number
            block_timestamp: None,
            transaction_hash: Some(alloy::primitives::B256::ZERO),
            transaction_index: Some(3),
            log_index: Some(7),
            removed: false,
        };

        let result: std::result::Result<Log, LogConversionError> = alloy_log.try_into();
        assert!(result.is_err());

        if let Err(e) = result {
            assert!(matches!(e, LogConversionError::MissingBlockNumber));
        }
    }

    #[test]
    fn test_log_conversion_missing_tx_index_fails() {
        let alloy_log = alloy::rpc::types::Log {
            inner: alloy::primitives::Log {
                address: alloy::primitives::Address::ZERO,
                data: alloy::primitives::LogData::new_unchecked(
                    vec![alloy::primitives::B256::ZERO],
                    vec![1, 2, 3].into(),
                ),
            },
            block_hash: Some(alloy::primitives::B256::ZERO),
            block_number: Some(200),
            block_timestamp: None,
            transaction_hash: Some(alloy::primitives::B256::ZERO),
            transaction_index: None, // Missing tx index
            log_index: Some(7),
            removed: false,
        };

        let result: std::result::Result<Log, LogConversionError> = alloy_log.try_into();
        assert!(result.is_err());

        if let Err(e) = result {
            assert!(matches!(e, LogConversionError::MissingTransactionIndex));
        }
    }

    #[test]
    fn test_log_conversion_missing_log_index_fails() {
        let alloy_log = alloy::rpc::types::Log {
            inner: alloy::primitives::Log {
                address: alloy::primitives::Address::ZERO,
                data: alloy::primitives::LogData::new_unchecked(
                    vec![alloy::primitives::B256::ZERO],
                    vec![1, 2, 3].into(),
                ),
            },
            block_hash: Some(alloy::primitives::B256::ZERO),
            block_number: Some(200),
            block_timestamp: None,
            transaction_hash: Some(alloy::primitives::B256::ZERO),
            transaction_index: Some(3),
            log_index: None, // Missing log index
            removed: false,
        };

        let result: std::result::Result<Log, LogConversionError> = alloy_log.try_into();
        assert!(result.is_err());

        if let Err(e) = result {
            assert!(matches!(e, LogConversionError::MissingLogIndex));
        }
    }

    #[test]
    fn test_log_to_serializable() {
        let log = create_test_log(150, 8, 12);

        let serializable: SerializableLog = log.clone().into();

        assert_eq!(serializable.block_number, 150);
        assert_eq!(serializable.tx_index, 8);
        assert_eq!(serializable.log_index, 12);
        assert_eq!(serializable.removed, false);
        assert!(serializable.processed.is_none());
        assert!(serializable.processed_at.is_none());
        assert!(serializable.checksum.is_none());
    }

    #[test]
    fn test_log_ordering_by_block() {
        let log1 = create_test_log(100, 0, 0);
        let log2 = create_test_log(101, 0, 0);

        assert!(log1 < log2);
        assert!(log2 > log1);
    }

    #[test]
    fn test_log_ordering_by_tx_index() {
        let log1 = create_test_log(100, 0, 0);
        let log2 = create_test_log(100, 1, 0);

        assert!(log1 < log2);
        assert!(log2 > log1);
    }

    #[test]
    fn test_log_ordering_by_log_index() {
        let log1 = create_test_log(100, 0, 0);
        let log2 = create_test_log(100, 0, 1);

        assert!(log1 < log2);
        assert!(log2 > log1);
    }

    #[test]
    fn test_log_ordering_complex() {
        let log1 = create_test_log(100, 0, 0);
        let log2 = create_test_log(100, 0, 1);
        let log3 = create_test_log(100, 1, 0);
        let log4 = create_test_log(101, 0, 0);

        assert!(log1 < log2);
        assert!(log2 < log3);
        assert!(log3 < log4);

        // Test transitivity
        assert!(log1 < log3);
        assert!(log1 < log4);
        assert!(log2 < log4);
    }

    #[test]
    fn test_log_equality() {
        let log1 = create_test_log(100, 5, 10);
        let log2 = create_test_log(100, 5, 10);

        assert_eq!(log1, log2);
        assert!(!(log1 < log2));
        assert!(!(log1 > log2));
    }

    #[test]
    fn test_log_display() {
        let log = create_test_log(100, 5, 10);
        let display_str = format!("{}", log);

        assert!(display_str.contains("log #10"));
        assert!(display_str.contains("tx #5"));
        assert!(display_str.contains("block #100"));
    }

    #[test]
    fn test_block_with_logs_is_empty() {
        let block = BlockWithLogs {
            block_id: 100,
            logs: BTreeSet::new(),
        };

        assert!(block.is_empty());
        assert_eq!(block.len(), 0);
    }

    #[test]
    fn test_block_with_logs_display() {
        let block = BlockWithLogs {
            block_id: 100,
            logs: BTreeSet::new(),
        };

        let display_str = format!("{}", block);
        assert!(display_str.contains("block #100"));
        assert!(display_str.contains("0 logs"));
    }
}
