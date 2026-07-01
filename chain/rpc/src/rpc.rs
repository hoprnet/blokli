//! General purpose high-level RPC operations implementation (`HoprRpcOperations`).
//!
//! The purpose of this module is to give implementation of the [HoprRpcOperations] trait:
//! [RpcOperations] type, which is the main API exposed by this crate.
use std::{
    sync::{Arc, Mutex},
    time::Duration,
};

use async_trait::async_trait;
use blokli_chain_types::{AlloyAddressExt, ContractAddresses, ContractInstances};
use hopr_bindings::exports::alloy::{
    primitives::{Address as AlloyAddress, FixedBytes, U256},
    providers::{
        Identity, PendingTransaction, Provider, ProviderBuilder, RootProvider,
        fillers::{BlobGasFiller, CachedNonceManager, ChainIdFiller, FillProvider, GasFiller, JoinFill, NonceFiller},
    },
    rpc::{
        client::RpcClient,
        types::{Block, TransactionRequest},
    },
    sol,
};
use hopr_types::{
    crypto::prelude::Hash,
    internal::prelude::{EncodedWinProb, WinningProbability},
    primitive::prelude::*,
};
use serde::{Deserialize, Serialize};
use tracing::{debug, warn};
use url::Url;
use validator::Validate;

// use crate::middleware::GnosisScan;
use crate::{
    Eip1559FeeEstimation, HoprRpcOperations,
    client::GasOracleFiller,
    errors::{Result, RpcError},
    transport::HttpRequestor,
};

// define basic safe abi
sol!(
    #![sol(abi)]
    #![sol(rpc)]
    contract SafeSingleton {
        function isModuleEnabled(address module) public view returns (bool);
        function nonce() public view returns (uint256);
        function getThreshold() public view returns (uint256);
        function getModulesPaginated(address start, uint256 pageSize) public view returns (address[] memory array, address next);
    }
);

// minimal interface for verifying HOPR modules
sol!(
    #![sol(abi)]
    #![sol(rpc)]
    contract HoprModule {
        function isHoprNodeManagementModule() external view returns (bool);
    }
);

/// Default gas oracle URL for Gnosis chain.
pub const DEFAULT_GAS_ORACLE_URL: &str = "https://ggnosis.blockscan.com/gasapi.ashx?apikey=key&method=gasoracle";

/// Default upper bound for adaptive `eth_getLogs` block range discovery.
pub const DEFAULT_AUTO_LOG_BLOCK_RANGE_CAP: u64 = 10_000;

/// Configuration of the RPC related parameters.
#[derive(Clone, Debug, PartialEq, Eq, smart_default::SmartDefault, Serialize, Deserialize, Validate)]
pub struct RpcOperationsConfig {
    /// Blockchain id
    ///
    /// Default is 100.
    #[default = 100]
    pub chain_id: u64,
    /// Addresses of all deployed contracts
    ///
    /// Default contains empty (null) addresses.
    pub contract_addrs: ContractAddresses,
    /// Expected block time of the blockchain
    ///
    /// Defaults to 5 seconds
    #[default(Duration::from_secs(5))]
    pub expected_block_time: Duration,
    /// The upper bound for adaptive log block range fetching.
    ///
    /// The RPC layer learns the effective value from real `eth_getLogs` requests and never requests more than this
    /// configured ceiling. A value of `0` uses [`DEFAULT_AUTO_LOG_BLOCK_RANGE_CAP`] as the ceiling.
    ///
    /// Defaults to 2000 blocks
    #[validate(range(min = 0))]
    #[default = 2000]
    pub max_block_range_fetch_size: u64,
    /// Interval for polling on TX submission
    ///
    /// Defaults to 7 seconds.
    #[default(Duration::from_secs(7))]
    pub tx_polling_interval: Duration,
    /// Finalization chain length
    ///
    /// The number of blocks including and decreasing from the chain HEAD
    /// that the logs will be buffered for before being considered
    /// successfully joined to the chain.
    ///
    /// Defaults to 3
    #[validate(range(min = 1, max = 100))]
    #[default = 3]
    pub finality: u32,
    /// URL to the gas price oracle.
    ///
    /// Defaults to [`DEFAULT_GAS_ORACLE_URL`].
    #[default(Some(DEFAULT_GAS_ORACLE_URL.parse().unwrap()))]
    pub gas_oracle_url: Option<Url>,
    /// Fallback max fee per gas for EIP-1559 transactions (in wei).
    ///
    /// Used when the gas oracle is unavailable or returns an error.
    ///
    /// Defaults to 3 gwei (3,000,000,000 wei) which is suitable for Gnosis chain.
    #[default = 3_000_000_000]
    pub gas_oracle_fallback_max_fee: u128,
    /// Fallback max priority fee per gas for EIP-1559 transactions (in wei).
    ///
    /// Used when the gas oracle is unavailable or returns an error.
    ///
    /// Defaults to 0.1 gwei (100,000,000 wei) which is suitable for Gnosis chain.
    #[default = 100_000_000]
    pub gas_oracle_fallback_priority_fee: u128,
    /// Maximum number of consecutive failures tolerated during log streaming before giving up.
    ///
    /// When the indexer encounters errors while fetching logs, it will retry up to this many times
    /// before panicking. This prevents infinite retry loops when RPC issues persist.
    ///
    /// Defaults to 5 failures.
    #[validate(range(min = 1, max = 20))]
    #[default = 5]
    pub max_indexer_loop_failures: usize,
    /// Maximum number of blocks the indexer can be ahead of the RPC provider.
    ///
    /// When requesting blocks that don't exist yet (indexer running ahead of RPC),
    /// this defines how far ahead is tolerable before considering it an error.
    ///
    /// Defaults to 50 blocks.
    #[validate(range(min = 1, max = 200))]
    #[default = 50]
    pub max_indexer_past_blocks: usize,
}

pub(crate) type HoprProvider<R> = FillProvider<
    JoinFill<
        JoinFill<
            JoinFill<JoinFill<JoinFill<Identity, ChainIdFiller>, NonceFiller<CachedNonceManager>>, GasFiller>,
            GasOracleFiller<R>,
        >,
        BlobGasFiller,
    >,
    RootProvider,
>;

#[derive(Debug)]
pub(crate) struct LogBlockRangeLimit {
    cap: u64,
    candidate: u64,
    last_working: u64,
    first_failing: Option<u64>,
    stable: Option<u64>,
    warned_single_block: bool,
}

impl LogBlockRangeLimit {
    fn new(configured_cap: u64) -> Self {
        let cap = sanitize_log_block_range_cap(configured_cap);

        Self {
            cap,
            candidate: cap,
            last_working: 0,
            first_failing: None,
            stable: None,
            warned_single_block: false,
        }
    }

    fn current(&self) -> u64 {
        self.stable.unwrap_or(self.candidate).clamp(1, self.cap)
    }

    fn record_success(&mut self, requested_span: u64, attempted_limit: u64) -> Option<u64> {
        if requested_span == 0 {
            return self.stable;
        }

        match self.first_failing {
            Some(first_failing) if requested_span < first_failing => {
                self.last_working = self.last_working.max(requested_span);

                if self.last_working + 1 >= first_failing {
                    let stable = self.last_working.max(1);
                    self.stable = Some(stable);
                    self.candidate = stable;
                } else if requested_span == attempted_limit {
                    self.candidate = next_probe(self.last_working, first_failing);
                }
            }
            Some(_) => {}
            None if requested_span == attempted_limit && attempted_limit >= self.cap => {
                self.last_working = self.cap;
                self.stable = Some(self.cap);
                self.candidate = self.cap;
            }
            None => {
                self.last_working = self.last_working.max(requested_span);
            }
        }

        self.stable
    }

    fn record_failure(&mut self, requested_span: u64) -> LogBlockRangeFailureUpdate {
        let failed_span = requested_span.max(1).min(self.cap);

        if failed_span <= 1 {
            self.first_failing = Some(1);
            self.stable = Some(1);
            self.candidate = 1;
            self.last_working = 0;

            let warn_single_block = !self.warned_single_block;
            self.warned_single_block = true;

            return LogBlockRangeFailureUpdate {
                next_limit: 1,
                retry: false,
                warn_single_block,
            };
        }

        if self.stable.is_some() || self.last_working >= failed_span {
            self.last_working = 0;
        }

        self.stable = None;
        self.first_failing = Some(
            self.first_failing
                .map_or(failed_span, |first_failing| first_failing.min(failed_span)),
        );

        if let Some(first_failing) = self.first_failing {
            self.candidate = next_probe(self.last_working, first_failing);
        }

        LogBlockRangeFailureUpdate {
            next_limit: self.current(),
            retry: self.current() < failed_span,
            warn_single_block: false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct LogBlockRangeFailureUpdate {
    pub next_limit: u64,
    pub retry: bool,
    pub warn_single_block: bool,
}

fn sanitize_log_block_range_cap(configured_cap: u64) -> u64 {
    if configured_cap == 0 {
        DEFAULT_AUTO_LOG_BLOCK_RANGE_CAP
    } else {
        configured_cap
    }
}

fn next_probe(last_working: u64, first_failing: u64) -> u64 {
    if last_working + 1 >= first_failing {
        last_working.max(1)
    } else {
        (last_working + ((first_failing - last_working) / 2)).max(1)
    }
}

fn message_indicates_log_range_limit(message: &str) -> bool {
    let message = message.to_ascii_lowercase();
    let mentions_limit = message.contains("limit")
        || message.contains("exceed")
        || message.contains("too many")
        || message.contains("too large")
        || message.contains("too wide")
        || message.contains("more than")
        || message.contains("maximum")
        || message.contains("response size");
    let mentions_logs_or_blocks = message.contains("log") || message.contains("block") || message.contains("result");
    let mentions_range = message.contains("range") || message.contains("block") || message.contains("eth_getlogs");

    mentions_limit && mentions_logs_or_blocks && mentions_range
}

pub(crate) fn is_log_block_range_limit_error(error: &RpcError) -> bool {
    let RpcError::AlloyRpcError(error) = error else {
        return false;
    };

    if let Some(error_payload) = error.as_error_resp() {
        let known_range_limit_code = matches!(error_payload.code, -32005 | -32016 | 429);

        return message_indicates_log_range_limit(&error_payload.message)
            || (known_range_limit_code && message_indicates_log_range_limit(&error_payload.to_string()));
    }

    message_indicates_log_range_limit(&error.to_string())
}

/// Implementation of `HoprRpcOperations` and `HoprIndexerRpcOperations` trait via `alloy`
#[derive(Debug, Clone)]
pub struct RpcOperations<R: HttpRequestor + 'static + Clone> {
    pub provider: Arc<HoprProvider<R>>,
    pub(crate) cfg: RpcOperationsConfig,
    contract_instances: Arc<ContractInstances<HoprProvider<R>>>,
    log_block_range_limit: Arc<Mutex<LogBlockRangeLimit>>,
}

#[cfg_attr(test, mockall::automock)]
impl<R: HttpRequestor + 'static + Clone> RpcOperations<R> {
    pub fn new(
        rpc_client: RpcClient,
        requestor: R,
        cfg: RpcOperationsConfig,
        use_dummy_nr: Option<bool>,
    ) -> Result<Self> {
        let provider = ProviderBuilder::new()
            .disable_recommended_fillers()
            .filler(ChainIdFiller::default())
            .filler(NonceFiller::new(CachedNonceManager::default()))
            .filler(GasFiller::default())
            .filler(GasOracleFiller::new(
                requestor.clone(),
                cfg.gas_oracle_url.clone(),
                cfg.gas_oracle_fallback_max_fee,
                cfg.gas_oracle_fallback_priority_fee,
            ))
            .filler(BlobGasFiller::default())
            .connect_client(rpc_client);

        if cfg.tx_polling_interval.is_zero() {
            return Err(RpcError::Other("tx_polling_interval must be non-zero".to_string()));
        }
        provider.client().set_poll_interval(cfg.tx_polling_interval);

        debug!(contract_addresses = ?cfg.contract_addrs, "configured contract addresses");

        Ok(Self {
            contract_instances: Arc::new(ContractInstances::new(
                &cfg.contract_addrs,
                provider.clone(),
                use_dummy_nr.unwrap_or(cfg!(test)),
            )),
            log_block_range_limit: Arc::new(Mutex::new(LogBlockRangeLimit::new(cfg.max_block_range_fetch_size))),
            cfg,
            provider: Arc::new(provider),
        })
    }

    /// Get the current block number from the RPC endpoint, adjusted for finality
    ///
    /// This method returns the block number minus the configured finality window
    /// to ensure operations only consider confirmed (non-reorgable) blocks.
    ///
    /// # Finality Adjustment
    ///
    /// The returned block number is calculated as:
    /// ```text
    /// confirmed_block = provider.get_block_number() - finality
    /// ```
    ///
    /// For example, with `finality=3`:
    /// - If RPC reports block 1000, this returns 997
    /// - Ensures all operations work with blocks that have 3 confirmations
    ///
    /// # Impact on Readiness Checks
    ///
    /// The API readiness check uses this method, meaning `max_indexer_lag`
    /// is compared against confirmed blocks, not the latest RPC chain head.
    /// This prevents premature "ready" state when the indexer is caught up
    /// to unconfirmed blocks that might be reorganized.
    pub async fn get_block_number(&self) -> Result<u64> {
        Ok(self
            .provider
            .get_block_number()
            .await?
            .saturating_sub(self.cfg.finality as u64))
    }

    pub(crate) async fn get_block(&self, block_number: u64) -> Result<Option<Block>> {
        let sanitized_block_number = block_number.saturating_sub(self.cfg.finality as u64);
        let result = self.provider.get_block_by_number(sanitized_block_number.into()).await?;
        Ok(result)
    }

    pub(crate) async fn get_xdai_balance(&self, address: Address) -> Result<XDaiBalance> {
        Ok(XDaiBalance::from_be_bytes(
            self.provider
                .get_balance(AlloyAddress::from_hopr_address(address))
                .await?
                .to_be_bytes::<32>(),
        ))
    }

    pub(crate) async fn get_hopr_balance(&self, address: Address) -> Result<HoprBalance> {
        Ok(HoprBalance::from_be_bytes(
            self.contract_instances
                .token
                .balanceOf(AlloyAddress::from_hopr_address(address))
                .call()
                .await?
                .to_be_bytes::<32>(),
        ))
    }

    pub(crate) async fn get_hopr_allowance(&self, owner: Address, spender: Address) -> Result<HoprBalance> {
        Ok(HoprBalance::from_be_bytes(
            self.contract_instances
                .token
                .allowance(
                    AlloyAddress::from_hopr_address(owner),
                    AlloyAddress::from_hopr_address(spender),
                )
                .call()
                .await?
                .to_be_bytes::<32>(),
        ))
    }

    pub(crate) async fn get_transaction_count(&self, address: Address) -> Result<u64> {
        let address_alloy = AlloyAddress::from_hopr_address(address);

        // Get provider from any contract instance (they all share the same provider)
        let provider = self.contract_instances.token.provider();

        // First, check if the address has code (is a contract)
        let code = provider.get_code_at(address_alloy).await?;

        if code.is_empty() {
            // Empty code means EOA (Externally Owned Account), use eth_getTransactionCount
            debug!(%address, "address has no code, using eth_getTransactionCount");
            let tx_count = provider.get_transaction_count(address_alloy).await?;
            return Ok(tx_count);
        }

        // Address has code, verify it's a Safe contract by calling getThreshold()
        // This prevents false positives from non-Safe contracts that have a nonce() method
        debug!(%address, "address has code, verifying Safe contract");
        let safe_contract = SafeSingleton::new(address_alloy, provider.clone());

        match safe_contract.getThreshold().call().await {
            Ok(_threshold) => {
                // Successfully called getThreshold(), this is a Safe contract
                // Now get the Safe nonce (transaction count)
                debug!(%address, "address is a Safe contract, getting nonce");
                match safe_contract.nonce().call().await {
                    Ok(nonce) => nonce.try_into().map_err(|_| RpcError::SafeNonceOverflow(nonce)),
                    Err(e) => {
                        // This should rarely happen if getThreshold() succeeded
                        tracing::error!(%address, error = %e, "safe getThreshold succeeded but nonce failed");
                        Err(e.into())
                    }
                }
            }
            Err(e) => {
                // getThreshold() failed - discriminate errors
                let error_str = format!("{:?}", e);
                let error_lower = error_str.to_lowercase();

                // Check if this is a "safe to fallback" error (contract doesn't implement Safe interface)
                let should_fallback = error_lower.contains("execution reverted")
                    || error_lower.contains("function selector not found")
                    || error_lower.contains("abi")
                    || error_lower.contains("decode");

                if should_fallback {
                    // Contract exists but doesn't implement Safe interface - fallback to eth_getTransactionCount
                    debug!(
                        "Address {} has code but getThreshold() failed (not a Safe contract), falling back to \
                         eth_getTransactionCount. Error: {:?}",
                        address, e
                    );
                    let tx_count = provider.get_transaction_count(address_alloy).await?;
                    Ok(tx_count)
                } else {
                    // Real error (RPC failure, network issue, etc.) - propagate it
                    tracing::error!("Failed to verify Safe contract at address {}: {}", address, e);
                    Err(e.into())
                }
            }
        }
    }

    pub(crate) async fn get_channel_closure_notice_period(&self) -> Result<Duration> {
        // TODO: should we cache this value internally ?
        match self
            .contract_instances
            .channels
            .NOTICE_PERIOD_CHANNEL_CLOSURE()
            .call()
            .await
        {
            Ok(returned_result) => Ok(Duration::from_secs(returned_result.into())),
            Err(e) => Err(e.into()),
        }
    }

    pub(crate) async fn calculate_module_address(
        &self,
        owner: Address,
        nonce: u64,
        safe_address: Address,
    ) -> Result<Address> {
        // Construct defaultTarget as concatenated bytes32:
        // hopr_channels_address (20 bytes) + DEFAULT_CAPABILITY_PERMISSIONS (12 bytes)
        let channels_addr_bytes = self.cfg.contract_addrs.channels.as_ref();
        let capability_permissions: [u8; 12] = [0x01, 0x01, 0x03, 0x03, 0x03, 0x03, 0x03, 0x03, 0x03, 0x03, 0x03, 0x03];

        let mut default_target = [0u8; 32];
        default_target[0..20].copy_from_slice(channels_addr_bytes);
        default_target[20..32].copy_from_slice(&capability_permissions);

        // Call predictModuleAddress_1 on stake_factory contract
        let predicted_address_fixed = self
            .contract_instances
            .node_stake_factory
            .predictModuleAddress_1(
                AlloyAddress::from_hopr_address(owner),
                U256::from(nonce), // Convert u64 to U256
                AlloyAddress::from_hopr_address(safe_address),
                FixedBytes::from(default_target), // Convert [u8; 32] to B256
            )
            .call()
            .await?
            .0; // Extract the FixedBytes<20> from the return value

        // Convert FixedBytes<20> to AlloyAddress and then to hopr Address
        let predicted_address = AlloyAddress::from(predicted_address_fixed);
        Ok(predicted_address.to_hopr_address())
    }
}

impl<R: HttpRequestor + 'static + Clone> RpcOperations<R> {
    pub(crate) fn log_block_range_limit(&self) -> u64 {
        self.with_log_block_range_limit(|range_limit| range_limit.current())
    }

    pub(crate) fn record_log_block_range_success(&self, requested_span: u64, attempted_limit: u64) {
        let stable =
            self.with_log_block_range_limit(|range_limit| range_limit.record_success(requested_span, attempted_limit));

        if let Some(stable) = stable {
            debug!(
                stable_block_range = stable,
                configured_cap = sanitize_log_block_range_cap(self.cfg.max_block_range_fetch_size),
                "using stable RPC log block range limit"
            );
        }
    }

    pub(crate) fn record_log_block_range_failure(
        &self,
        requested_span: u64,
        error: &RpcError,
    ) -> LogBlockRangeFailureUpdate {
        let update = self.with_log_block_range_limit(|range_limit| range_limit.record_failure(requested_span));

        if update.warn_single_block {
            warn!(
                error = %error,
                "RPC rejected a single-block eth_getLogs request; continuing in single-block mode, but this mode of \
                 operation is very slow"
            );
        } else {
            warn!(
                failed_block_range = requested_span,
                next_block_range = update.next_limit,
                error = %error,
                "RPC rejected eth_getLogs block range; adapting block range limit"
            );
        }

        update
    }

    fn with_log_block_range_limit<T, F>(&self, action: F) -> T
    where
        F: FnOnce(&mut LogBlockRangeLimit) -> T,
    {
        match self.log_block_range_limit.lock() {
            Ok(mut guard) => action(&mut guard),
            Err(poisoned) => {
                let mut guard = poisoned.into_inner();
                action(&mut guard)
            }
        }
    }
}

#[async_trait]
impl<R: HttpRequestor + 'static + Clone> HoprRpcOperations for RpcOperations<R> {
    async fn get_timestamp(&self, block_number: u64) -> Result<Option<u64>> {
        Ok(self.get_block(block_number).await?.map(|b| b.header.timestamp))
    }

    async fn calculate_module_address(&self, owner: Address, nonce: u64, safe_address: Address) -> Result<Address> {
        self.calculate_module_address(owner, nonce, safe_address).await
    }

    async fn get_minimum_network_winning_probability(&self) -> Result<WinningProbability> {
        match self
            .contract_instances
            .winning_probability_oracle
            .currentWinProb()
            .call()
            .await
        {
            Ok(encoded_win_prob) => {
                let mut encoded: EncodedWinProb = Default::default();
                encoded.copy_from_slice(&encoded_win_prob.to_be_bytes_vec());
                Ok(encoded.into())
            }
            Err(e) => Err(e.into()),
        }
    }

    async fn get_minimum_network_ticket_price(&self) -> Result<HoprBalance> {
        Ok(HoprBalance::from_be_bytes(
            self.contract_instances
                .ticket_price_oracle
                .currentTicketPrice()
                .call()
                .await?
                .to_be_bytes::<32>(),
        ))
    }

    async fn get_safe_from_node_safe_registry(&self, node_address: Address) -> Result<Address> {
        match self
            .contract_instances
            .node_safe_registry
            .nodeToSafe(AlloyAddress::from_hopr_address(node_address))
            .call()
            .await
        {
            Ok(returned_result) => Ok(returned_result.to_hopr_address()),
            Err(e) => Err(e.into()),
        }
    }

    async fn get_gas_price(&self) -> Result<u128> {
        Ok(self.provider.get_gas_price().await?)
    }

    async fn estimate_eip1559_fees(&self) -> Result<Eip1559FeeEstimation> {
        let estimate = self.provider.estimate_eip1559_fees().await?;
        Ok(Eip1559FeeEstimation {
            max_fee_per_gas: estimate.max_fee_per_gas,
            max_priority_fee_per_gas: estimate.max_priority_fee_per_gas,
        })
    }

    //     // Check on-chain status of, node, safe, and module
    //     async fn check_node_safe_module_status(&self, node_address: Address) -> Result<NodeSafeModuleStatus> {
    //         // 1) check if the node is already included into the module
    //         let tx_1 = CallItemBuilder::new(self.node_module.isNode(node_address.into())).allow_failure(false);
    //         // 2) if the module is enabled in the safe
    //         let tx_2 =
    //
    // CallItemBuilder::new(self.node_safe.isModuleEnabled(self.cfg.module_address.into())).allow_failure(false);
    //         // 3) if the safe is the owner of the module
    //         let tx_3 = CallItemBuilder::new(self.node_module.owner()).allow_failure(false);
    //         let multicall = self.provider.multicall().add_call(tx_1).add_call(tx_2).add_call(tx_3);
    //
    //         let (node_in_module_inclusion, module_safe_enabling, safe_of_module_ownership) =
    //             multicall.aggregate3_value().await.map_err(RpcError::MulticallError)?;
    //         let is_node_included_in_module =
    //             node_in_module_inclusion.map_err(|e| RpcError::MulticallFailure(e.idx, e.return_data.to_string()))?;
    //         let is_module_enabled_in_safe =
    //             module_safe_enabling.map_err(|e| RpcError::MulticallFailure(e.idx, e.return_data.to_string()))?;
    //         let is_safe_owner_of_module = self.cfg.safe_address.eq(&safe_of_module_ownership
    //             .map_err(|e| RpcError::MulticallFailure(e.idx, e.return_data.to_string()))?
    //             .0
    //             .0
    //             .into());
    //
    //         Ok(NodeSafeModuleStatus {
    //             is_node_included_in_module,
    //             is_module_enabled_in_safe,
    //             is_safe_owner_of_module,
    //         })
    //     }

    async fn send_transaction(&self, tx: TransactionRequest) -> Result<PendingTransaction> {
        let sent_tx = self.provider.send_transaction(tx).await?;

        let pending_tx = sent_tx
            .with_required_confirmations(self.cfg.finality as u64)
            .register()
            .await
            .map_err(RpcError::PendingTransactionError)?;

        Ok(pending_tx)
    }

    async fn send_transaction_with_confirm(&self, tx: TransactionRequest) -> Result<Hash> {
        let sent_tx = self.provider.send_transaction(tx).await?;

        let receipt = sent_tx
            .with_required_confirmations(self.cfg.finality as u64)
            .get_receipt()
            .await
            .map_err(RpcError::PendingTransactionError)?;

        let tx_hash = Hash::from(receipt.transaction_hash.0);

        // Check the transaction status. `status()` returns `true` for successful transactions
        // and `false` for failed or reverted transactions.
        if receipt.status() {
            Ok(tx_hash)
        } else {
            // Transaction failed, raise an error
            Err(RpcError::TransactionFailed(tx_hash))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{DEFAULT_AUTO_LOG_BLOCK_RANGE_CAP, LogBlockRangeLimit};

    fn converge_limit(cap: u64, allowed_limit: u64) -> LogBlockRangeLimit {
        let mut range_limit = LogBlockRangeLimit::new(cap);

        for _ in 0..64 {
            let candidate = range_limit.current();

            if candidate <= allowed_limit {
                range_limit.record_success(candidate, candidate);
            } else {
                range_limit.record_failure(candidate);
            }

            if range_limit.stable == Some(allowed_limit) {
                return range_limit;
            }
        }

        panic!("range limit did not converge to {allowed_limit}");
    }

    #[test]
    fn test_log_block_range_limit_uses_default_cap_for_zero_config() {
        let range_limit = LogBlockRangeLimit::new(0);

        assert_eq!(range_limit.current(), DEFAULT_AUTO_LOG_BLOCK_RANGE_CAP);
    }

    #[test]
    fn test_log_block_range_limit_converges_to_allowed_limits() {
        for allowed_limit in [10_000, 5_000, 1_024, 100, 1] {
            let range_limit = converge_limit(10_000, allowed_limit);

            assert_eq!(range_limit.current(), allowed_limit);
            assert_eq!(range_limit.stable, Some(allowed_limit));
        }
    }

    #[test]
    fn test_log_block_range_limit_downshifts_after_stable_failure() {
        let mut range_limit = converge_limit(10_000, 100);

        assert_eq!(range_limit.current(), 100);

        let update = range_limit.record_failure(100);
        assert!(update.retry);
        assert!(update.next_limit < 100);

        for _ in 0..64 {
            let candidate = range_limit.current();

            if candidate <= 50 {
                range_limit.record_success(candidate, candidate);
            } else {
                range_limit.record_failure(candidate);
            }

            if range_limit.stable == Some(50) {
                break;
            }
        }

        assert_eq!(range_limit.current(), 50);
        assert_eq!(range_limit.stable, Some(50));
    }

    #[test]
    fn test_log_block_range_limit_single_block_failure_keeps_single_block_mode() {
        let mut range_limit = LogBlockRangeLimit::new(10_000);
        let update = range_limit.record_failure(1);

        assert!(!update.retry);
        assert!(update.warn_single_block);
        assert_eq!(range_limit.current(), 1);
    }

    // NOTE: This test is commented out because check_node_safe_module_status() method is commented out
    // and the helper functions deploy_one_safe_one_module_and_setup_for_testing and include_node_to_module_by_safe
    // are no longer available in blokli_chain_types
    //
    // #[tokio::test]
    // async fn test_check_node_safe_module_status() -> anyhow::Result<()> {
    //         let _ = env_logger::builder().is_test(true).try_init();
    //
    //         let expected_block_time = Duration::from_secs(1);
    //         let anvil = blokli_chain_types::utils::create_anvil(Some(expected_block_time));
    //         let chain_key_0 = ChainKeypair::from_secret(anvil.keys()[0].to_bytes().as_ref())?;
    //
    //         // Deploy contracts
    //         let (contract_instances, module, safe) = {
    //             let client = create_rpc_client_to_anvil(&anvil, &chain_key_0);
    //             let instances = ContractInstances::deploy_for_testing(client.clone(), &chain_key_0).await?;
    //
    //             // deploy MULTICALL contract to anvil
    //             deploy_multicall3_to_anvil(&client.clone()).await?;
    //
    //             let (module, safe) = blokli_chain_types::utils::deploy_one_safe_one_module_and_setup_for_testing::<
    //                 Arc<AnvilRpcClient>,
    //             >(&instances, client.clone(), &chain_key_0)
    //             .await?;
    //
    //             // deploy a module and safe instance and add node into the module. The module is enabled by default
    // in the             // safe
    //             (instances, module, safe)
    //         };
    //
    //         let cfg = RpcOperationsConfig {
    //             chain_id: anvil.chain_id(),
    //             tx_polling_interval: Duration::from_millis(10),
    //             expected_block_time,
    //             finality: 2,
    //             contract_addrs: ContractAddresses::from(&contract_instances),
    //             gas_oracle_url: None,
    //             ..RpcOperationsConfig::default()
    //         };
    //
    //         let transport_client = ReqwestTransport::new(anvil.endpoint_url());
    //
    //         let rpc_client = ClientBuilder::default()
    //             .layer(RetryBackoffLayer::new(2, 100, 100))
    //             .transport(transport_client.clone(), transport_client.guess_local());
    //
    //         // Wait until contracts deployments are final
    //         sleep((1 + cfg.finality) * expected_block_time).await;
    //
    //         let rpc = RpcOperations::new(rpc_client, transport_client.client().clone(), cfg, None)?;
    //
    //         let result_before_including_node = rpc.check_node_safe_module_status((&chain_key_0).into()).await?;
    //         // before including node to the safe and module, only the first chck is false, the others are true
    //         assert!(
    //             !result_before_including_node.is_node_included_in_module,
    //             "node should not be included in a default module"
    //         );
    //         assert!(
    //             result_before_including_node.is_module_enabled_in_safe,
    //             "module should be enabled in a default safe"
    //         );
    //         assert!(
    //             result_before_including_node.is_safe_owner_of_module,
    //             "safe should not be the owner of a default module"
    //         );
    //
    //         // including node to the module
    //         blokli_chain_types::utils::include_node_to_module_by_safe(
    //             contract_instances.channels.provider().clone(),
    //             safe,
    //             module,
    //             (&chain_key_0).into(),
    //             &chain_key_0,
    //         )
    //         .await?;
    //
    //         let result_with_node_included = rpc.check_node_safe_module_status((&chain_key_0).into()).await?;
    //         // after the node gets included into the module, all checks should be true
    //         assert!(
    //             result_with_node_included.is_node_included_in_module,
    //             "node should be included in a default module"
    //         );
    //         assert!(
    //             result_with_node_included.is_module_enabled_in_safe,
    //             "module should be enabled in a default safe"
    //         );
    //         assert!(
    //             result_with_node_included.is_safe_owner_of_module,
    //             "safe should be the owner of a default module"
    //         );
    //
    //         Ok(())
    //     }
}
