//! Crate containing the API object for chain operations used by the HOPRd node.

pub mod errors;
pub mod executors;

use std::{collections::HashMap, sync::Arc, time::Duration};

use alloy::{
    rpc::{client::ClientBuilder, types::TransactionRequest},
    transports::{
        http::{Http, ReqwestTransport},
        layers::RetryBackoffLayer,
    },
};
use blokli_chain_actions::{
    ChainActions,
    action_queue::{ActionQueue, ActionQueueConfig},
    action_state::IndexerActionTracker,
};
use blokli_chain_indexer::{IndexerConfig, IndexerState, block::Indexer, handlers::ContractEventHandlers};
use blokli_chain_rpc::{
    HoprRpcOperations,
    client::DefaultRetryPolicy,
    rpc::{RpcOperations, RpcOperationsConfig},
    transport::ReqwestClient,
};
use blokli_chain_types::ContractAddresses;
pub use blokli_chain_types::chain_events::SignificantChainEvent;
use blokli_db::BlokliDbAllOperations;
use executors::{EthereumTransactionExecutor, RpcEthereumClient, RpcEthereumClientConfig};
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

use crate::errors::{BlokliChainError, Result};

pub type DefaultHttpRequestor = blokli_chain_rpc::transport::ReqwestClient;

fn build_transport_client(url: &str) -> Result<Http<ReqwestClient>> {
    let parsed_url = url::Url::parse(url).unwrap_or_else(|_| panic!("failed to parse URL: {url}"));
    Ok(ReqwestTransport::new(parsed_url))
}

#[derive(Debug, Copy, Clone, Hash, PartialEq, Eq)]
pub enum BlokliChainProcess {
    Indexer,
    OutgoingOnchainActionQueue,
}

type ActionQueueType<T> = ActionQueue<
    T,
    IndexerActionTracker,
    EthereumTransactionExecutor<TransactionRequest, RpcEthereumClient<RpcOperations<DefaultHttpRequestor>>>,
>;

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
    indexer_events_tx: async_channel::Sender<SignificantChainEvent>,
    indexer_state: IndexerState,
    db: T,
    blokli_chain_actions: ChainActions<T>,
    action_queue: ActionQueueType<T>,
    action_state: Arc<IndexerActionTracker>,
    rpc_operations: RpcOperations<DefaultHttpRequestor>,
}

impl<T: BlokliDbAllOperations + Send + Sync + Clone + std::fmt::Debug + 'static> BlokliChain<T> {
    #[allow(clippy::too_many_arguments)] // TODO: refactor this function into a reasonable group of components once fully rearchitected
    pub fn new(
        db: T,
        chain_config: ChainNetworkConfig,
        contract_addresses: ContractAddresses,
        indexer_cfg: IndexerConfig,
        indexer_events_tx: async_channel::Sender<SignificantChainEvent>,
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

        // TODO: extract this from the global config type
        let rpc_client_cfg = RpcEthereumClientConfig::default();

        // TODO: extract this from the global config type
        let action_queue_cfg = ActionQueueConfig::default();

        // --- Configs done ---

        let transport_client = build_transport_client(&rpc_url)?;

        let rpc_client = ClientBuilder::default()
            .layer(RetryBackoffLayer::new_with_policy(2, 100, 100, rpc_http_retry_policy))
            .transport(transport_client.clone(), transport_client.guess_local());

        let requestor = DefaultHttpRequestor::new();

        // Build RPC operations
        let rpc_operations =
            RpcOperations::new(rpc_client, requestor, rpc_cfg, None).expect("failed to initialize RPC");

        // Build the Ethereum Transaction Executor that uses RpcOperations as backend
        // let ethereum_tx_executor = EthereumTransactionExecutor::new(
        //     RpcEthereumClient::new(rpc_operations.clone(), rpc_client_cfg),
        //     SafePayloadGenerator::new(contract_addresses),
        // );
        let ethereum_tx_executor =
            EthereumTransactionExecutor::new(RpcEthereumClient::new(rpc_operations.clone(), rpc_client_cfg));

        // Build the Action Queue
        let action_queue = ActionQueue::new(
            db.clone(),
            IndexerActionTracker::default(),
            ethereum_tx_executor,
            action_queue_cfg,
        );

        let action_state = action_queue.action_state();
        let action_sender = action_queue.new_sender();

        // Instantiate Chain Actions
        let blokli_chain_actions = ChainActions::new(db.clone(), action_sender);

        // Create IndexerState for coordinating block processing with subscriptions
        let indexer_state = IndexerState::new(indexer_cfg.event_bus_capacity, indexer_cfg.shutdown_signal_capacity);

        Ok(Self {
            contract_addresses,
            indexer_cfg,
            indexer_events_tx,
            indexer_state,
            db,
            blokli_chain_actions,
            action_queue,
            action_state,
            rpc_operations,
        })
    }

    /// Execute all processes of the [`BlokliChain`] object.
    ///
    /// This method will spawn the [`BlokliChainProcess::Indexer`] and
    /// [`BlokliChainProcess::OutgoingOnchainActionQueue`] processes and return join handles to the calling
    /// function.
    pub async fn start(&self) -> errors::Result<HashMap<BlokliChainProcess, AbortHandle>> {
        let mut processes: HashMap<BlokliChainProcess, AbortHandle> = HashMap::new();

        processes.insert(
            BlokliChainProcess::OutgoingOnchainActionQueue,
            spawn_as_abortable!(self.action_queue.clone().start()),
        );
        processes.insert(
            BlokliChainProcess::Indexer,
            Indexer::new(
                self.rpc_operations.clone(),
                ContractEventHandlers::new(self.contract_addresses, self.db.clone(), self.rpc_operations.clone()),
                self.db.clone(),
                self.indexer_cfg.clone(),
                self.indexer_events_tx.clone(),
                self.indexer_state.clone(),
            )
            .start()
            .await?,
        );
        Ok(processes)
    }

    pub fn action_state(&self) -> Arc<IndexerActionTracker> {
        self.action_state.clone()
    }

    pub fn indexer_state(&self) -> IndexerState {
        self.indexer_state.clone()
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

    pub fn actions_ref(&self) -> &ChainActions<T> {
        &self.blokli_chain_actions
    }

    pub fn actions_mut_ref(&mut self) -> &mut ChainActions<T> {
        &mut self.blokli_chain_actions
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
