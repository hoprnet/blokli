use std::{
    fmt::{Debug, Formatter},
    sync::Arc,
};

use async_trait::async_trait;
use blokli_chain_rpc::{HoprIndexerRpcOperations, Log};
use blokli_chain_types::{AlloyAddressExt, ContractAddresses};
use blokli_db::{BlokliDbAllOperations, OpenTransaction};
use curvy_bindings::curvy_aggregator_alpha_v2::CurvyAggregatorAlphaV2::CurvyAggregatorAlphaV2Events;
use futures::StreamExt;
use hopr_bindings::{
    exports::alloy::{
        primitives::{Address as AlloyAddress, B256, Log as AlloyLog},
        sol_types::{SolEvent, SolEventInterface},
    },
    hopr_announcements::HoprAnnouncements::HoprAnnouncementsEvents,
    hopr_channels::HoprChannels::HoprChannelsEvents,
    hopr_node_management_module::HoprNodeManagementModule::HoprNodeManagementModuleEvents,
    hopr_node_safe_registry::HoprNodeSafeRegistry::HoprNodeSafeRegistryEvents,
    hopr_node_stake_factory::HoprNodeStakeFactory::HoprNodeStakeFactoryEvents,
    hopr_service_registry::HoprServiceRegistry::HoprServiceRegistryEvents,
    hopr_ticket_price_oracle::HoprTicketPriceOracle::HoprTicketPriceOracleEvents,
    hopr_token::HoprToken::HoprTokenEvents,
    hopr_winning_probability_oracle::HoprWinningProbabilityOracle::HoprWinningProbabilityOracleEvents,
};
use hopr_types::{
    crypto::prelude::Hash,
    primitive::prelude::{Address, SerializableLog},
};
use tracing::{debug, error, trace, warn};

use crate::{
    IndexerState,
    custom_abis::safe_contract_events::SafeContract::{ExecutionFromModuleFailure, SafeContractEvents},
    errors::{CoreEthereumIndexerError, Result},
    numeric::{u64_to_u32, u256_to_u32, u256_to_u64},
    state::IndexerEvent,
    traits::PrefetchedTransactions,
};

mod announcements;
mod channel_utils;
mod channels;
mod curvy;
mod helpers;
mod node_safe_registry;
mod oracles;
mod safe_contracts;
mod service_registry;
mod stake_factory;
#[cfg(test)]
mod test_utils;
mod tokens;

#[cfg(all(feature = "telemetry", not(test)))]
use hopr_types::telemetry::MultiCounter;

#[cfg(all(feature = "telemetry", not(test)))]
lazy_static::lazy_static! {
    static ref METRIC_INDEXER_LOG_COUNTERS: MultiCounter =
        MultiCounter::new(
            "blokli_indexer_contract_log_count",
            "Counts of different HOPR contract logs processed by the Indexer",
            &["contract"]
    ).unwrap();
}

#[cfg(all(feature = "telemetry", not(test)))]
fn increment_indexer_contract_log_count(contract: &str) {
    METRIC_INDEXER_LOG_COUNTERS.increment(&[contract]);
}

/// Maximum number of concurrent JSON-RPC requests issued while pre-fetching the raw transactions
/// needed to decode Safe module execution failures.
const SAFE_TX_PREFETCH_CONCURRENCY: usize = 8;

/// Number of transaction hashes packed into a single batched JSON-RPC request.
///
/// Kept modest so that a provider enforcing a batch-size limit still accepts the request, and so
/// that one slow batch cannot stall every outstanding lookup.
const SAFE_TX_PREFETCH_BATCH_SIZE: usize = 16;

/// Event handling an object for on-chain operations
///
/// Once an on-chain operation is recorded by the [crate::block::Indexer], it is pre-processed
/// and passed on to this object that handles event-specific actions for each on-chain operation.
#[derive(Clone)]
pub struct ContractEventHandlers<T, Db> {
    /// Contract addresses whose events are processed, including the optional Curvy Aggregator.
    pub(super) addresses: Arc<ContractAddresses>,
    /// callbacks to inform other modules
    pub(super) db: Db,
    /// rpc operations to interact with the chain
    _rpc_operations: T,
    /// indexer state for publishing events to subscribers
    pub(super) indexer_state: IndexerState,
    pub(super) enable_safe_indexing: bool,
    pub(super) enable_curvy_indexing: bool,
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
    T: HoprIndexerRpcOperations + Clone + Send + Sync + 'static,
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
    pub fn new(
        addresses: ContractAddresses,
        db: Db,
        rpc_operations: T,
        indexer_state: IndexerState,
        enable_safe_indexing: bool,
        enable_curvy_indexing: bool,
    ) -> Self {
        Self {
            addresses: Arc::new(addresses),
            db,
            _rpc_operations: rpc_operations,
            indexer_state,
            enable_safe_indexing,
            enable_curvy_indexing,
        }
    }

    /// Pre-fetches the raw transactions required to decode Safe `ExecutionFromModuleFailure` logs.
    ///
    /// Each such log needs one `eth_getTransactionByHash` round-trip to tell a rejected ticket
    /// redemption from any other failed module call. Issuing them concurrently here, before the
    /// block's database transaction is opened, keeps a slow remote RPC endpoint from serializing
    /// one round-trip per failure event while the write transaction is held open.
    ///
    /// Transaction hashes are de-duplicated. A lookup that fails is simply omitted from the cache;
    /// the handler that needs it then retries inline, preserving the previous error semantics.
    async fn prefetch_safe_transaction_bytes(&self, slogs: &[SerializableLog]) -> PrefetchedTransactions {
        self.fetch_transaction_bytes(self.safe_transaction_hashes(slogs, &PrefetchedTransactions::new()))
            .await
    }

    /// Collects the de-duplicated transaction hashes the given logs still need looked up.
    ///
    /// Hashes already present in `known` are skipped, which is what lets the block pipeline
    /// pre-fetch ahead of time without the processing path repeating the work.
    fn safe_transaction_hashes(&self, slogs: &[SerializableLog], known: &PrefetchedTransactions) -> Vec<Hash> {
        if !self.enable_safe_indexing {
            return Vec::new();
        }

        let mut tx_hashes = slogs
            .iter()
            .filter(|slog| {
                slog.topics
                    .first()
                    .is_some_and(|topic| topic.as_slice() == ExecutionFromModuleFailure::SIGNATURE_HASH.as_slice())
            })
            .map(|slog| Hash::from(slog.tx_hash))
            .filter(|tx_hash| !known.contains_key(tx_hash))
            .collect::<Vec<_>>();
        tx_hashes.sort_unstable();
        tx_hashes.dedup();
        tx_hashes
    }

    /// Fetches the given transaction hashes as concurrent, batched JSON-RPC requests.
    async fn fetch_transaction_bytes(&self, tx_hashes: Vec<Hash>) -> PrefetchedTransactions {
        if tx_hashes.is_empty() {
            return PrefetchedTransactions::new();
        }

        debug!(count = tx_hashes.len(), "pre-fetching Safe transaction bytes");

        let batches = tx_hashes
            .chunks(SAFE_TX_PREFETCH_BATCH_SIZE)
            .map(<[Hash]>::to_vec)
            .collect::<Vec<_>>();

        futures::stream::iter(batches.into_iter().map(|batch| {
            let rpc = self._rpc_operations.clone();
            async move {
                let results = rpc.get_transaction_bytes_batch(&batch).await;
                batch
                    .into_iter()
                    .zip(results)
                    .filter_map(|(tx_hash, result)| match result {
                        Ok(bytes) => Some((tx_hash, bytes)),
                        Err(error) => {
                            warn!(%tx_hash, %error, "failed to pre-fetch Safe transaction bytes, will retry inline");
                            None
                        }
                    })
                    .collect::<Vec<_>>()
            }
        }))
        .buffer_unordered(SAFE_TX_PREFETCH_CONCURRENCY)
        .flat_map(futures::stream::iter)
        .collect()
        .await
    }

    fn is_safe_contract_topic(topic: &[u8; 32]) -> bool {
        crate::constants::topics::safe_contract()
            .iter()
            .any(|safe_topic| safe_topic.as_slice() == topic.as_slice())
    }

    #[allow(dead_code)]
    async fn on_node_management_module_event(
        &self,
        _tx: &OpenTransaction,
        _event: HoprNodeManagementModuleEvents,
        _is_synced: bool,
    ) -> Result<()> {
        #[cfg(all(feature = "telemetry", not(test)))]
        increment_indexer_contract_log_count("node_management_module");
        // Don't care at the moment
        Ok(())
    }

    /// Test-facing convenience wrapper that processes a single log without a pre-fetch cache.
    #[cfg(test)]
    async fn process_log_event(
        &self,
        tx: &OpenTransaction,
        slog: SerializableLog,
        is_synced: bool,
    ) -> Result<Vec<IndexerEvent>> {
        self.process_log_event_with_prefetch(tx, slog, is_synced, &PrefetchedTransactions::new())
            .await
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
    /// let _ = handler.process_log_event_with_prefetch(&tx, slog, is_synced, &Default::default()).await;
    /// # });
    /// ```
    ///
    /// Any transaction bytes already fetched by [`Self::prefetch_safe_transaction_bytes`] for the
    /// whole batch are reused instead of being looked up again over RPC.
    #[tracing::instrument(level = "debug", skip(self, slog, prefetched), fields(log=%slog))]
    async fn process_log_event_with_prefetch(
        &self,
        tx: &OpenTransaction,
        slog: SerializableLog,
        is_synced: bool,
        prefetched: &PrefetchedTransactions,
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
            let bn = u64_to_u32(log.block_number, "block_number")?;
            let tx_idx = u64_to_u32(log.tx_index, "tx_index")?;
            let log_idx = u256_to_u32(log.log_index, "log_index")?;
            let event = HoprAnnouncementsEvents::decode_log(&primitive_log)?;
            self.on_announcement_event(tx, event.data, bn, tx_idx, log_idx, is_synced)
                .await
        } else if log.address.eq(&self.addresses.node_stake_factory) {
            let event = HoprNodeStakeFactoryEvents::decode_log(&primitive_log)?;
            let block = log.block_number;
            let tx_idx = log.tx_index;
            let log_idx = u256_to_u64(log.log_index, "log_index")?;
            self.on_stake_factory_event(tx, &slog, event.data, is_synced, block, tx_idx, log_idx)
                .await
        } else if log.address.eq(&self.addresses.channels) {
            let event = HoprChannelsEvents::decode_log(&primitive_log)?;
            let block = u64_to_u32(log.block_number, "block_number")?;
            let tx_idx = u64_to_u32(log.tx_index, "tx_index")?;
            let log_idx = u256_to_u32(log.log_index, "log_index")?;
            match self
                .on_channel_event(tx, event.data, block, tx_idx, log_idx, is_synced)
                .await
            {
                Ok(res) => Ok(res),
                Err(CoreEthereumIndexerError::ChannelDoesNotExist) => {
                    // This is not an error, just a log that we don't have the channel in the DB
                    debug!(?log, "channel didn't exist in the db. Ignored event");
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
        } else if !self.addresses.service_registry.is_zero() && log.address.eq(&self.addresses.service_registry) {
            // Placed ahead of the Safe lookup below so that a registry log costs no database
            // query. The zero-address guard keeps a network without the registry from routing an
            // unrelated zero-address log here.
            let event = HoprServiceRegistryEvents::decode_log(&primitive_log)?;
            self.on_service_registry_event(tx, &log, event.data, is_synced).await
        } else if self
            .db
            .get_safe_contract_by_address(Some(tx), log.address)
            .await?
            .is_some()
        {
            if !self.enable_safe_indexing {
                debug!(address = %log.address, "Ignoring Safe contract event because Safe indexing is disabled");
                return Ok(vec![]);
            }
            let event = SafeContractEvents::decode_log(&primitive_log)?;
            self.on_safe_contract_event_with_prefetch(tx, log.address, &log, event.data, is_synced, prefetched)
                .await
        } else if slog.topics.first().is_some_and(Self::is_safe_contract_topic) {
            debug!(
                address = %log.address,
                "Ignoring Safe contract event for address not yet indexed as a Safe"
            );
            Ok(vec![])
        } else if log.address.eq(&self.addresses.ticket_price_oracle) {
            let event = HoprTicketPriceOracleEvents::decode_log(&primitive_log)?;
            self.on_ticket_price_oracle_event(tx, event.data, is_synced).await
        } else if log.address.eq(&self.addresses.winning_probability_oracle) {
            let event = HoprWinningProbabilityOracleEvents::decode_log(&primitive_log)?;
            self.on_ticket_winning_probability_oracle_event(tx, event.data, is_synced)
                .await
        } else if self.enable_curvy_indexing && log.address.eq(&self.addresses.curvy_aggregator) {
            let event = CurvyAggregatorAlphaV2Events::decode_log(&primitive_log)?;
            self.on_curvy_aggregator_event(tx, &log, event.data).await
        } else {
            #[cfg(all(feature = "telemetry", not(test)))]
            increment_indexer_contract_log_count("unknown");

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
    /// node_safe_registry, node_stake_factory, token, and - only where it is deployed -
    /// service_registry, followed by the configured Curvy Aggregator address when indexing is enabled.
    ///
    /// The service registry is the one optional entry. Networks without a deployed registry
    /// carry the zero address for it, and filtering `eth_getLogs` on the null address is both
    /// meaningless and expensive, so it is skipped there.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// let addrs = handlers.contract_addresses();
    /// assert_eq!(addrs.len(), 8); // 7 where the service registry is not deployed
    /// // order: announcements, channels, ticket_price_oracle, winning_probability_oracle,
    /// // node_safe_registry, node_stake_factory, token, optional service_registry, optional Curvy Aggregator
    /// ```
    fn contract_addresses(&self) -> Vec<Address> {
        let mut addresses = vec![
            self.addresses.announcements,
            self.addresses.channels,
            self.addresses.ticket_price_oracle,
            self.addresses.winning_probability_oracle,
            self.addresses.node_safe_registry,
            self.addresses.node_stake_factory,
            self.addresses.token,
        ];
        if !self.addresses.service_registry.is_zero() {
            addresses.push(self.addresses.service_registry);
        }

        if self.enable_curvy_indexing {
            addresses.push(self.addresses.curvy_aggregator);
        }
        addresses
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
        } else if !self.addresses.service_registry.is_zero() && contract.eq(&self.addresses.service_registry) {
            // The zero-address guard keeps a network without the registry from matching here on
            // an unrelated call with a zero address, which would silently return registry topics.
            crate::constants::topics::service_registry()
        } else if self.enable_curvy_indexing && contract.eq(&self.addresses.curvy_aggregator) {
            crate::constants::topics::curvy_aggregator()
        } else {
            panic!("use of unsupported contract address: {contract}");
        }
    }

    async fn collect_log_event(&self, slog: SerializableLog, is_synced: bool) -> Result<()> {
        self.collect_log_events(vec![slog], is_synced, PrefetchedTransactions::new())
            .await
    }

    async fn prefetch_log_data(&self, logs: &[SerializableLog]) -> PrefetchedTransactions {
        self.prefetch_safe_transaction_bytes(logs).await
    }

    fn supports_atomic_batches(&self) -> bool {
        true
    }

    async fn collect_log_events(
        &self,
        slogs: Vec<SerializableLog>,
        is_synced: bool,
        mut prefetched: PrefetchedTransactions,
    ) -> Result<()> {
        // Fetch whatever the block pipeline has not already fetched up-front and concurrently, so
        // the database transaction below performs no blocking RPC round-trips.
        let missing = self.safe_transaction_hashes(&slogs, &prefetched);
        prefetched.extend(self.fetch_transaction_bytes(missing).await);
        let prefetched = Arc::new(prefetched);

        let myself = self.clone();
        let events = self
            .db
            .begin_transaction()
            .await?
            .perform(move |tx| {
                Box::pin(async move {
                    let mut events = Vec::new();

                    for log in slogs {
                        let tx_hash = Hash::from(log.tx_hash);
                        let log_id = log.log_index;
                        let block_id = log.block_number;

                        match myself
                            .process_log_event_with_prefetch(tx, log, is_synced, prefetched.as_ref())
                            .await
                        {
                            Ok(log_events) => {
                                debug!(block_id, %tx_hash, log_id, "processed log successfully");
                                events.extend(log_events);
                            }
                            Err(error) => {
                                error!(block_id, %tx_hash, log_id, %error, "error processing log in tx");
                                return Err(error);
                            }
                        }
                    }

                    Ok(events)
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

    async fn revert_block_derived_state(&self, from_block: u64) -> Result<()> {
        self.revert_curvy_state(from_block).await
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
    use hopr_types::{
        crypto::keypairs::Keypair,
        primitive::{
            prelude::{Address, SerializableLog},
            traits::ToHex,
        },
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
        rpc_operations
            .expect_get_logs_for_address()
            .returning(|_, _, _, _| Ok(vec![]));

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
