use blokli_chain_rpc::HoprIndexerRpcOperations;
use blokli_chain_types::AlloyAddressExt;
use blokli_db::{BlokliDbAllOperations, OpenTransaction, api::info::DomainSeparator, errors::DbSqlError};
use hopr_bindings::hopr_node_safe_registry::HoprNodeSafeRegistry::HoprNodeSafeRegistryEvents;
use hopr_primitive_types::prelude::{Address, ToHex};
use tracing::{debug, info, warn};

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
    /// - `RegisteredNodeSafe`: creates a safe contract entry if it doesn't exist. Fetches module address from the Safe
    ///   contract via RPC if no existing entry with a module address is found. Idempotent due to unique constraint on
    ///   event coordinates.
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
                    "Processing RegisteredNodeSafe event"
                );

                // Check for existing safe entry with non-zero module address
                let existing_module = match self.db.get_safe_contract_by_address(Some(tx), safe_addr).await {
                    Ok(Some(safe)) => {
                        let module = Address::try_from(safe.module_address.as_slice()).ok();
                        // Only use if non-zero
                        module.filter(|addr| *addr != Address::default())
                    }
                    Ok(None) => None,
                    Err(e) => {
                        warn!(
                            safe_address = %safe_addr.to_hex(),
                            error = %e,
                            "DB lookup failed for safe address, will fetch module from RPC"
                        );
                        None
                    }
                };

                let module_addr = if let Some(addr) = existing_module {
                    info!(
                        module_address = %addr.to_hex(),
                        "Using existing module address from DB"
                    );
                    addr
                } else {
                    // Fetch module address from Safe contract via RPC
                    match self._rpc_operations.get_hopr_module_from_safe(safe_addr).await {
                        Ok(Some(addr)) => {
                            info!(
                                module_address = %addr.to_hex(),
                                "Found HOPR module from RPC"
                            );
                            addr
                        }
                        Ok(None) => {
                            warn!(
                                safe_address = %safe_addr.to_hex(),
                                "No HOPR module found for safe, using zero address"
                            );
                            Address::default()
                        }
                        Err(e) => {
                            warn!(
                                safe_address = %safe_addr.to_hex(),
                                error = %e,
                                "Failed to fetch module from safe, using zero address"
                            );
                            Address::default()
                        }
                    }
                };

                // Upsert safe contract entry with the determined module address
                let _safe_id = self
                    .db
                    .upsert_safe_contract(Some(tx), safe_addr, module_addr, node_addr, block, tx_index, log_index)
                    .await?;

                // Register node to safe in the registration table
                let _registration_id = self
                    .db
                    .register_node_to_safe(Some(tx), safe_addr, node_addr, block, tx_index, log_index)
                    .await?;

                debug!(
                    node_address = %node_addr.to_hex(),
                    safe_address = %safe_addr.to_hex(),
                    module_address = %module_addr.to_hex(),
                    "Safe contract entry and node registration created from RegisteredNodeSafe"
                );
            }
            HoprNodeSafeRegistryEvents::DeregisteredNodeSafe(deregistered) => {
                let safe_addr = deregistered.safeAddress.to_hopr_address();
                let node_addr = deregistered.nodeAddress.to_hopr_address();

                info!(
                    node_address = %node_addr.to_hex(),
                    safe_address = %safe_addr.to_hex(),
                    "Deleting node registration from DeregisteredNodeSafe event"
                );

                // Deregister node from safe (idempotent - ignore if already deregistered)
                // Note: This does NOT delete the safe contract itself, only the node registration
                match self.db.deregister_node_from_safe(Some(tx), safe_addr, node_addr).await {
                    Ok(()) => {
                        debug!(
                            node_address = %node_addr.to_hex(),
                            safe_address = %safe_addr.to_hex(),
                            "Node registration deleted"
                        );
                    }
                    Err(DbSqlError::EntityNotFound(_)) => {
                        debug!(
                            node_address = %node_addr.to_hex(),
                            safe_address = %safe_addr.to_hex(),
                            "Node registration already deleted, skipping"
                        );
                    }
                    Err(e) => return Err(e.into()),
                }
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
    use blokli_db::{
        BlokliDbGeneralModelOperations, db::BlokliDb, node_safe_registrations::BlokliDbNodeSafeRegistrationOperations,
        safe_contracts::BlokliDbSafeContractOperations,
    };
    use blokli_db_entity::{
        hopr_node_safe_registration, hopr_safe_contract,
        prelude::{HoprNodeSafeRegistration, HoprSafeContract},
    };
    use hopr_bindings::hopr_node_safe_registry::HoprNodeSafeRegistry;
    use hopr_primitive_types::prelude::{Address, SerializableLog};
    use primitive_types::H256;
    use sea_orm::{ColumnTrait, EntityTrait, PaginatorTrait, QueryFilter};

    use crate::handlers::{node_safe_registry::tests::SAFE_INSTANCE_ADDR, test_utils::test_helpers::*};

    #[tokio::test]
    async fn test_on_node_safe_registry_registered_creates_safe_with_module_from_rpc() -> anyhow::Result<()> {
        let db = BlokliDb::new_in_memory().await?;
        let mut rpc_operations = MockIndexerRpcOperations::new();

        let module_address: Address = "aabbccddee00112233445566778899aabbccddee".parse()?;

        // Set up expectation for get_hopr_module_from_safe - should return the module address
        rpc_operations
            .expect_get_hopr_module_from_safe()
            .returning(move |_| Ok(Some(module_address)));

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
                HoprNodeSafeRegistry::RegisteredNodeSafe::SIGNATURE_HASH.into(),
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

        // Verify safe was created in database with module address from RPC
        let safe = HoprSafeContract::find()
            .filter(hopr_safe_contract::Column::Address.eq(safe_address.as_ref().to_vec()))
            .one(db.conn(blokli_db::TargetDb::Index))
            .await?
            .expect("safe should exist");

        assert_eq!(safe.address, safe_address.as_ref().to_vec());
        assert_eq!(safe.chain_key, node_address.as_ref().to_vec());
        assert_eq!(safe.module_address, module_address.as_ref().to_vec()); // Module should be from RPC
        assert_eq!(safe.deployed_block, 100);
        assert_eq!(safe.deployed_tx_index, 5);
        assert_eq!(safe.deployed_log_index, 10);

        Ok(())
    }

    #[tokio::test]
    async fn test_on_node_safe_registry_registered_uses_existing_module_address() -> anyhow::Result<()> {
        let db = BlokliDb::new_in_memory().await?;

        let safe_address: Address = "1234567890abcdef1234567890abcdef12345678".parse()?;
        let node_address = *SELF_CHAIN_ADDRESS;
        let existing_module_address: Address = "aabbccddee00112233445566778899aabbccddee".parse()?;

        // Pre-create safe entry with module address (as would happen from NewHoprNodeStakeModuleForSafe event)
        // Use the same event coordinates as the RegisteredNodeSafe event will have
        db.create_safe_contract(None, safe_address, existing_module_address, node_address, 100, 5, 10)
            .await?;

        let mut rpc_operations = MockIndexerRpcOperations::new();

        // RPC should not be called since existing entry has module address
        // If it were called, this would panic because we didn't set up expectation
        rpc_operations.expect_get_hopr_module_from_safe().never();

        let clonable_rpc_operations = ClonableMockOperations {
            inner: Arc::new(rpc_operations),
        };
        let handlers = init_handlers(clonable_rpc_operations, db.clone());

        let encoded_data = ().abi_encode();

        let safe_registered_log = SerializableLog {
            address: handlers.addresses.node_safe_registry,
            topics: vec![
                HoprNodeSafeRegistry::RegisteredNodeSafe::SIGNATURE_HASH.into(),
                H256::from_slice(&safe_address.to_bytes32()).into(),
                H256::from_slice(&node_address.to_bytes32()).into(),
            ],
            data: encoded_data,
            block_number: 100,
            tx_index: 5,
            log_index: 10,
            ..test_log()
        };

        // Process the event - should use existing module address and not create a new entry
        db.begin_transaction()
            .await?
            .perform(|tx| {
                Box::pin(async move { handlers.process_log_event(tx, safe_registered_log.clone(), true).await })
            })
            .await?;

        // Verify only 1 entry exists (the original one, since unique address constraint)
        let safes = HoprSafeContract::find()
            .filter(hopr_safe_contract::Column::Address.eq(safe_address.as_ref().to_vec()))
            .all(db.conn(blokli_db::TargetDb::Index))
            .await?;
        assert_eq!(safes.len(), 1);

        // The entry should have the existing module address
        assert_eq!(safes[0].module_address, existing_module_address.as_ref().to_vec());

        Ok(())
    }

    #[tokio::test]
    async fn test_on_node_safe_registry_registered_fetches_module_when_existing_is_zero() -> anyhow::Result<()> {
        let db = BlokliDb::new_in_memory().await?;

        let safe_address: Address = "1234567890abcdef1234567890abcdef12345678".parse()?;
        let node_address = *SELF_CHAIN_ADDRESS;
        let rpc_module_address: Address = "aabbccddee00112233445566778899aabbccddee".parse()?;

        // Pre-create safe entry with zero module address using different event coordinates
        // This simulates a case where a previous event created the safe with zero module
        db.create_safe_contract(None, safe_address, Address::default(), node_address, 50, 0, 0)
            .await?;

        let mut rpc_operations = MockIndexerRpcOperations::new();

        // RPC should be called since existing entry has zero module address
        rpc_operations
            .expect_get_hopr_module_from_safe()
            .returning(move |_| Ok(Some(rpc_module_address)));

        let clonable_rpc_operations = ClonableMockOperations {
            inner: Arc::new(rpc_operations),
        };
        let handlers = init_handlers(clonable_rpc_operations, db.clone());

        let encoded_data = ().abi_encode();

        let safe_registered_log = SerializableLog {
            address: handlers.addresses.node_safe_registry,
            topics: vec![
                HoprNodeSafeRegistry::RegisteredNodeSafe::SIGNATURE_HASH.into(),
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

        // The handler should succeed with upsert - it updates the existing entry
        // Verify the RPC was called and the existing entry was updated with the module address
        let safe = HoprSafeContract::find()
            .filter(hopr_safe_contract::Column::Address.eq(safe_address.as_ref().to_vec()))
            .one(db.conn(blokli_db::TargetDb::Index))
            .await?
            .expect("safe should exist");

        // The module address should now be updated to the RPC value (not zero anymore)
        assert_eq!(safe.module_address, rpc_module_address.as_ref().to_vec());
        // The deployment coordinates should also be updated to the new event
        assert_eq!(safe.deployed_block, 100);
        assert_eq!(safe.deployed_tx_index, 5);
        assert_eq!(safe.deployed_log_index, 10);

        Ok(())
    }

    #[tokio::test]
    async fn test_on_node_safe_registry_registered_uses_zero_when_rpc_returns_none() -> anyhow::Result<()> {
        let db = BlokliDb::new_in_memory().await?;
        let mut rpc_operations = MockIndexerRpcOperations::new();

        // Set up expectation for get_hopr_module_from_safe - returns None (no HOPR module found)
        rpc_operations
            .expect_get_hopr_module_from_safe()
            .returning(|_| Ok(None));

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
                HoprNodeSafeRegistry::RegisteredNodeSafe::SIGNATURE_HASH.into(),
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

        // Verify safe was created with zero address for module
        let safe = HoprSafeContract::find()
            .filter(hopr_safe_contract::Column::Address.eq(safe_address.as_ref().to_vec()))
            .one(db.conn(blokli_db::TargetDb::Index))
            .await?
            .expect("safe should exist");

        assert_eq!(safe.module_address, Address::default().as_ref().to_vec()); // Zero address

        Ok(())
    }

    #[tokio::test]
    async fn test_on_node_safe_registry_registered_idempotency() -> anyhow::Result<()> {
        let db = BlokliDb::new_in_memory().await?;
        let mut rpc_operations = MockIndexerRpcOperations::new();

        let module_address: Address = "aabbccddee00112233445566778899aabbccddee".parse()?;

        // Allow multiple calls to get_hopr_module_from_safe
        rpc_operations
            .expect_get_hopr_module_from_safe()
            .returning(move |_| Ok(Some(module_address)));

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
                HoprNodeSafeRegistry::RegisteredNodeSafe::SIGNATURE_HASH.into(),
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
        let handlers_clone = handlers.clone();
        let safe_registered_log_clone = safe_registered_log.clone();
        db.begin_transaction()
            .await?
            .perform(|tx| {
                Box::pin(async move {
                    handlers_clone
                        .process_log_event(tx, safe_registered_log_clone.clone(), true)
                        .await
                })
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

        // Also register the node to the safe
        db.register_node_to_safe(None, safe_address, node_address, 10, 0, 0)
            .await?;

        // Verify safe exists before deregistration
        let safe_before = HoprSafeContract::find()
            .filter(hopr_safe_contract::Column::Address.eq(safe_address.as_ref().to_vec()))
            .one(db.conn(blokli_db::TargetDb::Index))
            .await?;
        assert!(safe_before.is_some());

        // Verify node registration exists before deregistration
        let registration_before = HoprNodeSafeRegistration::find()
            .filter(hopr_node_safe_registration::Column::SafeAddress.eq(safe_address.as_ref().to_vec()))
            .filter(hopr_node_safe_registration::Column::NodeAddress.eq(node_address.as_ref().to_vec()))
            .one(db.conn(blokli_db::TargetDb::Index))
            .await?;
        assert!(registration_before.is_some());

        let encoded_data = ().abi_encode();

        let deregistered_log = SerializableLog {
            address: handlers.addresses.node_safe_registry,
            topics: vec![
                HoprNodeSafeRegistry::DeregisteredNodeSafe::SIGNATURE_HASH.into(),
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

        // Verify node registration is deleted
        let registration_after = HoprNodeSafeRegistration::find()
            .filter(hopr_node_safe_registration::Column::SafeAddress.eq(safe_address.as_ref().to_vec()))
            .filter(hopr_node_safe_registration::Column::NodeAddress.eq(node_address.as_ref().to_vec()))
            .one(db.conn(blokli_db::TargetDb::Index))
            .await?;
        assert!(registration_after.is_none());

        // Verify HoprSafeContract still exists (only node registration was deleted)
        let safe_after = HoprSafeContract::find()
            .filter(hopr_safe_contract::Column::Address.eq(safe_address.as_ref().to_vec()))
            .one(db.conn(blokli_db::TargetDb::Index))
            .await?;
        assert!(safe_after.is_some());

        Ok(())
    }
}
