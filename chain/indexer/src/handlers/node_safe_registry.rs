use blokli_chain_rpc::HoprIndexerRpcOperations;
use blokli_chain_types::AlloyAddressExt;
use blokli_db::{BlokliDbAllOperations, OpenTransaction, api::info::DomainSeparator};
use hopr_bindings::hopr_node_safe_registry::HoprNodeSafeRegistry::HoprNodeSafeRegistryEvents;
use hopr_primitive_types::prelude::{Address, ToHex};
use tracing::{debug, info};

use super::ContractEventHandlers;
use crate::errors::Result;

#[cfg(all(feature = "prometheus", not(test)))]
lazy_static::lazy_static! {
    static ref METRIC_INDEXER_LOG_COUNTERS: hopr_metrics::MultiCounter =
        hopr_metrics::MultiCounter::new(
            "hopr_indexer_contract_log_count",
            "Counts of different HOPR contract logs processed by the Indexer",
            &["contract"]
    ).unwrap();
}

impl<T, Db> ContractEventHandlers<T, Db>
where
    T: HoprIndexerRpcOperations + Clone + Send + 'static,
    Db: BlokliDbAllOperations + Clone,
{
    /// Handle `HoprNodeSafeRegistryEvents` by creating or deleting safe entries in the database.
    ///
    /// Processes three event variants:
    /// - `RegisteredNodeSafe`: creates a safe contract entry if it doesn't exist. Uses zero address for module_address
    ///   since it's not provided in the event. Idempotent due to unique constraint on event coordinates.
    /// - `DeregisteredNodeSafe`: deletes the safe contract entry from the database.
    /// - `DomainSeparatorUpdated`: updates the SafeRegistry domain separator in the database.
    ///
    /// # Returns
    ///
    /// `Result<()>` indicating success, or an error if a database operation fails.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// # async fn run_example() -> crate::Result<()> {
    /// # use crate::ContractEventHandlers;
    /// # let handlers = todo!();
    /// # let tx = todo!();
    /// # let log = todo!();
    /// # let event = todo!();
    /// handlers.on_node_safe_registry_event(&tx, &log, event, true).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub(super) async fn on_node_safe_registry_event(
        &self,
        tx: &OpenTransaction,
        log: &blokli_chain_rpc::Log,
        event: HoprNodeSafeRegistryEvents,
        _is_synced: bool,
    ) -> Result<()> {
        #[cfg(all(feature = "prometheus", not(test)))]
        METRIC_INDEXER_LOG_COUNTERS.increment(&["node_safe_registry"]);

        match event {
            HoprNodeSafeRegistryEvents::RegisteredNodeSafe(registered) => {
                let safe_addr = registered.safeAddress.to_hopr_address();
                let node_addr = registered.nodeAddress.to_hopr_address();
                let block = log.block_number;
                let tx_index = log.tx_index;
                let log_index = log.log_index.as_u64();

                info!(
                    node_address = %node_addr.to_hex(),
                    safe_address = %safe_addr.to_hex(),
                    block,
                    tx_index,
                    log_index,
                    "Creating safe contract entry from RegisteredNodeSafe event"
                );

                // Create safe contract entry using zero address for module_address
                // since the RegisteredNodeSafe event doesn't provide module information
                let _safe_id = self
                    .db
                    .create_safe_contract(
                        Some(tx),
                        safe_addr,
                        Address::default(), // Use zero address for module since event doesn't provide it
                        node_addr,
                        block,
                        tx_index,
                        log_index,
                    )
                    .await?;

                debug!(
                    node_address = %node_addr.to_hex(),
                    safe_address = %safe_addr.to_hex(),
                    "Safe contract entry created from RegisteredNodeSafe"
                );
            }
            HoprNodeSafeRegistryEvents::DeregisteredNodeSafe(deregistered) => {
                let safe_addr = deregistered.safeAddress.to_hopr_address();
                let node_addr = deregistered.nodeAddress.to_hopr_address();

                info!(
                    node_address = %node_addr.to_hex(),
                    safe_address = %safe_addr.to_hex(),
                    "Deleting safe contract entry from DeregisteredNodeSafe event"
                );

                // Delete the safe contract entry
                self.db.delete_safe_contract(Some(tx), safe_addr).await?;

                debug!(
                    node_address = %node_addr.to_hex(),
                    safe_address = %safe_addr.to_hex(),
                    "Safe contract entry deleted"
                );
            }
            HoprNodeSafeRegistryEvents::DomainSeparatorUpdated(domain_separator_updated) => {
                self.db
                    .set_domain_separator(
                        Some(tx),
                        DomainSeparator::SafeRegistry,
                        domain_separator_updated.domainSeparator.0.into(),
                    )
                    .await?;
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use alloy::sol_types::{SolEvent, SolValue};
    use blokli_db::{BlokliDbGeneralModelOperations, db::BlokliDb, safe_contracts::BlokliDbSafeContractOperations};
    use blokli_db_entity::codegen::prelude::HoprSafeContract;
    use hopr_primitive_types::prelude::{Address, SerializableLog};
    use primitive_types::H256;
    use sea_orm::{ColumnTrait, EntityTrait, PaginatorTrait, QueryFilter};

    use crate::handlers::{node_safe_registry::tests::SAFE_INSTANCE_ADDR, test_utils::test_helpers::*};

    #[tokio::test]
    async fn test_on_node_safe_registry_registered_creates_safe() -> anyhow::Result<()> {
        let db = BlokliDb::new_in_memory().await?;
        let rpc_operations = MockIndexerRpcOperations::new();
        let clonable_rpc_operations = ClonableMockOperations {
            inner: Arc::new(rpc_operations),
        };
        let handlers = init_handlers(clonable_rpc_operations, db.clone());

        let safe_address = handlers.addresses.node_safe_registry; // Using registry addr as safe for test convenience
        let node_address = *SELF_CHAIN_ADDRESS; // Using self address as node

        let encoded_data = ().abi_encode();

        let safe_registered_log = SerializableLog {
            address: handlers.addresses.node_safe_registry,
            topics: vec![
                hopr_bindings::hopr_node_safe_registry::HoprNodeSafeRegistry::RegisteredNodeSafe::SIGNATURE_HASH.into(),
                H256::from_slice(&safe_address.to_bytes32()).into(),
                H256::from_slice(&node_address.to_bytes32()).into(),
            ],
            data: encoded_data,
            block_number: 100,
            tx_index: 5,
            log_index: 10,
            ..test_log()
        };

        // Process the event
        db.begin_transaction()
            .await?
            .perform(|tx| {
                Box::pin(async move { handlers.process_log_event(tx, safe_registered_log.clone(), true).await })
            })
            .await?;

        // Verify safe was created in database
        let safe = HoprSafeContract::find()
            .filter(blokli_db_entity::codegen::hopr_safe_contract::Column::Address.eq(safe_address.as_ref().to_vec()))
            .one(db.conn(blokli_db::TargetDb::Index))
            .await?
            .expect("safe should exist");

        assert_eq!(safe.address, safe_address.as_ref().to_vec());
        assert_eq!(safe.chain_key, node_address.as_ref().to_vec());
        assert_eq!(safe.module_address, Address::default().as_ref().to_vec()); // Module should be zero address
        assert_eq!(safe.deployed_block, 100);
        assert_eq!(safe.deployed_tx_index, 5);
        assert_eq!(safe.deployed_log_index, 10);

        Ok(())
    }

    #[tokio::test]
    async fn test_on_node_safe_registry_registered_idempotency() -> anyhow::Result<()> {
        let db = BlokliDb::new_in_memory().await?;
        let rpc_operations = MockIndexerRpcOperations::new();
        let clonable_rpc_operations = ClonableMockOperations {
            inner: Arc::new(rpc_operations),
        };
        let handlers = init_handlers(clonable_rpc_operations, db.clone());

        let safe_address = handlers.addresses.node_safe_registry;
        let node_address = *SELF_CHAIN_ADDRESS;

        let encoded_data = ().abi_encode();

        let safe_registered_log = SerializableLog {
            address: handlers.addresses.node_safe_registry,
            topics: vec![
                hopr_bindings::hopr_node_safe_registry::HoprNodeSafeRegistry::RegisteredNodeSafe::SIGNATURE_HASH.into(),
                H256::from_slice(&safe_address.to_bytes32()).into(),
                H256::from_slice(&node_address.to_bytes32()).into(),
            ],
            data: encoded_data,
            block_number: 100,
            tx_index: 5,
            log_index: 10,
            ..test_log()
        };

        // Process the event first time
        db.begin_transaction()
            .await?
            .perform(|tx| {
                Box::pin(async move { handlers.process_log_event(tx, safe_registered_log.clone(), true).await })
            })
            .await?;

        // Verify safe exists
        let count_before = HoprSafeContract::find()
            .count(db.conn(blokli_db::TargetDb::Index))
            .await?;
        assert_eq!(count_before, 1);

        // Process the same event again (idempotency test)
        db.begin_transaction()
            .await?
            .perform(|tx| {
                Box::pin(async move { handlers.process_log_event(tx, safe_registered_log.clone(), true).await })
            })
            .await?;

        // Verify still only one safe exists (no duplicate)
        let count_after = HoprSafeContract::find()
            .count(db.conn(blokli_db::TargetDb::Index))
            .await?;
        assert_eq!(count_after, 1);

        Ok(())
    }

    #[tokio::test]
    async fn test_on_node_safe_registry_deregistered() -> anyhow::Result<()> {
        let db = BlokliDb::new_in_memory().await?;
        let rpc_operations = MockIndexerRpcOperations::new();
        let clonable_rpc_operations = ClonableMockOperations {
            inner: Arc::new(rpc_operations),
        };
        let handlers = init_handlers(clonable_rpc_operations, db.clone());

        let safe_address = *SAFE_INSTANCE_ADDR;
        let node_address = *SELF_CHAIN_ADDRESS;

        // Pre-create the safe in DB
        db.create_safe_contract(
            None,
            safe_address,
            Address::from(hopr_crypto_random::random_bytes()),
            node_address,
            10,
            0,
            0,
        )
        .await?;

        // Verify safe exists before deregistration
        let safe_before = HoprSafeContract::find()
            .filter(blokli_db_entity::codegen::hopr_safe_contract::Column::Address.eq(safe_address.as_ref().to_vec()))
            .one(db.conn(blokli_db::TargetDb::Index))
            .await?;
        assert!(safe_before.is_some());

        let encoded_data = ().abi_encode();

        let deregistered_log = SerializableLog {
            address: handlers.addresses.node_safe_registry,
            topics: vec![
                hopr_bindings::hopr_node_safe_registry::HoprNodeSafeRegistry::DeregisteredNodeSafe::SIGNATURE_HASH
                    .into(),
                H256::from_slice(&safe_address.to_bytes32()).into(),
                H256::from_slice(&node_address.to_bytes32()).into(),
            ],
            data: encoded_data,
            ..test_log()
        };

        // Process deregistration event
        db.begin_transaction()
            .await?
            .perform(|tx| Box::pin(async move { handlers.process_log_event(tx, deregistered_log, true).await }))
            .await?;

        // Verify safe is deleted
        let safe_after = HoprSafeContract::find()
            .filter(blokli_db_entity::codegen::hopr_safe_contract::Column::Address.eq(safe_address.as_ref().to_vec()))
            .one(db.conn(blokli_db::TargetDb::Index))
            .await?;
        assert!(safe_after.is_none());

        Ok(())
    }
}
