use blokli_chain_rpc::HoprIndexerRpcOperations;
use blokli_chain_types::AlloyAddressExt;
use blokli_db::{BlokliDbAllOperations, OpenTransaction};
use hopr_bindings::hopr_node_stake_factory::HoprNodeStakeFactory::HoprNodeStakeFactoryEvents;
use hopr_crypto_types::types::Hash;
use hopr_primitive_types::prelude::{SerializableLog, ToHex};
use tracing::{error, info};

use super::ContractEventHandlers;
use crate::errors::Result;

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
    /// handler.on_stake_factory_event(tx, log, event, true, 123, 0, 0).await.unwrap();
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
    ) -> Result<()> {
        if let HoprNodeStakeFactoryEvents::NewHoprNodeStakeModuleForSafe(deployed) = event {
            let module_addr = deployed.module.to_hopr_address();
            let safe_addr = deployed.safe.to_hopr_address();

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

            info!(
                safe_id,
                safe = %safe_addr.to_hex(),
                "Safe contract entry created"
            );

            // Emit SafeDeployed event if synced
            if is_synced {
                self.indexer_state
                    .publish_event(crate::state::IndexerEvent::SafeDeployed(safe_addr));
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use alloy::{
        primitives::{Address as AlloyAddress, B256},
        sol_types::{SolEvent, SolValue},
    };
    use blokli_chain_rpc::HoprIndexerRpcOperations;
    use blokli_chain_types::AlloyAddressExt;
    use blokli_db::{
        BlokliDbGeneralModelOperations, TargetDb, db::BlokliDb, safe_contracts::BlokliDbSafeContractOperations,
    };
    use blokli_db_entity::codegen::{hopr_safe_contract, prelude::*};
    use hopr_bindings::hopr_node_stake_factory::HoprNodeStakeFactory;
    use hopr_crypto_types::types::Hash;
    use hopr_primitive_types::{
        prelude::{Address, SerializableLog},
        traits::IntoEndian,
    };
    use mockall::predicate::*;
    use sea_orm::{ColumnTrait, EntityTrait, PaginatorTrait, QueryFilter};

    use crate::{
        handlers::test_utils::test_helpers::*,
        state::{IndexerEvent, IndexerState},
        traits::ChainLogHandler,
    };

    /// Generates a cryptographically random Hopr `Address`.
    ///
    /// # Examples
    ///
    /// ```
    /// let _addr = random_address();
    /// ```
    fn random_address() -> Address {
        Address::from(hopr_crypto_random::random_bytes())
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
        Hash::from(hopr_crypto_random::random_bytes())
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
            address: handlers.addresses.node_stake_v2_factory,
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
            .returning(|_| Err(blokli_chain_rpc::errors::RpcError::Other("RPC failed".into())));

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
            address: handlers.addresses.node_stake_v2_factory,
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
            address: handlers.addresses.node_stake_v2_factory,
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
}
