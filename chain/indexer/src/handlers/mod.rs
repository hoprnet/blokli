use std::{
    fmt::{Debug, Formatter},
    sync::Arc,
};

use async_trait::async_trait;
use blokli_chain_rpc::{HoprIndexerRpcOperations, Log};
use blokli_chain_types::{AlloyAddressExt, ContractAddresses};
use blokli_db::{BlokliDbAllOperations, OpenTransaction};
use hopr_bindings::{
    exports::alloy::{
        primitives::{Address as AlloyAddress, B256, Log as AlloyLog},
        sol_types::SolEventInterface,
    },
    hopr_announcements::HoprAnnouncements::HoprAnnouncementsEvents,
    hopr_channels::HoprChannels::HoprChannelsEvents,
    hopr_node_management_module::HoprNodeManagementModule::HoprNodeManagementModuleEvents,
    hopr_node_safe_registry::HoprNodeSafeRegistry::HoprNodeSafeRegistryEvents,
    hopr_node_stake_factory::HoprNodeStakeFactory::HoprNodeStakeFactoryEvents,
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
    state::IndexerEvent,
};

mod announcements;
mod channel_utils;
mod channels;
mod helpers;
mod node_safe_registry;
mod oracles;
mod stake_factory;
#[cfg(test)]
mod test_utils;
mod tokens;

#[cfg(all(feature = "prometheus", not(test)))]
use hopr_metrics::MultiCounter;

#[cfg(all(feature = "prometheus", not(test)))]
lazy_static::lazy_static! {
    static ref METRIC_INDEXER_LOG_COUNTERS: MultiCounter =
        MultiCounter::new(
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

    /// Dispatches a single on-chain log to the appropriate contract event handler after decoding it.
    ///
    /// Decodes the provided `SerializableLog` into a primitive log, matches its contract address against
    /// known contract addresses, and forwards the decoded event to the corresponding `on_*_event` handler.
    /// Returns an error if decoding fails or if the log's contract address is not recognized. Channel
    /// events that map to `ChannelDoesNotExist` are treated as non-fatal and ignored.
    ///
    /// # Errors
    ///
    /// Returns `CoreEthereumIndexerError::ProcessError` if the log cannot be converted to a primitive log,
    /// `CoreEthereumIndexerError::UnknownContract` if the log's address is not one of the known contracts,
    /// or other `CoreEthereumIndexerError` variants produced by the specific handler invoked.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// # use Arc;
    /// # use Runtime;
    /// # // setup placeholders for the example — real types come from the library
    /// # let rt = Runtime::new().unwrap();
    /// # rt.block_on(async {
    /// #     // `handler` is an instance of ContractEventHandlers configured with addresses and db.
    /// #     // `tx` is an open database transaction handle and `slog` is a SerializableLog.
    /// #     let handler = /* ContractEventHandlers::new(...) */ unimplemented!();
    /// #     let tx = /* OpenTransaction */ unimplemented!();
    /// #     let slog = /* SerializableLog */ unimplemented!();
    /// let is_synced = true;
    /// // Awaiting the processing result; errors propagate as `CoreEthereumIndexerError`.
    /// let _ = handler.process_log_event(&tx, slog, is_synced).await;
    /// # });
    /// ```
    #[tracing::instrument(level = "debug", skip(self, slog), fields(log=%slog))]
    async fn process_log_event(
        &self,
        tx: &OpenTransaction,
        slog: SerializableLog,
        is_synced: bool,
    ) -> Result<Vec<IndexerEvent>> {
        trace!(log = %slog, "log content");

        let log = Log::from(slog.clone());

        let primitive_log = AlloyLog::new(
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
        } else if log.address.eq(&self.addresses.node_stake_factory) {
            let event = HoprNodeStakeFactoryEvents::decode_log(&primitive_log)?;
            let block = log.block_number;
            let tx_idx = log.tx_index;
            let log_idx = log.log_index.as_u64();
            self.on_stake_factory_event(tx, &slog, event.data, is_synced, block, tx_idx, log_idx)
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
                    Ok(vec![])
                }
                Err(e) => Err(e),
            }
        } else if log.address.eq(&self.addresses.token) {
            let event = HoprTokenEvents::decode_log(&primitive_log)?;
            self.on_token_event(tx, event.data, is_synced).await
        } else if log.address.eq(&self.addresses.node_safe_registry) {
            let event = HoprNodeSafeRegistryEvents::decode_log(&primitive_log)?;
            self.on_node_safe_registry_event(tx, &log, event.data, is_synced).await
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
    /// The contract addresses whose on-chain logs this handler processes.
    ///
    /// # Returns
    ///
    /// `Vec<Address>` containing the monitored contract addresses in the following order:
    /// announcements, channels, ticket_price_oracle, winning_probability_oracle,
    /// node_safe_registry, node_stake_factory, token.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// let addrs = handlers.contract_addresses();
    /// assert_eq!(addrs.len(), 7);
    /// // order: announcements, channels, ticket_price_oracle, winning_probability_oracle,
    /// // node_safe_registry, node_stake_factory, token
    /// ```
    fn contract_addresses(&self) -> Vec<Address> {
        vec![
            self.addresses.announcements,
            self.addresses.channels,
            self.addresses.ticket_price_oracle,
            self.addresses.winning_probability_oracle,
            self.addresses.node_safe_registry,
            self.addresses.node_stake_factory,
            self.addresses.token,
        ]
    }

    fn contract_addresses_map(&self) -> Arc<ContractAddresses> {
        self.addresses.clone()
    }

    /// Map a contract address to its associated event topics.
    ///
    /// Given a contract address managed by this handler, returns the list of event topic hashes
    /// (`Vec<B256>`) that should be used to filter logs for that contract.
    ///
    /// # Panics
    ///
    /// Panics if `contract` is not one of the supported contract addresses held in `self.addresses`.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// // assume `handler` is an instance of ContractEventHandlers and `addr` is one of its addresses
    /// let topics = handler.contract_address_topics(handler.addresses.announcements);
    /// assert!(!topics.is_empty());
    /// ```
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
        } else if contract.eq(&self.addresses.node_stake_factory) {
            crate::constants::topics::stake_factory()
        } else if contract.eq(&self.addresses.token) {
            crate::constants::topics::token()
        } else {
            panic!("use of unsupported contract address: {contract}");
        }
    }

    async fn collect_log_event(&self, slog: SerializableLog, is_synced: bool) -> Result<()> {
        let myself = self.clone();
        let events = self
            .db
            .begin_transaction()
            .await?
            .perform(move |tx| {
                let log = slog.clone();
                let tx_hash = Hash::from(log.tx_hash);
                let log_id = log.log_index;
                let block_id = log.block_number;

                Box::pin(async move {
                    match myself.process_log_event(tx, log, is_synced).await {
                        Ok(events) => {
                            debug!(block_id, %tx_hash, log_id, "processed log successfully");
                            Ok(events)
                        }
                        Err(error) => {
                            error!(block_id, %tx_hash, log_id, %error, "error processing log in tx");
                            Err(error)
                        }
                    }
                })
            })
            .await?;

        // Publish events after transaction commit
        if is_synced {
            for event in events {
                self.indexer_state.publish_event(event);
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::{sync::Arc, time::Duration};

    use blokli_db::{BlokliDbGeneralModelOperations, accounts::BlokliDbAccountOperations, db::BlokliDb};
    use blokli_db_entity::{hopr_safe_contract, prelude::HoprSafeContract};
    use hopr_bindings::{
        exports::alloy::sol_types::{SolEvent, SolValue},
        hopr_node_safe_registry::HoprNodeSafeRegistry,
    };
    use hopr_crypto_types::keypairs::Keypair;
    use hopr_primitive_types::{
        prelude::{Address, SerializableLog},
        traits::ToHex,
    };
    use primitive_types::H256;
    use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};

    use crate::{
        handlers::test_utils::test_helpers::{
            ClonableMockOperations, MockIndexerRpcOperations, SAFE_INSTANCE_ADDR, SELF_CHAIN_ADDRESS, SELF_PRIV_KEY,
            init_handlers_with_events, test_log,
        },
        state::IndexerEvent,
        traits::ChainLogHandler,
    };

    #[tokio::test]
    async fn test_collect_log_event_publishes_after_transaction_commit() -> anyhow::Result<()> {
        let db = BlokliDb::new_in_memory().await?;
        let mut rpc_operations = MockIndexerRpcOperations::new();

        // Setup RPC mock for RegisteredNodeSafe
        let module_addr: Address = "aabbccddee00112233445566778899aabbccddee".parse()?;
        rpc_operations
            .expect_get_hopr_module_from_safe()
            .returning(move |_| Ok(Some(module_addr)));

        let clonable_rpc = ClonableMockOperations {
            inner: Arc::new(rpc_operations),
        };
        let (handlers, _, mut event_receiver) = init_handlers_with_events(clonable_rpc, db.clone());

        let safe_addr = *SAFE_INSTANCE_ADDR;
        let node_addr = *SELF_CHAIN_ADDRESS;

        // Ensure the account exists so construct_account_update succeeds
        db.upsert_account(None, 1, node_addr, *SELF_PRIV_KEY.public(), None, 1, 0, 0)
            .await?;

        // Create log
        let encoded_data = ().abi_encode();
        let log = SerializableLog {
            address: handlers.addresses.node_safe_registry,
            topics: vec![
                HoprNodeSafeRegistry::RegisteredNodeSafe::SIGNATURE_HASH.into(),
                H256::from_slice(&safe_addr.to_bytes32()).into(),
                H256::from_slice(&node_addr.to_bytes32()).into(),
            ],
            data: encoded_data,
            block_number: 100,
            ..test_log()
        };

        // Spawn a listener that verifies data availability upon event receipt
        let db_check = db.clone();
        let listener = tokio::spawn(async move {
            // Wait for event
            let event = tokio::time::timeout(Duration::from_secs(5), event_receiver.recv())
                .await
                .expect("Should receive event")
                .expect("Channel shouldn't be closed");

            // Look for AccountUpdated event (since we removed SafeDeployed)
            match event {
                IndexerEvent::AccountUpdated(account) => {
                    assert_eq!(account.chain_key, node_addr.to_hex());
                }
                _ => panic!("Expected AccountUpdated event, got {:?}", event),
            };

            // Crucial check: Data must be visible in DB immediately when event is received
            // This proves that the transaction committed before the event was published.
            let safe = HoprSafeContract::find()
                .filter(hopr_safe_contract::Column::Address.eq(safe_addr.as_ref().to_vec()))
                .one(db_check.conn(blokli_db::TargetDb::Index))
                .await
                .expect("DB query failed");

            assert!(
                safe.is_some(),
                "Safe contract must be visible in DB when AccountUpdated event is received"
            );
        });

        // Execute handler via collect_log_event (which handles the transaction)
        handlers.collect_log_event(log, true).await?;

        // Await listener verification
        listener.await?;

        Ok(())
    }
}
