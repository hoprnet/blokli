//! Handler for `HoprServiceRegistry`, the permissionless registry of the services HOPR nodes
//! offer.
//!
//! Every registry log lands in the database first and is published to the subscription event bus
//! afterwards, so a subscriber that reacts to an event always finds the same state in a query.
//!
//! The payload of both event variants carries the kind of change explicitly. Deregistration
//! leaves no entry behind, and a registration is not distinguishable from an update by its
//! timestamps alone, so a bare entry cannot express what happened.

use std::time::{SystemTime, UNIX_EPOCH};

use blokli_api_types::{
    ServiceEntry as ApiServiceEntry, ServiceRegistryConfig as ApiServiceRegistryConfig,
    ServiceTypeInfo as ApiServiceTypeInfo, ServiceTypeUpdate, ServiceTypeUpdateKind, ServiceUpdate, ServiceUpdateKind,
    TokenValueString, UInt64,
};
use blokli_chain_rpc::{HoprIndexerRpcOperations, Log};
use blokli_chain_types::AlloyAddressExt;
use blokli_db::{
    BlokliDbAllOperations, OpenTransaction,
    errors::DbSqlError,
    services::{ServiceRegistryConfigInfo, ServiceTypeInfo},
};
use hopr_bindings::{
    exports::alloy::{
        hex,
        primitives::{Address as AlloyAddress, U256},
    },
    hopr_service_registry::HoprServiceRegistry::HoprServiceRegistryEvents,
};
use hopr_types::{
    internal::prelude::{ServiceEntry, ServiceType},
    primitive::prelude::{Address, HoprBalance, IntoEndian, ToHex},
};
use tracing::{debug, info, trace};

#[cfg(all(feature = "telemetry", not(test)))]
use super::increment_indexer_contract_log_count;
use super::{ContractEventHandlers, u256_to_u64};
use crate::{
    errors::{CoreEthereumIndexerError, Result},
    state::IndexerEvent,
};

/// Resolves the zero-address sentinel the registry uses for "no address".
///
/// An owner of zero marks an abandoned service type, and a requirement of zero marks a type that
/// is open to any node.
fn optional_address(address: AlloyAddress) -> Option<Address> {
    let address = address.to_hopr_address();
    (!address.is_zero()).then_some(address)
}

/// Converts an on-chain `uint256` amount into a wxHOPR balance.
fn to_balance(amount: U256) -> HoprBalance {
    HoprBalance::from_be_bytes(amount.to_be_bytes::<32>())
}

/// Converts an entry timestamp into the Unix seconds the GraphQL schema exposes.
fn unix_seconds(time: SystemTime) -> Result<UInt64> {
    let seconds = time
        .duration_since(UNIX_EPOCH)
        .map_err(|_| {
            CoreEthereumIndexerError::ProcessError("service entry timestamp precedes the Unix epoch".to_string())
        })?
        .as_secs();

    Ok(UInt64(seconds))
}

/// Converts a registry entry into its GraphQL representation.
///
/// The service type renders as its ASCII name when it follows the convention and as hex
/// otherwise, which is what [`ServiceType`]'s own `Display` does. The metadata stays opaque: its
/// schema belongs to the service type, so it is exposed as `0x`-prefixed hex.
pub(super) fn to_api_entry(entry: &ServiceEntry) -> Result<ApiServiceEntry> {
    Ok(ApiServiceEntry {
        service_type: entry.service_type.to_string(),
        node: entry.node.to_hex(),
        safe: entry.safe.to_hex(),
        metadata: hex::encode_prefixed(entry.metadata.as_ref()),
        registered_at: unix_seconds(entry.registered_at)?,
        updated_at: unix_seconds(entry.updated_at)?,
    })
}

/// Converts the state of a service type into its GraphQL representation.
pub(super) fn to_api_type_info(info: &ServiceTypeInfo) -> ApiServiceTypeInfo {
    ApiServiceTypeInfo {
        service_type: info.service_type.to_string(),
        owner: info.owner.map(|owner| owner.to_hex()),
        requirement: info.requirement.map(|requirement| requirement.to_hex()),
        registration_burn: TokenValueString(info.registration_burn.to_string()),
        update_burn: TokenValueString(info.update_burn.to_string()),
    }
}

/// Converts the registry-wide configuration into its GraphQL representation.
///
/// The node-safe registry pointer is reported as the zero address until the registry has emitted
/// it, which mirrors how the contract itself reads before initialization.
pub(super) fn to_api_registry_config(config: &ServiceRegistryConfigInfo) -> ApiServiceRegistryConfig {
    ApiServiceRegistryConfig {
        type_registration_fee: TokenValueString(config.type_registration_fee.to_string()),
        node_safe_registry: config.node_safe_registry.unwrap_or_default().to_hex(),
    }
}

impl<T, Db> ContractEventHandlers<T, Db>
where
    T: HoprIndexerRpcOperations + Clone + Send + 'static,
    Db: BlokliDbAllOperations + Clone,
{
    /// Builds the subscription event for a change to the configuration of a single service type.
    ///
    /// The configuration is read back from the database rather than assembled from the log, so
    /// that the payload carries every field of the type and not only the one that changed.
    async fn service_type_event(
        &self,
        tx: &OpenTransaction,
        service_type: ServiceType,
        kind: ServiceTypeUpdateKind,
    ) -> Result<Vec<IndexerEvent>> {
        let config = self.db.get_service_type(Some(tx), service_type).await?;

        Ok(vec![IndexerEvent::ServiceTypeUpdated(ServiceTypeUpdate {
            kind,
            service_type: Some(service_type.to_string()),
            config: config.as_ref().map(to_api_type_info),
            registry_config: None,
        })])
    }

    /// Builds the subscription event for a change to the registry-wide configuration.
    async fn registry_config_event(
        &self,
        tx: &OpenTransaction,
        kind: ServiceTypeUpdateKind,
    ) -> Result<Vec<IndexerEvent>> {
        let config = self.db.get_service_registry_config(Some(tx)).await?;

        Ok(vec![IndexerEvent::ServiceTypeUpdated(ServiceTypeUpdate {
            kind,
            service_type: None,
            config: None,
            registry_config: Some(to_api_registry_config(&config)),
        })])
    }

    /// Handle `HoprServiceRegistryEvents` by recording registry entries and service type
    /// configuration in the database.
    ///
    /// Ten of the contract's events change indexed state. `RegistryInitialized`, `TokensRecovered`
    /// and the access-control events carry deployment or permission bookkeeping with no counterpart
    /// in the indexed model; they are matched explicitly so that a future event cannot be added to
    /// the contract without this handler failing to compile.
    ///
    /// # Errors
    ///
    /// Returns [`CoreEthereumIndexerError::GeneralError`] when a log carries a zero service type or
    /// metadata above the contract's 2048-byte cap - both are rejected rather than truncated -
    /// and propagates database failures.
    ///
    /// # Returns
    ///
    /// The subscription events to publish, which is empty while the indexer is still catching up.
    pub(super) async fn on_service_registry_event(
        &self,
        tx: &OpenTransaction,
        log: &Log,
        event: HoprServiceRegistryEvents,
        is_synced: bool,
    ) -> Result<Vec<IndexerEvent>> {
        #[cfg(all(feature = "telemetry", not(test)))]
        increment_indexer_contract_log_count("service_registry");

        let block = log.block_number;
        let tx_index = log.tx_index;
        let log_index = u256_to_u64(log.log_index, "log_index")?;

        match event {
            HoprServiceRegistryEvents::Registered(registered) => {
                let entry = ServiceEntry::try_from(&registered)?;

                self.db
                    .upsert_service_entry(Some(tx), &entry, block, tx_index, log_index)
                    .await?;

                info!(%entry, block, "service registry entry registered");

                if is_synced {
                    return Ok(vec![IndexerEvent::ServiceEntryUpdated(ServiceUpdate {
                        kind: ServiceUpdateKind::Registered,
                        service_type: entry.service_type.to_string(),
                        node: entry.node.to_hex(),
                        entry: Some(to_api_entry(&entry)?),
                    })]);
                }
            }
            HoprServiceRegistryEvents::Updated(updated) => {
                let decoded = ServiceEntry::try_from(&updated)?;

                // The `Updated` event omits `registeredAt`, because an update leaves it untouched,
                // so the decoded entry carries `updatedAt` in both timestamps. Keep the stored
                // registration time whenever the entry is already indexed.
                let entry = match self
                    .db
                    .get_service_entry(Some(tx), decoded.service_type, decoded.node)
                    .await?
                {
                    Some(stored) => ServiceEntry::new(
                        decoded.service_type,
                        decoded.node,
                        decoded.safe,
                        decoded.metadata,
                        stored.registered_at,
                        decoded.updated_at,
                    )?,
                    None => decoded,
                };

                self.db
                    .upsert_service_entry(Some(tx), &entry, block, tx_index, log_index)
                    .await?;

                info!(%entry, block, "service registry entry updated");

                if is_synced {
                    return Ok(vec![IndexerEvent::ServiceEntryUpdated(ServiceUpdate {
                        kind: ServiceUpdateKind::Updated,
                        service_type: entry.service_type.to_string(),
                        node: entry.node.to_hex(),
                        entry: Some(to_api_entry(&entry)?),
                    })]);
                }
            }
            HoprServiceRegistryEvents::Deregistered(deregistered) => {
                let service_type = ServiceType::try_from(deregistered.serviceType)?;
                let node = deregistered.node.to_hopr_address();

                match self
                    .db
                    .deregister_service_entry(Some(tx), service_type, node, block, tx_index, log_index)
                    .await
                {
                    Ok(()) => {
                        info!(%service_type, %node, block, "service registry entry deregistered");

                        if is_synced {
                            return Ok(vec![IndexerEvent::ServiceEntryUpdated(ServiceUpdate {
                                kind: ServiceUpdateKind::Deregistered,
                                service_type: service_type.to_string(),
                                node: node.to_hex(),
                                entry: None,
                            })]);
                        }
                    }
                    Err(DbSqlError::EntityNotFound(_)) => {
                        // The entry was never indexed, so there is nothing to tombstone. This is
                        // the same tolerance the node-safe registry applies to a deregistration of
                        // a binding it does not know.
                        debug!(%service_type, %node, "deregistration of an entry that is not indexed, ignored");
                    }
                    Err(e) => return Err(e.into()),
                }
            }
            HoprServiceRegistryEvents::ServiceTypeRegistered(registered) => {
                let service_type = ServiceType::try_from(registered.serviceType)?;
                let owner = registered.owner.to_hopr_address();

                self.db
                    .register_service_type(Some(tx), service_type, owner, block, tx_index, log_index)
                    .await?;

                info!(%service_type, %owner, block, "service type registered");

                if is_synced {
                    return self
                        .service_type_event(tx, service_type, ServiceTypeUpdateKind::Registered)
                        .await;
                }
            }
            HoprServiceRegistryEvents::TypeOwnershipTransferred(transferred) => {
                let service_type = ServiceType::try_from(transferred.serviceType)?;
                let owner = optional_address(transferred.newOwner);

                self.db
                    .set_service_type_owner(Some(tx), service_type, owner, block, tx_index, log_index)
                    .await?;

                info!(%service_type, abandoned = owner.is_none(), "service type ownership transferred");

                if is_synced {
                    return self
                        .service_type_event(tx, service_type, ServiceTypeUpdateKind::OwnerChanged)
                        .await;
                }
            }
            HoprServiceRegistryEvents::RequirementUpdated(updated) => {
                let service_type = ServiceType::try_from(updated.serviceType)?;
                let requirement = optional_address(updated.requirement);

                self.db
                    .set_service_type_requirement(Some(tx), service_type, requirement, block, tx_index, log_index)
                    .await?;

                info!(%service_type, open = requirement.is_none(), "service type requirement updated");

                if is_synced {
                    return self
                        .service_type_event(tx, service_type, ServiceTypeUpdateKind::RequirementChanged)
                        .await;
                }
            }
            HoprServiceRegistryEvents::SelfRegistrationBurnUpdated(updated) => {
                let service_type = ServiceType::try_from(updated.serviceType)?;
                let burn = to_balance(updated.amount);

                self.db
                    .set_service_type_registration_burn(Some(tx), service_type, burn, block, tx_index, log_index)
                    .await?;

                info!(%service_type, %burn, "service type registration burn updated");

                if is_synced {
                    return self
                        .service_type_event(tx, service_type, ServiceTypeUpdateKind::RegistrationBurnChanged)
                        .await;
                }
            }
            HoprServiceRegistryEvents::SelfUpdateBurnUpdated(updated) => {
                let service_type = ServiceType::try_from(updated.serviceType)?;
                let burn = to_balance(updated.amount);

                self.db
                    .set_service_type_update_burn(Some(tx), service_type, burn, block, tx_index, log_index)
                    .await?;

                info!(%service_type, %burn, "service type update burn updated");

                if is_synced {
                    return self
                        .service_type_event(tx, service_type, ServiceTypeUpdateKind::UpdateBurnChanged)
                        .await;
                }
            }
            HoprServiceRegistryEvents::TypeRegistrationFeeUpdated(updated) => {
                let fee = to_balance(updated.amount);

                self.db
                    .set_type_registration_fee(Some(tx), fee, block, tx_index, log_index)
                    .await?;

                info!(%fee, "service type registration fee updated");

                if is_synced {
                    return self
                        .registry_config_event(tx, ServiceTypeUpdateKind::RegistrationFeeChanged)
                        .await;
                }
            }
            HoprServiceRegistryEvents::NodeSafeRegistryUpdated(updated) => {
                let registry = updated.newNodeSafeRegistry.to_hopr_address();

                self.db
                    .set_node_safe_registry(Some(tx), registry, block, tx_index, log_index)
                    .await?;

                info!(%registry, "service registry node-safe registry pointer updated");

                if is_synced {
                    return self
                        .registry_config_event(tx, ServiceTypeUpdateKind::RegistryPointerChanged)
                        .await;
                }
            }
            HoprServiceRegistryEvents::RegistryInitialized(_) => {
                // Deployment parameters, all of which are either already known from the
                // configuration or irrelevant to a reader of the registry.
                trace!("ignoring RegistryInitialized");
            }
            HoprServiceRegistryEvents::TokensRecovered(_) => {
                // Administrative sweep of tokens sent to the registry by mistake; it changes no
                // entry and no service type.
                trace!("ignoring TokensRecovered");
            }
            HoprServiceRegistryEvents::DefaultAdminDelayChangeCanceled(_)
            | HoprServiceRegistryEvents::DefaultAdminDelayChangeScheduled(_)
            | HoprServiceRegistryEvents::DefaultAdminTransferCanceled(_)
            | HoprServiceRegistryEvents::DefaultAdminTransferScheduled(_)
            | HoprServiceRegistryEvents::RoleAdminChanged(_)
            | HoprServiceRegistryEvents::RoleGranted(_)
            | HoprServiceRegistryEvents::RoleRevoked(_) => {
                // Access-control bookkeeping of the registry admin and manager roles. The indexed
                // model exposes no permissions, so there is nothing to record.
                trace!("ignoring service registry access control event");
            }
        }

        Ok(vec![])
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use blokli_db::{BlokliDbGeneralModelOperations, db::BlokliDb, services::BlokliDbServiceOperations};
    use hopr_bindings::{
        exports::alloy::primitives::{Address as AlloyAddress, B256, Bytes, U256, aliases::U48},
        hopr_service_registry::HoprServiceRegistry::{
            Deregistered, NodeSafeRegistryUpdated, Registered, RegistryInitialized, RequirementUpdated,
            SelfRegistrationBurnUpdated, SelfUpdateBurnUpdated, ServiceTypeRegistered, TokensRecovered,
            TypeOwnershipTransferred, TypeRegistrationFeeUpdated, Updated,
        },
    };
    use hopr_types::primitive::prelude::SerializableLog;

    use super::*;
    use crate::{
        errors::CoreEthereumIndexerError,
        handlers::test_utils::test_helpers::{
            ClonableMockOperations, MockIndexerRpcOperations, SERVICE_REGISTRY_ADDR, event_to_log_at_block,
            init_handlers_with_events,
        },
        state::IndexerEvent,
    };

    /// Unix seconds used for every registration in these tests, so that the snapshots stay stable.
    const REGISTERED_AT: u64 = 1_700_000_000;

    fn handlers(db: BlokliDb) -> crate::handlers::ContractEventHandlers<ClonableMockOperations, BlokliDb> {
        let (handlers, ..) = init_handlers_with_events(
            ClonableMockOperations {
                inner: Arc::new(MockIndexerRpcOperations::new()),
            },
            db,
        );
        handlers
    }

    fn alloy_address(byte: u8) -> AlloyAddress {
        AlloyAddress::from([byte; 20])
    }

    fn gvpn_exit() -> B256 {
        B256::from(ServiceType::GVPN_EXIT.as_encoded())
    }

    /// Runs one log through the full dispatch path, which also proves the address of the registry
    /// is routed to this handler.
    async fn process(
        handlers: &crate::handlers::ContractEventHandlers<ClonableMockOperations, BlokliDb>,
        db: &BlokliDb,
        log: SerializableLog,
    ) -> Result<Vec<IndexerEvent>> {
        let handlers = handlers.clone();
        db.begin_transaction()
            .await?
            .perform(move |tx| Box::pin(async move { handlers.process_log_event(tx, log, true).await }))
            .await
    }

    fn registered_log(metadata: Vec<u8>, block: u64) -> SerializableLog {
        event_to_log_at_block(
            Registered {
                serviceType: gvpn_exit(),
                node: alloy_address(0x11),
                safe: alloy_address(0x22),
                metadata: Bytes::from(metadata),
                registeredAt: U48::from(REGISTERED_AT),
                burned: U256::from(1_000u64),
            },
            *SERVICE_REGISTRY_ADDR,
            block,
            0,
            0,
        )
    }

    fn service_type_registered_log(block: u64) -> SerializableLog {
        event_to_log_at_block(
            ServiceTypeRegistered {
                serviceType: gvpn_exit(),
                owner: alloy_address(0x33),
                feeBurned: U256::from(7u64),
            },
            *SERVICE_REGISTRY_ADDR,
            block,
            0,
            0,
        )
    }

    /// Extracts the single entry update an event batch must carry.
    fn entry_update(events: Vec<IndexerEvent>) -> ServiceUpdate {
        match events.into_iter().next() {
            Some(IndexerEvent::ServiceEntryUpdated(update)) => update,
            other => panic!("expected a ServiceEntryUpdated event, got {other:?}"),
        }
    }

    /// Extracts the single type update an event batch must carry.
    fn type_update(events: Vec<IndexerEvent>) -> ServiceTypeUpdate {
        match events.into_iter().next() {
            Some(IndexerEvent::ServiceTypeUpdated(update)) => update,
            other => panic!("expected a ServiceTypeUpdated event, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_registered_stores_entry_and_publishes_event() -> anyhow::Result<()> {
        let db = BlokliDb::new_in_memory().await?;
        let handlers = handlers(db.clone());

        let events = process(&handlers, &db, registered_log(b"{\"v\":1}".to_vec(), 100)).await?;
        insta::assert_yaml_snapshot!("registered_event", entry_update(events));

        let stored = db
            .get_service_entry(None, ServiceType::GVPN_EXIT, Address::from([0x11u8; 20]))
            .await?
            .expect("the entry must be indexed");
        insta::assert_yaml_snapshot!("registered_row", to_api_entry(&stored)?);

        Ok(())
    }

    /// An update leaves `registeredAt` untouched on-chain and the event does not carry it, so the
    /// indexed entry must keep the registration timestamp it already had.
    #[tokio::test]
    async fn test_updated_keeps_the_stored_registration_timestamp() -> anyhow::Result<()> {
        let db = BlokliDb::new_in_memory().await?;
        let handlers = handlers(db.clone());

        process(&handlers, &db, registered_log(b"{\"v\":1}".to_vec(), 100)).await?;

        let updated_log = event_to_log_at_block(
            Updated {
                serviceType: gvpn_exit(),
                node: alloy_address(0x11),
                safe: alloy_address(0x22),
                metadata: Bytes::from(b"{\"v\":2}".to_vec()),
                updatedAt: U48::from(REGISTERED_AT + 3_600),
                burned: U256::from(500u64),
            },
            *SERVICE_REGISTRY_ADDR,
            200,
            0,
            0,
        );

        let events = process(&handlers, &db, updated_log).await?;
        insta::assert_yaml_snapshot!("updated_event", entry_update(events));

        let stored = db
            .get_service_entry(None, ServiceType::GVPN_EXIT, Address::from([0x11u8; 20]))
            .await?
            .expect("the entry must still be indexed");
        insta::assert_yaml_snapshot!("updated_row", to_api_entry(&stored)?);

        Ok(())
    }

    #[tokio::test]
    async fn test_deregistered_removes_entry_and_publishes_event() -> anyhow::Result<()> {
        let db = BlokliDb::new_in_memory().await?;
        let handlers = handlers(db.clone());

        process(&handlers, &db, registered_log(b"{\"v\":1}".to_vec(), 100)).await?;

        let deregistered_log = event_to_log_at_block(
            Deregistered {
                serviceType: gvpn_exit(),
                node: alloy_address(0x11),
                safe: alloy_address(0x22),
            },
            *SERVICE_REGISTRY_ADDR,
            300,
            0,
            0,
        );

        let events = process(&handlers, &db, deregistered_log).await?;
        insta::assert_yaml_snapshot!("deregistered_event", entry_update(events));

        let stored = db
            .get_service_entry(None, ServiceType::GVPN_EXIT, Address::from([0x11u8; 20]))
            .await?;
        assert!(stored.is_none());

        Ok(())
    }

    /// A deregistration for an entry the indexer never saw is tolerated, the way the node-safe
    /// registry tolerates one for an unknown binding.
    #[tokio::test]
    async fn test_deregistering_an_unknown_entry_is_a_no_op() -> anyhow::Result<()> {
        let db = BlokliDb::new_in_memory().await?;
        let handlers = handlers(db.clone());

        let deregistered_log = event_to_log_at_block(
            Deregistered {
                serviceType: gvpn_exit(),
                node: alloy_address(0x11),
                safe: alloy_address(0x22),
            },
            *SERVICE_REGISTRY_ADDR,
            300,
            0,
            0,
        );

        let events = process(&handlers, &db, deregistered_log).await?;
        assert!(events.is_empty());

        Ok(())
    }

    #[tokio::test]
    async fn test_service_type_registered() -> anyhow::Result<()> {
        let db = BlokliDb::new_in_memory().await?;
        let handlers = handlers(db.clone());

        let events = process(&handlers, &db, service_type_registered_log(100)).await?;
        insta::assert_yaml_snapshot!("service_type_registered_event", type_update(events));

        let stored = db
            .get_service_type(None, ServiceType::GVPN_EXIT)
            .await?
            .expect("the service type must be indexed");
        insta::assert_yaml_snapshot!("service_type_registered_row", to_api_type_info(&stored));

        Ok(())
    }

    /// Transferring ownership to the zero address abandons the type, which the indexed state
    /// records as no owner at all.
    #[tokio::test]
    async fn test_type_ownership_transferred() -> anyhow::Result<()> {
        let db = BlokliDb::new_in_memory().await?;
        let handlers = handlers(db.clone());

        process(&handlers, &db, service_type_registered_log(100)).await?;

        let transferred = event_to_log_at_block(
            TypeOwnershipTransferred {
                serviceType: gvpn_exit(),
                oldOwner: alloy_address(0x33),
                newOwner: alloy_address(0x44),
            },
            *SERVICE_REGISTRY_ADDR,
            200,
            0,
            0,
        );
        let events = process(&handlers, &db, transferred).await?;
        insta::assert_yaml_snapshot!("owner_changed_event", type_update(events));

        let abandoned = event_to_log_at_block(
            TypeOwnershipTransferred {
                serviceType: gvpn_exit(),
                oldOwner: alloy_address(0x44),
                newOwner: AlloyAddress::ZERO,
            },
            *SERVICE_REGISTRY_ADDR,
            300,
            0,
            0,
        );
        let events = process(&handlers, &db, abandoned).await?;
        insta::assert_yaml_snapshot!("owner_abandoned_event", type_update(events));

        Ok(())
    }

    /// A requirement of zero reopens the type to any node.
    #[tokio::test]
    async fn test_requirement_updated() -> anyhow::Result<()> {
        let db = BlokliDb::new_in_memory().await?;
        let handlers = handlers(db.clone());

        process(&handlers, &db, service_type_registered_log(100)).await?;

        let gated = event_to_log_at_block(
            RequirementUpdated {
                serviceType: gvpn_exit(),
                requirement: alloy_address(0x55),
            },
            *SERVICE_REGISTRY_ADDR,
            200,
            0,
            0,
        );
        let events = process(&handlers, &db, gated).await?;
        insta::assert_yaml_snapshot!("requirement_changed_event", type_update(events));

        let reopened = event_to_log_at_block(
            RequirementUpdated {
                serviceType: gvpn_exit(),
                requirement: AlloyAddress::ZERO,
            },
            *SERVICE_REGISTRY_ADDR,
            300,
            0,
            0,
        );
        let events = process(&handlers, &db, reopened).await?;
        insta::assert_yaml_snapshot!("requirement_reopened_event", type_update(events));

        Ok(())
    }

    #[tokio::test]
    async fn test_self_registration_burn_updated() -> anyhow::Result<()> {
        let db = BlokliDb::new_in_memory().await?;
        let handlers = handlers(db.clone());

        process(&handlers, &db, service_type_registered_log(100)).await?;

        let log = event_to_log_at_block(
            SelfRegistrationBurnUpdated {
                serviceType: gvpn_exit(),
                amount: U256::from(1_500_000_000_000_000_000u64),
            },
            *SERVICE_REGISTRY_ADDR,
            200,
            0,
            0,
        );
        let events = process(&handlers, &db, log).await?;
        insta::assert_yaml_snapshot!("registration_burn_changed_event", type_update(events));

        Ok(())
    }

    #[tokio::test]
    async fn test_self_update_burn_updated() -> anyhow::Result<()> {
        let db = BlokliDb::new_in_memory().await?;
        let handlers = handlers(db.clone());

        process(&handlers, &db, service_type_registered_log(100)).await?;

        let log = event_to_log_at_block(
            SelfUpdateBurnUpdated {
                serviceType: gvpn_exit(),
                amount: U256::from(250_000_000_000_000_000u64),
            },
            *SERVICE_REGISTRY_ADDR,
            200,
            0,
            0,
        );
        let events = process(&handlers, &db, log).await?;
        insta::assert_yaml_snapshot!("update_burn_changed_event", type_update(events));

        Ok(())
    }

    #[tokio::test]
    async fn test_type_registration_fee_updated() -> anyhow::Result<()> {
        let db = BlokliDb::new_in_memory().await?;
        let handlers = handlers(db.clone());

        let log = event_to_log_at_block(
            TypeRegistrationFeeUpdated {
                amount: U256::from(10_000_000_000_000_000_000u128),
            },
            *SERVICE_REGISTRY_ADDR,
            100,
            0,
            0,
        );
        let events = process(&handlers, &db, log).await?;
        insta::assert_yaml_snapshot!("registration_fee_changed_event", type_update(events));

        Ok(())
    }

    #[tokio::test]
    async fn test_node_safe_registry_updated() -> anyhow::Result<()> {
        let db = BlokliDb::new_in_memory().await?;
        let handlers = handlers(db.clone());

        let log = event_to_log_at_block(
            NodeSafeRegistryUpdated {
                oldNodeSafeRegistry: AlloyAddress::ZERO,
                newNodeSafeRegistry: alloy_address(0x66),
            },
            *SERVICE_REGISTRY_ADDR,
            100,
            0,
            0,
        );
        let events = process(&handlers, &db, log).await?;
        insta::assert_yaml_snapshot!("registry_pointer_changed_event", type_update(events));

        Ok(())
    }

    /// Metadata above the contract's 2048-byte cap must fail the log, never be truncated into a
    /// shorter entry that no longer matches the chain.
    #[tokio::test]
    async fn test_oversized_metadata_is_rejected() -> anyhow::Result<()> {
        let db = BlokliDb::new_in_memory().await?;
        let handlers = handlers(db.clone());

        let result = process(&handlers, &db, registered_log(vec![0xabu8; 2049], 100)).await;
        assert!(matches!(result, Err(CoreEthereumIndexerError::GeneralError(_))));

        Ok(())
    }

    /// The contract cannot emit a zero service type, so a log carrying one was not decoded from
    /// the registry and must not reach the database.
    #[tokio::test]
    async fn test_zero_service_type_is_rejected() -> anyhow::Result<()> {
        let db = BlokliDb::new_in_memory().await?;
        let handlers = handlers(db.clone());

        let log = event_to_log_at_block(
            Deregistered {
                serviceType: B256::ZERO,
                node: alloy_address(0x11),
                safe: alloy_address(0x22),
            },
            *SERVICE_REGISTRY_ADDR,
            100,
            0,
            0,
        );

        let result = process(&handlers, &db, log).await;
        assert!(matches!(result, Err(CoreEthereumIndexerError::GeneralError(_))));

        Ok(())
    }

    /// The two events the RPC filter leaves out still decode when they arrive from a wider filter,
    /// and must change nothing.
    #[tokio::test]
    async fn test_ignored_events_change_nothing() -> anyhow::Result<()> {
        let db = BlokliDb::new_in_memory().await?;
        let handlers = handlers(db.clone());

        let initialized = event_to_log_at_block(
            RegistryInitialized {
                version: U256::from(1u64),
                admin: alloy_address(0x77),
                manager: alloy_address(0x78),
                wxHopr: alloy_address(0x79),
                initialAdminDelay: U48::from(3_600u64),
            },
            *SERVICE_REGISTRY_ADDR,
            100,
            0,
            0,
        );
        assert!(process(&handlers, &db, initialized).await?.is_empty());

        let recovered = event_to_log_at_block(
            TokensRecovered {
                token: alloy_address(0x7a),
                to: alloy_address(0x7b),
                amount: U256::from(42u64),
            },
            *SERVICE_REGISTRY_ADDR,
            101,
            0,
            0,
        );
        assert!(process(&handlers, &db, recovered).await?.is_empty());

        assert!(db.get_service_types(None).await?.is_empty());
        insta::assert_yaml_snapshot!(
            "registry_config_after_ignored_events",
            to_api_registry_config(&db.get_service_registry_config(None).await?)
        );

        Ok(())
    }
}
