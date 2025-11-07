use blokli_chain_rpc::HoprIndexerRpcOperations;
use blokli_db::{BlokliDbAllOperations, OpenTransaction, api::info::DomainSeparator};
use hopr_bindings::hopr_node_safe_registry::HoprNodeSafeRegistry::HoprNodeSafeRegistryEvents;
use tracing::info;

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
    pub(super) async fn on_node_safe_registry_event(
        &self,
        tx: &OpenTransaction,
        event: HoprNodeSafeRegistryEvents,
        _is_synced: bool,
    ) -> Result<()> {
        #[cfg(all(feature = "prometheus", not(test)))]
        METRIC_INDEXER_LOG_COUNTERS.increment(&["safe_registry"]);

        match event {
            HoprNodeSafeRegistryEvents::RegisteredNodeSafe(registered) => {
                info!(node_address = %registered.nodeAddress, safe_address = %registered.safeAddress, "Node safe registered", );
            }
            HoprNodeSafeRegistryEvents::DeregisteredNodeSafe(deregistered) => {
                info!(node_address = %deregistered.nodeAddress, safe_address = %deregistered.safeAddress, "Node safe deregistered", );
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
    use blokli_db::{BlokliDbGeneralModelOperations, db::BlokliDb};
    use hopr_primitive_types::prelude::SerializableLog;
    use primitive_types::H256;

    use crate::handlers::test_utils::test_helpers::*;

    #[tokio::test]
    async fn test_on_node_safe_registry_registered() -> anyhow::Result<()> {
        let db = BlokliDb::new_in_memory().await?;
        let rpc_operations = MockIndexerRpcOperations::new();
        // ==> set mock expectations here
        let clonable_rpc_operations = ClonableMockOperations {
            //
            inner: Arc::new(rpc_operations),
        };
        let handlers = init_handlers(clonable_rpc_operations, db.clone());

        let encoded_data = ().abi_encode();

        let safe_registered_log = SerializableLog {
            address: handlers.addresses.safe_registry,
            topics: vec![
                hopr_bindings::hopr_node_safe_registry::HoprNodeSafeRegistry::RegisteredNodeSafe::SIGNATURE_HASH.into(),
                // RegisteredNodeSafeFilter::signature().into(),
                H256::from_slice(&SAFE_INSTANCE_ADDR.to_bytes32()).into(),
                H256::from_slice(&SELF_CHAIN_ADDRESS.to_bytes32()).into(),
            ],
            data: encoded_data,
            ..test_log()
        };

        db.begin_transaction()
            .await?
            .perform(|tx| Box::pin(async move { handlers.process_log_event(tx, safe_registered_log, true).await }))
            .await?;

        // TODO: Add event verification - check published IndexerEvent instead of return value

        // Nothing to check in the DB here, since we do not track this
        Ok(())
    }

    #[tokio::test]
    async fn test_on_node_safe_registry_deregistered() -> anyhow::Result<()> {
        let db = BlokliDb::new_in_memory().await?;
        let rpc_operations = MockIndexerRpcOperations::new();
        // ==> set mock expectations here
        let clonable_rpc_operations = ClonableMockOperations {
            //
            inner: Arc::new(rpc_operations),
        };
        let handlers = init_handlers(clonable_rpc_operations, db.clone());

        // Nothing to write to the DB here, since we do not track this

        let encoded_data = ().abi_encode();

        let safe_registered_log = SerializableLog {
            address: handlers.addresses.safe_registry,
            topics: vec![
                hopr_bindings::hopr_node_safe_registry::HoprNodeSafeRegistry::DeregisteredNodeSafe::SIGNATURE_HASH
                    .into(),
                // DeregisteredNodeSafeFilter::signature().into(),
                H256::from_slice(&SAFE_INSTANCE_ADDR.to_bytes32()).into(),
                H256::from_slice(&SELF_CHAIN_ADDRESS.to_bytes32()).into(),
            ],
            data: encoded_data,
            ..test_log()
        };

        db.begin_transaction()
            .await?
            .perform(|tx| Box::pin(async move { handlers.process_log_event(tx, safe_registered_log, true).await }))
            .await?;

        // Nothing to check in the DB here, since we do not track this
        Ok(())
    }
}
