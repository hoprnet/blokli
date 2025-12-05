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
