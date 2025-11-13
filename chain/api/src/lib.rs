//! Crate containing the API object for chain operations used by the HOPRd node.

pub mod errors;
pub mod rpc_adapter;
pub mod transaction_executor;
pub mod transaction_monitor;
pub mod transaction_store;
pub mod transaction_validator;

use std::{collections::HashMap, sync::Arc, time::Duration};

use alloy::{
    rpc::client::ClientBuilder,
    transports::{
        http::{Http, ReqwestTransport},
        layers::RetryBackoffLayer,
    },
};
use blokli_chain_indexer::{IndexerConfig, IndexerState, block::Indexer, handlers::ContractEventHandlers};
use blokli_chain_rpc::{
    HoprRpcOperations,
    client::DefaultRetryPolicy,
    rpc::{RpcOperations, RpcOperationsConfig},
    transport::ReqwestClient,
};
use blokli_chain_types::ContractAddresses;
use blokli_db::BlokliDbAllOperations;
use futures::future::AbortHandle;
use hopr_async_runtime::spawn_as_abortable;
use hopr_chain_config::ChainNetworkConfig;
pub use hopr_internal_types::channels::ChannelEntry;
use hopr_internal_types::{
    account::AccountEntry, // channels::CorruptedChannelEntry,
    prelude::ChannelDirection,
    tickets::WinningProbability,
};
use hopr_primitive_types::{
    prelude::{Address, Balance, Currency, HoprBalance, U256, WxHOPR, XDai},
    traits::IntoEndian,
};

use crate::{
    errors::{BlokliChainError, Result},
    rpc_adapter::RpcAdapter,
    transaction_executor::{RawTransactionExecutor, RawTransactionExecutorConfig},
    transaction_monitor::{TransactionMonitor, TransactionMonitorConfig},
    transaction_store::TransactionStore,
    transaction_validator::TransactionValidator,
};

pub type DefaultHttpRequestor = blokli_chain_rpc::transport::ReqwestClient;

fn build_transport_client(url: &str) -> Result<Http<ReqwestClient>> {
    let parsed_url = url::Url::parse(url).unwrap_or_else(|_| panic!("failed to parse URL: {url}"));
    Ok(ReqwestTransport::new(parsed_url))
}

#[derive(Debug, Copy, Clone, Hash, PartialEq, Eq)]
pub enum BlokliChainProcess {
    Indexer,
    TransactionMonitor,
}

/// Represents all chain interactions exported to be used in the hopr-lib
///
/// NOTE: instead of creating a unified interface the [BlokliChain] exports
/// some functionality (e.g. the [ChainActions] as a referentially used)
/// object. This behavior will be refactored and hidden behind a trait
/// in the future implementations.
#[derive(Debug, Clone)]
pub struct BlokliChain<T: BlokliDbAllOperations + Send + Sync + Clone + std::fmt::Debug> {
    contract_addresses: ContractAddresses,
    indexer_cfg: IndexerConfig,
    indexer_state: IndexerState,
    db: T,
    rpc_operations: RpcOperations<DefaultHttpRequestor>,
    transaction_executor: Arc<RawTransactionExecutor<RpcAdapter<DefaultHttpRequestor>>>,
    transaction_store: Arc<TransactionStore>,
    transaction_monitor: Arc<TransactionMonitor<RpcAdapter<DefaultHttpRequestor>>>,
}

impl<T: BlokliDbAllOperations + Send + Sync + Clone + std::fmt::Debug + 'static> BlokliChain<T> {
    pub fn new(
        db: T,
        chain_config: ChainNetworkConfig,
        contract_addresses: ContractAddresses,
        indexer_cfg: IndexerConfig,
        rpc_url: String,
    ) -> Result<Self> {
        // TODO: extract this from the global config type
        let mut rpc_http_config = blokli_chain_rpc::HttpPostRequestorConfig::default();
        if let Some(max_rpc_req) = chain_config.max_requests_per_sec {
            rpc_http_config.max_requests_per_sec = Some(max_rpc_req); // override the default if set
        }

        // TODO(#7140): replace this DefaultRetryPolicy with a custom one that computes backoff with the number of
        // retries
        let rpc_http_retry_policy = DefaultRetryPolicy::default();

        // TODO: extract this from the global config type
        let rpc_cfg = RpcOperationsConfig {
            chain_id: chain_config.chain.chain_id as u64,
            contract_addrs: contract_addresses,
            expected_block_time: Duration::from_millis(chain_config.chain.block_time),
            tx_polling_interval: Duration::from_millis(chain_config.tx_polling_interval),
            finality: chain_config.confirmations,
            max_block_range_fetch_size: chain_config.max_block_range,
            ..Default::default()
        };

        // --- Configs done ---

        let transport_client = build_transport_client(&rpc_url)?;

        let rpc_client = ClientBuilder::default()
            .layer(RetryBackoffLayer::new_with_policy(2, 100, 100, rpc_http_retry_policy))
            .transport(transport_client.clone(), transport_client.guess_local());

        let requestor = DefaultHttpRequestor::new();

        // Build RPC operations
        let rpc_operations = RpcOperations::new(rpc_client, requestor, rpc_cfg, None)?;

        // Create IndexerState for coordinating block processing with subscriptions
        let indexer_state = IndexerState::new(indexer_cfg.event_bus_capacity, indexer_cfg.shutdown_signal_capacity);

        // Build transaction submission infrastructure
        let transaction_store = Arc::new(TransactionStore::new());
        let transaction_validator = Arc::new(TransactionValidator::new());
        let rpc_adapter = Arc::new(RpcAdapter::new(rpc_operations.clone()));

        let transaction_executor = Arc::new(RawTransactionExecutor::with_shared_dependencies(
            rpc_adapter.clone(),
            transaction_store.clone(),
            transaction_validator,
            RawTransactionExecutorConfig::default(),
        ));

        let transaction_monitor = Arc::new(TransactionMonitor::new(
            transaction_store.clone(),
            (*rpc_adapter).clone(),
            TransactionMonitorConfig::default(),
        ));

        Ok(Self {
            contract_addresses,
            indexer_cfg,
            indexer_state,
            db,
            rpc_operations,
            transaction_executor,
            transaction_store,
            transaction_monitor,
        })
    }

    /// Execute all processes of the [`BlokliChain`] object.
    ///
    /// This method will spawn the [`BlokliChainProcess::Indexer`] and
    /// [`BlokliChainProcess::TransactionMonitor`] processes and return join handles to the calling
    /// function.
    pub async fn start(&self) -> errors::Result<HashMap<BlokliChainProcess, AbortHandle>> {
        let mut processes: HashMap<BlokliChainProcess, AbortHandle> = HashMap::new();

        processes.insert(
            BlokliChainProcess::TransactionMonitor,
            spawn_as_abortable!(self.transaction_monitor.clone().start()),
        );
        processes.insert(
            BlokliChainProcess::Indexer,
            Indexer::new(
                self.rpc_operations.clone(),
                ContractEventHandlers::new(
                    self.contract_addresses,
                    self.db.clone(),
                    self.rpc_operations.clone(),
                    self.indexer_state.clone(),
                ),
                self.db.clone(),
                self.indexer_cfg.clone(),
                self.indexer_state.clone(),
            )
            .start()
            .await?,
        );
        Ok(processes)
    }

    pub fn indexer_state(&self) -> IndexerState {
        self.indexer_state.clone()
    }

    pub fn transaction_executor(&self) -> Arc<RawTransactionExecutor<RpcAdapter<DefaultHttpRequestor>>> {
        self.transaction_executor.clone()
    }

    pub fn transaction_store(&self) -> Arc<TransactionStore> {
        self.transaction_store.clone()
    }

    pub async fn accounts_announced_on_chain(&self) -> errors::Result<Vec<AccountEntry>> {
        Ok(self.db.get_accounts(None, true).await?)
    }

    pub async fn channel(&self, src: &Address, dest: &Address) -> errors::Result<ChannelEntry> {
        self.db
            .get_channel_by_parties(None, src, dest)
            .await
            .map_err(BlokliChainError::from)
            .and_then(|v| {
                v.ok_or(errors::BlokliChainError::Api(format!(
                    "Channel entry not available {src}-{dest}"
                )))
            })
    }

    pub async fn channels_from(&self, src: &Address) -> errors::Result<Vec<ChannelEntry>> {
        Ok(self.db.get_channels_via(None, ChannelDirection::Outgoing, src).await?)
    }

    pub async fn channels_to(&self, dest: &Address) -> errors::Result<Vec<ChannelEntry>> {
        Ok(self.db.get_channels_via(None, ChannelDirection::Incoming, dest).await?)
    }

    pub async fn all_channels(&self) -> errors::Result<Vec<ChannelEntry>> {
        Ok(self.db.get_all_channels(None).await?)
    }

    // TODO: Refactor to use channel.corrupted_state field
    // pub async fn corrupted_channels(&self) -> errors::Result<Vec<CorruptedChannelEntry>> {
    //     Ok(self.db.get_all_corrupted_channels(None).await?)
    // }

    pub async fn ticket_price(&self) -> errors::Result<Option<HoprBalance>> {
        Ok(self.db.get_indexer_data(None).await?.ticket_price)
    }

    pub async fn safe_allowance(&self) -> errors::Result<HoprBalance> {
        Ok(self.db.get_safe_hopr_allowance(None).await?)
    }

    pub fn rpc(&self) -> &RpcOperations<DefaultHttpRequestor> {
        &self.rpc_operations
    }

    /// Retrieves the balance of the specified address for the given currency.
    ///
    /// This method queries the on-chain balance of the provided address for the specified currency.
    /// It supports querying balances for XDai and WxHOPR currencies. If the currency is unsupported,
    /// an error is returned.
    ///
    /// # Arguments
    /// * `address` - The address whose balance is to be retrieved.
    ///
    /// # Returns
    /// * `Result<Balance<C>>` - The balance of the specified address for the given currency, or an error if the query
    ///   fails.
    pub async fn get_safe_balance<C: Currency + Send>(&self, safe_address: Address) -> errors::Result<Balance<C>> {
        let bal = if C::is::<XDai>() {
            self.rpc_operations.get_xdai_balance(safe_address).await?.to_be_bytes()
        } else if C::is::<WxHOPR>() {
            self.rpc_operations.get_hopr_balance(safe_address).await?.to_be_bytes()
        } else {
            return Err(BlokliChainError::Api("unsupported currency".into()));
        };

        Ok(Balance::<C>::from(U256::from_be_bytes(bal)))
    }

    /// Retrieves the HOPR token allowance granted by the safe address to the channels contract.
    ///
    /// This method queries the on-chain HOPR token contract to determine how many tokens
    /// the safe address has approved the channels contract to spend on its behalf.
    ///
    /// # Returns
    /// * `Result<HoprBalance>` - The current allowance amount, or an error if the query fails
    pub async fn get_safe_hopr_allowance(&self, safe_address: Address) -> Result<HoprBalance> {
        Ok(self
            .rpc_operations
            .get_hopr_allowance(safe_address, self.contract_addresses.channels)
            .await?)
    }

    pub async fn get_channel_closure_notice_period(&self) -> errors::Result<Duration> {
        Ok(self.rpc_operations.get_channel_closure_notice_period().await?)
    }

    pub async fn get_minimum_winning_probability(&self) -> errors::Result<WinningProbability> {
        Ok(self.rpc_operations.get_minimum_network_winning_probability().await?)
    }

    pub async fn get_minimum_ticket_price(&self) -> errors::Result<HoprBalance> {
        Ok(self.rpc_operations.get_minimum_network_ticket_price().await?)
    }
}
