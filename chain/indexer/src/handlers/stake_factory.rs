use blokli_chain_rpc::HoprIndexerRpcOperations;
use blokli_chain_types::AlloyAddressExt;
use blokli_db::{BlokliDbAllOperations, OpenTransaction};
use hopr_bindings::hopr_node_stake_factory::HoprNodeStakeFactory::HoprNodeStakeFactoryEvents;
use hopr_types::{
    crypto::types::Hash,
    primitive::prelude::{SerializableLog, ToHex},
};
use tracing::{error, info};

use super::{ContractEventHandlers, LogBatchContext};
use crate::{errors::Result, state::IndexerEvent};

impl<T, Db> ContractEventHandlers<T, Db>
where
    T: HoprIndexerRpcOperations + Clone + Send + 'static,
    Db: BlokliDbAllOperations + Clone,
{
    /// Handle a HoprNodeStakeFactory event that deploys a new safe-module pair and record the safe in the database.
    ///
    /// When the event is `NewHoprNodeStakeModuleForSafe`, this creates a safe contract entry using the deployed
    /// safe and module addresses, resolves the transaction sender (chain key) via RPC, stores the entry in the DB,
    /// and—if `is_synced` is true—emits a `SafeDeployed` indexer event.
    ///
    /// # Parameters
    ///
    /// - `is_synced`: if `true`, publish a `SafeDeployed` event after creating the DB entry.
    ///
    /// # Returns
    ///
    /// `Ok(())` on success; `Err` if resolving the transaction sender via RPC or creating the DB entry fails.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// // Illustrative example — types and values are placeholders.
    /// # async fn example<H, D>(handler: &H, tx: &crate::db::OpenTransaction, log: &crate::chain::SerializableLog, event: crate::chain::HoprNodeStakeFactoryEvents)
    /// # where H: std::ops::Deref<Target=crate::chain::handlers::ContractEventHandlers<(), ()>> + Send + Sync {
    /// handler.on_stake_factory_event(tx, log, event, true, 123, 0, 0, &batch_context).await.unwrap();
    /// # }
    /// ```
    #[allow(clippy::too_many_arguments)]
    pub(super) async fn on_stake_factory_event(
        &self,
        tx: &OpenTransaction,
        log: &SerializableLog,
        event: HoprNodeStakeFactoryEvents,
        is_synced: bool,
        block: u64,
        tx_index: u64,
        log_index: u64,
        batch: &LogBatchContext,
    ) -> Result<Vec<IndexerEvent>> {
        let mut events = Vec::new();
        if let HoprNodeStakeFactoryEvents::NewHoprNodeStakeModuleForSafe(deployed) = event {
            let module_addr = deployed.module.to_hopr_address();
            let safe_addr = deployed.safe.to_hopr_address();
            let safe_previously_known = self
                .db
                .get_safe_contract_by_address(Some(tx), safe_addr)
                .await?
                .is_some();

            // Query RPC for transaction sender (this is the chain_key)
            let chain_key = self
                ._rpc_operations
                .get_transaction_sender(Hash::from(log.tx_hash))
                .await
                .map_err(|e| {
                    error!(
                        tx_hash = %Hash::from(log.tx_hash),
                        error = %e,
                        "Failed to get transaction sender for NewHoprNodeStakeModuleForSafe"
                    );
                    e
                })?;

            info!(
                chain_key = %chain_key.to_hex(),
                safe = %safe_addr.to_hex(),
                module = %module_addr.to_hex(),
                block,
                "Creating safe contract entry from deployment"
            );

            // Create safe contract entry
            let safe_id = self
                .db
                .create_safe_contract(Some(tx), safe_addr, module_addr, chain_key, block, tx_index, log_index)
                .await?;

            if !safe_previously_known && is_synced {
                self.backfill_safe_logs_in_discovery_block(tx, safe_addr, block, batch)
                    .await?;
                let epoch = self.indexer_state.mark_safe_filters_dirty();
                info!(
                    safe = %safe_addr.to_hex(),
                    epoch,
                    "Backfilled discovery-block Safe logs and marked Safe filters for refresh after deployment"
                );
            }

            info!(
                safe_id,
                safe = %safe_addr.to_hex(),
                "Safe contract entry created"
            );

            // Emit SafeDeployed event if synced
            if is_synced {
                events.push(crate::state::IndexerEvent::SafeDeployed(safe_addr));
            }
        } else {
            error!(
                tx_hash = %Hash::from(log.tx_hash),
                "Unhandled HoprNodeStakeFactory event variant"
            );
        }

        Ok(events)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };

    use blokli_chain_rpc::errors::RpcError;
    use blokli_chain_types::AlloyAddressExt;
    use blokli_db::{
        BlokliDbGeneralModelOperations, TargetDb, api::logs::BlokliDbLogOperations, db::BlokliDb,
        safe_contracts::BlokliDbSafeContractOperations, safe_history::BlokliDbSafeHistoryOperations,
    };
    use blokli_db_entity::{hopr_safe_contract, prelude::HoprSafeContract};
    use hopr_bindings::{
        exports::alloy::{
            primitives::{Address as AlloyAddress, U256},
            sol_types::SolEvent,
        },
        hopr_node_stake_factory::HoprNodeStakeFactory,
    };
    use hopr_types::{
        crypto::types::Hash,
        primitive::prelude::{Address, SerializableLog},
    };
    use mockall::predicate::*;
    use sea_orm::{ColumnTrait, EntityTrait, PaginatorTrait, QueryFilter};

    use crate::{
        custom_abis::safe_contract_events::SafeContract,
        handlers::test_utils::test_helpers::*,
        state::IndexerEvent,
        traits::{ChainLogHandler, PrefetchedLogData},
    };

    /// Generates a cryptographically random Hopr `Address`.
    ///
    /// # Examples
    ///
    /// ```
    /// let _addr = random_address();
    /// ```
    fn random_address() -> Address {
        Address::from(hopr_types::crypto_random::random_bytes())
    }

    /// Generates a cryptographically secure random `Hash`.
    ///
    /// # Examples
    ///
    /// ```
    /// let h: Hash = random_hash();
    /// let _ = h;
    /// ```
    fn random_hash() -> Hash {
        Hash::from(hopr_types::crypto_random::random_bytes())
    }

    #[tokio::test]
    async fn test_on_stake_factory_event_creates_safe() -> anyhow::Result<()> {
        let db = BlokliDb::new_in_memory().await?;
        let mut rpc_operations = MockIndexerRpcOperations::new();

        // Mock get_transaction_sender
        let tx_hash = random_hash();
        let sender = random_address();
        rpc_operations
            .expect_get_transaction_sender()
            .with(eq(tx_hash))
            .returning(move |_| Ok(sender));
        rpc_operations
            .expect_get_logs_for_address()
            .returning(|_, _, _, _| Ok(vec![]));

        let clonable_rpc_operations = ClonableMockOperations {
            inner: Arc::new(rpc_operations),
        };

        let (handlers, _indexer_state, mut event_rx) = init_handlers_with_events(clonable_rpc_operations, db.clone());

        // Create event
        let safe_address = random_address();
        let module_address = random_address();
        let event = HoprNodeStakeFactory::NewHoprNodeStakeModuleForSafe {
            safe: AlloyAddress::from_hopr_address(safe_address),
            module: AlloyAddress::from_hopr_address(module_address),
        };

        let encoded_data = event.encode_log_data();
        let log = SerializableLog {
            address: handlers.addresses.node_stake_factory,
            topics: encoded_data.topics().iter().map(|t| t.0).collect(),
            data: encoded_data.data.to_vec(),
            tx_hash: tx_hash.into(),
            block_number: 100,
            tx_index: 1,
            log_index: 2,
            ..test_log()
        };

        // Process event
        handlers.collect_log_event(log, true).await?;

        // Verify safe created in DB
        let safe = db.verify_safe_contract(None, safe_address, sender).await?;
        assert!(safe, "Safe should be created and verified");

        // Verify event published
        let event = try_recv_event(&mut event_rx).expect("Should receive event");
        match event {
            IndexerEvent::SafeDeployed(addr) => assert_eq!(addr, safe_address),
            _ => panic!("Unexpected event type"),
        }

        Ok(())
    }

    /// Verifies that processing a NewHoprNodeStakeModuleForSafe event fails when the RPC `get_transaction_sender`
    /// returns an error.
    ///
    /// # Examples
    ///
    /// ```
    /// // Arrange: mock RPC to return an error, initialize handlers, and prepare a NewHoprNodeStakeModuleForSafe log.
    /// // Act: call `collect_log_event` with the prepared log.
    /// // Assert: the result is an error.
    /// ```
    #[tokio::test]
    async fn test_on_stake_factory_event_rpc_failure() -> anyhow::Result<()> {
        let db = BlokliDb::new_in_memory().await?;
        let mut rpc_operations = MockIndexerRpcOperations::new();

        // Mock get_transaction_sender failure
        rpc_operations
            .expect_get_transaction_sender()
            .returning(|_| Err(RpcError::Other("RPC failed".into())));
        // The deployment log has its Safe's discovery block read ahead of processing, which
        // happens before the failing sender lookup.
        rpc_operations
            .expect_get_logs_for_address()
            .returning(|_, _, _, _| Ok(vec![]));

        let clonable_rpc_operations = ClonableMockOperations {
            inner: Arc::new(rpc_operations),
        };

        let handlers = init_handlers(clonable_rpc_operations, db.clone());

        // Create event
        let event = HoprNodeStakeFactory::NewHoprNodeStakeModuleForSafe {
            safe: AlloyAddress::from_hopr_address(random_address()),
            module: AlloyAddress::from_hopr_address(random_address()),
        };

        let encoded_data = event.encode_log_data();
        let log = SerializableLog {
            address: handlers.addresses.node_stake_factory,
            topics: encoded_data.topics().iter().map(|t| t.0).collect(),
            data: encoded_data.data.to_vec(),
            ..test_log()
        };

        // Process event - should fail
        let result = handlers.collect_log_event(log, true).await;
        assert!(result.is_err());

        Ok(())
    }

    #[tokio::test]
    async fn test_on_stake_factory_event_idempotency() -> anyhow::Result<()> {
        let db = BlokliDb::new_in_memory().await?;
        let mut rpc_operations = MockIndexerRpcOperations::new();

        // Mock get_transaction_sender
        let tx_hash = random_hash();
        let sender = random_address();
        rpc_operations
            .expect_get_transaction_sender()
            .with(eq(tx_hash))
            .times(2) // Called twice for duplicate event processing
            .returning(move |_| Ok(sender));
        rpc_operations
            .expect_get_logs_for_address()
            .times(1)
            .returning(|_, _, _, _| Ok(vec![]));

        let clonable_rpc_operations = ClonableMockOperations {
            inner: Arc::new(rpc_operations),
        };

        let (handlers, _indexer_state, mut event_rx) = init_handlers_with_events(clonable_rpc_operations, db.clone());

        // Create event
        let safe_address = random_address();
        let module_address = random_address();
        let event = HoprNodeStakeFactory::NewHoprNodeStakeModuleForSafe {
            safe: AlloyAddress::from_hopr_address(safe_address),
            module: AlloyAddress::from_hopr_address(module_address),
        };

        let encoded_data = event.encode_log_data();
        let log = SerializableLog {
            address: handlers.addresses.node_stake_factory,
            topics: encoded_data.topics().iter().map(|t| t.0).collect(),
            data: encoded_data.data.to_vec(),
            tx_hash: tx_hash.into(),
            block_number: 100,
            tx_index: 1,
            log_index: 2,
            ..test_log()
        };

        // Process event first time
        handlers.collect_log_event(log.clone(), true).await?;

        // Verify safe created in DB
        let safe_exists = db.verify_safe_contract(None, safe_address, sender).await?;
        assert!(safe_exists, "Safe should be created after first processing");

        // Verify event published
        let first_event = try_recv_event(&mut event_rx).expect("Should receive first event");
        match first_event {
            IndexerEvent::SafeDeployed(addr) => assert_eq!(addr, safe_address),
            _ => panic!("Unexpected event type"),
        }

        // Process the exact same event again (idempotency test)
        handlers.collect_log_event(log, true).await?;

        // Verify still only one safe entry (check via verification)
        let still_exists = db.verify_safe_contract(None, safe_address, sender).await?;
        assert!(still_exists, "Safe should still exist and verify correctly");

        // Verify second event published
        let second_event = try_recv_event(&mut event_rx).expect("Should receive second event");
        match second_event {
            IndexerEvent::SafeDeployed(addr) => assert_eq!(addr, safe_address),
            _ => panic!("Unexpected event type"),
        }

        // Verify no duplicate DB entries by querying the raw table
        let safe_count = HoprSafeContract::find()
            .filter(hopr_safe_contract::Column::Address.eq(safe_address.as_ref()))
            .count(db.conn(TargetDb::Index))
            .await?;
        assert_eq!(safe_count, 1, "Should only have one safe entry in database");

        Ok(())
    }

    #[tokio::test]
    async fn test_on_stake_factory_event_does_not_replay_prior_safe_setup_logs() -> anyhow::Result<()> {
        let db = BlokliDb::new_in_memory().await?;
        let mut rpc_operations = MockIndexerRpcOperations::new();

        let tx_hash = random_hash();
        let sender = random_address();
        rpc_operations
            .expect_get_transaction_sender()
            .with(eq(tx_hash))
            .returning(move |_| Ok(sender));
        rpc_operations
            .expect_get_logs_for_address()
            .withf(|_, _, from_block, to_block| *from_block == 100 && *to_block == 100)
            .returning(|_, _, _, _| Ok(vec![]));

        let clonable_rpc_operations = ClonableMockOperations {
            inner: Arc::new(rpc_operations),
        };
        let handlers = init_handlers(clonable_rpc_operations, db.clone());

        let safe_address = random_address();
        let module_address = random_address();
        let owner_one = random_address();
        let owner_two = random_address();

        let safe_setup = SafeContract::SafeSetup {
            initiator: AlloyAddress::from_hopr_address(sender),
            owners: vec![
                AlloyAddress::from_hopr_address(owner_one),
                AlloyAddress::from_hopr_address(owner_two),
            ],
            threshold: U256::from(2_u64),
            initializer: AlloyAddress::from_hopr_address(Address::default()),
            fallbackHandler: AlloyAddress::from_hopr_address(Address::default()),
        };

        let encoded_setup = safe_setup.encode_log_data();
        db.store_log(SerializableLog {
            address: safe_address,
            topics: encoded_setup.topics().iter().map(|topic| topic.0).collect(),
            data: encoded_setup.data.to_vec(),
            tx_hash: random_hash().into(),
            block_number: 99,
            tx_index: 0,
            log_index: 0,
            ..test_log()
        })
        .await?;

        let event = HoprNodeStakeFactory::NewHoprNodeStakeModuleForSafe {
            safe: AlloyAddress::from_hopr_address(safe_address),
            module: AlloyAddress::from_hopr_address(module_address),
        };

        let encoded_event = event.encode_log_data();
        handlers
            .collect_log_event(
                SerializableLog {
                    address: handlers.addresses.node_stake_factory,
                    topics: encoded_event.topics().iter().map(|topic| topic.0).collect(),
                    data: encoded_event.data.to_vec(),
                    tx_hash: tx_hash.into(),
                    block_number: 100,
                    tx_index: 1,
                    log_index: 2,
                    ..test_log()
                },
                true,
            )
            .await?;

        let owners = db.get_safe_owners(None, safe_address).await?;
        assert!(owners.is_empty());

        Ok(())
    }

    #[tokio::test]
    async fn test_on_stake_factory_event_backfills_safe_setup_from_discovery_block() -> anyhow::Result<()> {
        let db = BlokliDb::new_in_memory().await?;
        let mut rpc_operations = MockIndexerRpcOperations::new();

        let deployment_tx_hash = random_hash();
        let sender = random_address();
        let safe_address = random_address();
        let module_address = random_address();
        let owner_one = random_address();
        let owner_two = random_address();

        rpc_operations
            .expect_get_transaction_sender()
            .with(eq(deployment_tx_hash))
            .returning(move |_| Ok(sender));

        let safe_setup = SafeContract::SafeSetup {
            initiator: AlloyAddress::from_hopr_address(sender),
            owners: vec![
                AlloyAddress::from_hopr_address(owner_one),
                AlloyAddress::from_hopr_address(owner_two),
            ],
            threshold: U256::from(2_u64),
            initializer: AlloyAddress::from_hopr_address(Address::default()),
            fallbackHandler: AlloyAddress::from_hopr_address(Address::default()),
        };
        let encoded_setup = safe_setup.encode_log_data();
        let safe_setup_log = blokli_chain_rpc::Log {
            address: safe_address,
            topics: encoded_setup.topics().iter().map(|topic| Hash::from(topic.0)).collect(),
            data: encoded_setup.data.to_vec().into_boxed_slice(),
            tx_index: 19,
            block_number: 100,
            block_hash: random_hash(),
            tx_hash: random_hash(),
            log_index: 10_u64.into(),
            removed: false,
        };
        rpc_operations
            .expect_get_logs_for_address()
            .withf(move |address, _, from_block, to_block| {
                *address == safe_address && *from_block == 100 && *to_block == 100
            })
            .returning(move |_, _, _, _| Ok(vec![safe_setup_log.clone()]));

        let clonable_rpc_operations = ClonableMockOperations {
            inner: Arc::new(rpc_operations),
        };
        let handlers = init_handlers(clonable_rpc_operations, db.clone());

        let event = HoprNodeStakeFactory::NewHoprNodeStakeModuleForSafe {
            safe: AlloyAddress::from_hopr_address(safe_address),
            module: AlloyAddress::from_hopr_address(module_address),
        };
        let encoded_event = event.encode_log_data();
        handlers
            .collect_log_event(
                SerializableLog {
                    address: handlers.addresses.node_stake_factory,
                    topics: encoded_event.topics().iter().map(|topic| topic.0).collect(),
                    data: encoded_event.data.to_vec(),
                    tx_hash: deployment_tx_hash.into(),
                    block_number: 100,
                    tx_index: 20,
                    log_index: 201,
                    ..test_log()
                },
                true,
            )
            .await?;

        let mut owners = db.get_safe_owners(None, safe_address).await?;
        owners.sort_unstable();
        let mut expected = vec![owner_one, owner_two];
        expected.sort_unstable();
        assert_eq!(owners, expected);

        let stored_safe_log = db.get_log(100, 19, 10).await?;
        assert_eq!(stored_safe_log.address, safe_address);
        assert_eq!(stored_safe_log.processed, Some(true));

        Ok(())
    }

    /// Builds the `SafeSetup` log a Safe emits in the block it is deployed in.
    fn safe_setup_log(safe_address: Address, initiator: Address, owners: &[Address]) -> blokli_chain_rpc::Log {
        let safe_setup = SafeContract::SafeSetup {
            initiator: AlloyAddress::from_hopr_address(initiator),
            owners: owners.iter().map(|o| AlloyAddress::from_hopr_address(*o)).collect(),
            threshold: U256::from(owners.len() as u64),
            initializer: AlloyAddress::from_hopr_address(Address::default()),
            fallbackHandler: AlloyAddress::from_hopr_address(Address::default()),
        };
        let encoded_setup = safe_setup.encode_log_data();

        blokli_chain_rpc::Log {
            address: safe_address,
            topics: encoded_setup.topics().iter().map(|topic| Hash::from(topic.0)).collect(),
            data: encoded_setup.data.to_vec().into_boxed_slice(),
            tx_index: 19,
            block_number: 100,
            block_hash: random_hash(),
            tx_hash: random_hash(),
            log_index: 10_u64.into(),
            removed: false,
        }
    }

    /// Builds the factory log announcing a Safe deployment in block 100.
    fn deployment_log(node_stake_factory: Address, safe: Address, module: Address, tx_hash: Hash) -> SerializableLog {
        let event = HoprNodeStakeFactory::NewHoprNodeStakeModuleForSafe {
            safe: AlloyAddress::from_hopr_address(safe),
            module: AlloyAddress::from_hopr_address(module),
        };
        let encoded_event = event.encode_log_data();

        SerializableLog {
            address: node_stake_factory,
            topics: encoded_event.topics().iter().map(|topic| topic.0).collect(),
            data: encoded_event.data.to_vec(),
            tx_hash: tx_hash.into(),
            block_number: 100,
            tx_index: 20,
            log_index: 201,
            ..test_log()
        }
    }

    #[tokio::test]
    async fn test_discovery_block_safe_logs_are_read_before_the_transaction() -> anyhow::Result<()> {
        let db = BlokliDb::new_in_memory().await?;
        let mut rpc_operations = MockIndexerRpcOperations::new();

        let deployment_tx_hash = random_hash();
        let sender = random_address();
        let safe_address = random_address();
        let module_address = random_address();
        let owner = random_address();

        rpc_operations
            .expect_get_transaction_sender()
            .returning(move |_| Ok(sender));

        // Counts how often the Safe's discovery block is read, to show the backfill inside the
        // transaction reuses the pre-fetched logs rather than issuing its own round-trip.
        let discovery_reads = Arc::new(AtomicUsize::new(0));
        let observed_reads = discovery_reads.clone();
        let setup_log = safe_setup_log(safe_address, sender, &[owner]);
        rpc_operations
            .expect_get_logs_for_address()
            .withf(move |address, _, from_block, to_block| {
                *address == safe_address && *from_block == 100 && *to_block == 100
            })
            .returning(move |_, _, _, _| {
                observed_reads.fetch_add(1, Ordering::SeqCst);
                Ok(vec![setup_log.clone()])
            });

        let handlers = init_handlers(
            ClonableMockOperations {
                inner: Arc::new(rpc_operations),
            },
            db.clone(),
        );
        let log = deployment_log(
            handlers.addresses.node_stake_factory,
            safe_address,
            module_address,
            deployment_tx_hash,
        );

        // Before the indexer is synced no backfill happens, so nothing is read ahead of time.
        assert!(
            handlers
                .prefetch_log_data(std::slice::from_ref(&log), false)
                .await
                .safe_discovery_logs
                .is_empty()
        );
        assert_eq!(discovery_reads.load(Ordering::SeqCst), 0);

        let prefetched = handlers.prefetch_log_data(std::slice::from_ref(&log), true).await;
        assert_eq!(
            prefetched.safe_discovery_logs.get(&(safe_address, 100)).map(Vec::len),
            Some(1),
            "the discovery block of the deployed Safe should be read ahead of the transaction"
        );
        assert_eq!(discovery_reads.load(Ordering::SeqCst), 1);

        handlers.collect_log_events(vec![log], true, prefetched).await?;

        assert_eq!(
            discovery_reads.load(Ordering::SeqCst),
            1,
            "the backfill should reuse the pre-fetched discovery-block logs"
        );
        assert_eq!(db.get_safe_owners(None, safe_address).await?, vec![owner]);
        assert_eq!(db.get_log(100, 19, 10).await?.processed, Some(true));

        Ok(())
    }

    #[tokio::test]
    async fn test_rolled_back_batch_leaves_no_backfilled_log_behind() -> anyhow::Result<()> {
        let db = BlokliDb::new_in_memory().await?;
        let mut rpc_operations = MockIndexerRpcOperations::new();

        let deployment_tx_hash = random_hash();
        let sender = random_address();
        let safe_address = random_address();
        let module_address = random_address();
        let owner = random_address();

        rpc_operations
            .expect_get_transaction_sender()
            .returning(move |_| Ok(sender));

        let setup_log = safe_setup_log(safe_address, sender, &[owner]);
        rpc_operations
            .expect_get_logs_for_address()
            .returning(move |_, _, _, _| Ok(vec![setup_log.clone()]));

        let handlers = init_handlers(
            ClonableMockOperations {
                inner: Arc::new(rpc_operations),
            },
            db.clone(),
        );

        // A log of an unrelated contract fails the batch after the Safe deployment was applied,
        // which rolls the whole block back.
        let unknown_contract_log = SerializableLog {
            address: random_address(),
            block_number: 100,
            tx_index: 21,
            log_index: 202,
            ..test_log()
        };
        let logs = vec![
            deployment_log(
                handlers.addresses.node_stake_factory,
                safe_address,
                module_address,
                deployment_tx_hash,
            ),
            unknown_contract_log,
        ];

        assert!(
            handlers
                .collect_log_events(logs, true, PrefetchedLogData::default())
                .await
                .is_err()
        );

        // Neither the Safe state nor the dynamically discovered log survived the rollback, so the
        // retry will discover and process that log again.
        assert!(db.get_safe_owners(None, safe_address).await?.is_empty());
        assert!(
            db.get_log(100, 19, 10).await.is_err(),
            "the backfilled log should not have been stored by a rolled back batch"
        );

        Ok(())
    }
}
