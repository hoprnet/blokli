use std::{
    fmt::{Debug, Formatter},
    sync::Arc,
};

use alloy::{
    primitives::{Address as AlloyAddress, B256},
    sol_types::SolEventInterface,
};
use async_trait::async_trait;
use blokli_chain_rpc::{HoprIndexerRpcOperations, Log};
use blokli_chain_types::{AlloyAddressExt, ContractAddresses};
use blokli_db::{BlokliDbAllOperations, OpenTransaction};
use hopr_bindings::{
    hopr_announcements::HoprAnnouncements::HoprAnnouncementsEvents, hopr_channels::HoprChannels::HoprChannelsEvents,
    hopr_node_management_module::HoprNodeManagementModule::HoprNodeManagementModuleEvents,
    hopr_node_safe_registry::HoprNodeSafeRegistry::HoprNodeSafeRegistryEvents,
    hopr_ticket_price_oracle::HoprTicketPriceOracle::HoprTicketPriceOracleEvents,
    hopr_token::HoprToken::HoprTokenEvents,
    hopr_winning_probability_oracle::HoprWinningProbabilityOracle::HoprWinningProbabilityOracleEvents,
};
use hopr_crypto_types::prelude::Hash;
use hopr_primitive_types::prelude::{Address, SerializableLog};
use tracing::{debug, error, trace};

use crate::{
    IndexerState,
    errors::{CoreEthereumIndexerError, Result},
};

mod announcements;
mod channel_utils;
mod channels;
mod helpers;
mod node_safe_registry;
mod oracles;
#[cfg(test)]
mod test_utils;
mod tokens;

#[cfg(all(feature = "prometheus", not(test)))]
lazy_static::lazy_static! {
    static ref METRIC_INDEXER_LOG_COUNTERS: hopr_metrics::MultiCounter =
        hopr_metrics::MultiCounter::new(
            "hopr_indexer_contract_log_count",
            "Counts of different HOPR contract logs processed by the Indexer",
            &["contract"]
    ).unwrap();
}

/// Event handling an object for on-chain operations
///
/// Once an on-chain operation is recorded by the [crate::block::Indexer], it is pre-processed
/// and passed on to this object that handles event-specific actions for each on-chain operation.
#[derive(Clone)]
pub struct ContractEventHandlers<T, Db> {
    /// channels, announcements, token: contract addresses
    /// whose event we process
    pub(super) addresses: Arc<ContractAddresses>,
    /// callbacks to inform other modules
    pub(super) db: Db,
    /// rpc operations to interact with the chain
    _rpc_operations: T,
    /// indexer state for publishing events to subscribers
    pub(super) indexer_state: IndexerState,
}

impl<T, Db> Debug for ContractEventHandlers<T, Db> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ContractEventHandler")
            .field("addresses", &self.addresses)
            .finish_non_exhaustive()
    }
}

impl<T, Db> ContractEventHandlers<T, Db>
where
    T: HoprIndexerRpcOperations + Clone + Send + 'static,
    Db: BlokliDbAllOperations + Clone,
{
    /// Creates a new instance of contract event handlers with RPC operations support.
    ///
    /// This constructor initializes the event handlers with all necessary dependencies
    /// for processing blockchain events and making direct RPC calls for fresh state data.
    ///
    /// # Type Parameters
    /// * `T` - Type implementing `HoprIndexerRpcOperations` for blockchain queries
    ///
    /// # Arguments
    /// * `addresses` - Contract addresses configuration
    /// * `db` - Database connection for persistent storage
    /// * `rpc_operations` - RPC interface for direct blockchain queries
    /// * `indexer_state` - Indexer state for publishing events to subscribers
    ///
    /// # Returns
    /// * `Self` - New instance of `ContractEventHandlers`
    pub fn new(addresses: ContractAddresses, db: Db, rpc_operations: T, indexer_state: IndexerState) -> Self {
        Self {
            addresses: Arc::new(addresses),
            db,
            _rpc_operations: rpc_operations,
            indexer_state,
        }
    }

    #[allow(dead_code)]
    async fn on_node_management_module_event(
        &self,
        _tx: &OpenTransaction,
        _event: HoprNodeManagementModuleEvents,
        _is_synced: bool,
    ) -> Result<()> {
        #[cfg(all(feature = "prometheus", not(test)))]
        METRIC_INDEXER_LOG_COUNTERS.increment(&["node_management_module"]);
        // Don't care at the moment
        Ok(())
    }

    #[tracing::instrument(level = "debug", skip(self, slog), fields(log=%slog))]
    async fn process_log_event(&self, tx: &OpenTransaction, slog: SerializableLog, is_synced: bool) -> Result<()> {
        trace!(log = %slog, "log content");

        let log = Log::from(slog.clone());

        let primitive_log = alloy::primitives::Log::new(
            AlloyAddress::from_hopr_address(slog.address),
            slog.topics.iter().map(|h| B256::from_slice(h.as_ref())).collect(),
            slog.data.clone().into(),
        )
        .ok_or_else(|| {
            CoreEthereumIndexerError::ProcessError(format!("failed to convert log to primitive log: {slog:?}"))
        })?;

        if log.address.eq(&self.addresses.announcements) {
            let bn = log.block_number as u32;
            let tx_idx = log.tx_index as u32;
            let log_idx = log.log_index.as_u32();
            let event = HoprAnnouncementsEvents::decode_log(&primitive_log)?;
            self.on_announcement_event(tx, event.data, bn, tx_idx, log_idx, is_synced)
                .await
        } else if log.address.eq(&self.addresses.channels) {
            let event = HoprChannelsEvents::decode_log(&primitive_log)?;
            let block = log.block_number as u32;
            let tx_idx = log.tx_index as u32;
            let log_idx = log.log_index.as_u32();
            match self
                .on_channel_event(tx, event.data, block, tx_idx, log_idx, is_synced)
                .await
            {
                Ok(res) => Ok(res),
                Err(CoreEthereumIndexerError::ChannelDoesNotExist) => {
                    // This is not an error, just a log that we don't have the channel in the DB
                    debug!(
                        ?log,
                        "channel didn't exist in the db. Created a corrupted channel entry and ignored event"
                    );
                    Ok(())
                }
                Err(e) => Err(e),
            }
        } else if log.address.eq(&self.addresses.token) {
            let event = HoprTokenEvents::decode_log(&primitive_log)?;
            self.on_token_event(tx, event.data, is_synced).await
        } else if log.address.eq(&self.addresses.node_safe_registry) {
            let event = HoprNodeSafeRegistryEvents::decode_log(&primitive_log)?;
            self.on_node_safe_registry_event(tx, event.data, is_synced).await
        } else if log.address.eq(&self.addresses.ticket_price_oracle) {
            let event = HoprTicketPriceOracleEvents::decode_log(&primitive_log)?;
            self.on_ticket_price_oracle_event(tx, event.data, is_synced).await
        } else if log.address.eq(&self.addresses.winning_probability_oracle) {
            let event = HoprWinningProbabilityOracleEvents::decode_log(&primitive_log)?;
            self.on_ticket_winning_probability_oracle_event(tx, event.data, is_synced)
                .await
        } else {
            #[cfg(all(feature = "prometheus", not(test)))]
            METRIC_INDEXER_LOG_COUNTERS.increment(&["unknown"]);

            error!(
                address = %log.address, log = ?log,
                "on_event error - unknown contract address, received log"
            );
            return Err(CoreEthereumIndexerError::UnknownContract(log.address));
        }
    }
}

#[async_trait]
impl<T, Db> crate::traits::ChainLogHandler for ContractEventHandlers<T, Db>
where
    T: HoprIndexerRpcOperations + Clone + Send + Sync + 'static,
    Db: BlokliDbAllOperations + Clone + Debug + Send + Sync + 'static,
{
    fn contract_addresses(&self) -> Vec<Address> {
        vec![
            self.addresses.announcements,
            self.addresses.channels,
            self.addresses.ticket_price_oracle,
            self.addresses.winning_probability_oracle,
            self.addresses.node_safe_registry,
            self.addresses.token,
        ]
    }

    fn contract_addresses_map(&self) -> Arc<ContractAddresses> {
        self.addresses.clone()
    }

    fn contract_address_topics(&self, contract: Address) -> Vec<B256> {
        if contract.eq(&self.addresses.announcements) {
            crate::constants::topics::announcement()
        } else if contract.eq(&self.addresses.channels) {
            crate::constants::topics::channel()
        } else if contract.eq(&self.addresses.ticket_price_oracle) {
            crate::constants::topics::ticket_price_oracle()
        } else if contract.eq(&self.addresses.winning_probability_oracle) {
            crate::constants::topics::winning_prob_oracle()
        } else if contract.eq(&self.addresses.node_safe_registry) {
            crate::constants::topics::node_safe_registry()
        } else if contract.eq(&self.addresses.token) {
            crate::constants::topics::token()
        } else {
            panic!("use of unsupported contract address: {contract}");
        }
    }

    async fn collect_log_event(&self, slog: SerializableLog, is_synced: bool) -> Result<()> {
        let myself = self.clone();
        self.db
            .begin_transaction()
            .await?
            .perform(move |tx| {
                let log = slog.clone();
                let tx_hash = Hash::from(log.tx_hash);
                let log_id = log.log_index;
                let block_id = log.block_number;

                Box::pin(async move {
                    match myself.process_log_event(tx, log, is_synced).await {
                        Ok(()) => {
                            debug!(block_id, %tx_hash, log_id, "processed log successfully");
                            Ok(())
                        }
                        Err(error) => {
                            error!(block_id, %tx_hash, log_id, %error, "error processing log in tx");
                            Err(error)
                        }
                    }
                })
            })
            .await
    }
}
