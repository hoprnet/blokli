use blokli_chain_rpc::Log;
use blokli_chain_types::AlloyAddressExt;
use blokli_db::{BlokliDbAllOperations, OpenTransaction, safe_history::SafeActivityKind};
use hopr_bindings::exports::alloy::{
    primitives::{Address as AlloyAddress, B256, Log as AlloyLog},
    sol_types::SolEventInterface,
};
use hopr_types::{
    crypto::types::Hash,
    primitive::prelude::{Address, SerializableLog},
};
use tracing::{debug, info, warn};

use super::ContractEventHandlers;
use crate::{custom_abis::safe_contract_events::SafeContract::SafeContractEvents, errors::Result, state::IndexerEvent};

impl<T, Db> ContractEventHandlers<T, Db>
where
    T: blokli_chain_rpc::HoprIndexerRpcOperations + Clone + Send + 'static,
    Db: BlokliDbAllOperations + Clone,
{
    pub(super) async fn backfill_safe_logs_in_discovery_block(
        &self,
        tx: &OpenTransaction,
        safe_address: Address,
        block: u64,
    ) -> Result<()> {
        let safe_logs = self
            ._rpc_operations
            .get_logs_for_address(safe_address, crate::constants::topics::safe_contract(), block, block)
            .await?;

        if safe_logs.is_empty() {
            return Ok(());
        }

        let serialized_logs = safe_logs.iter().cloned().map(SerializableLog::from).collect::<Vec<_>>();

        let store_results = self.db.store_logs(serialized_logs.clone()).await?;
        if let Some(error) = store_results.into_iter().find_map(|result| result.err()) {
            return Err(crate::errors::CoreEthereumIndexerError::ProcessError(format!(
                "failed to store Safe discovery block logs: {error}"
            )));
        }

        for (log, slog) in safe_logs.into_iter().zip(serialized_logs.into_iter()) {
            let primitive_log = AlloyLog::new(
                AlloyAddress::from_hopr_address(log.address),
                log.topics.iter().map(|hash| B256::from_slice(hash.as_ref())).collect(),
                log.data.clone().into(),
            )
            .ok_or_else(|| {
                crate::errors::CoreEthereumIndexerError::ProcessError(format!(
                    "failed to convert fetched Safe log to primitive log: {slog:?}"
                ))
            })?;

            let event = SafeContractEvents::decode_log(&primitive_log)?;
            debug!(
                safe_address = %safe_address,
                block = log.block_number,
                tx_index = log.tx_index,
                log_index = %log.log_index,
                "Backfilling Safe discovery-block log"
            );
            self.on_safe_contract_event(tx, safe_address, &log, event.data, false)
                .await?;
            self.db.set_log_processed(slog).await.map_err(|e| {
                crate::errors::CoreEthereumIndexerError::ProcessError(format!(
                    "failed to mark Safe discovery block log as processed: {e}"
                ))
            })?;
        }

        Ok(())
    }

    pub(super) async fn on_safe_contract_event(
        &self,
        tx: &OpenTransaction,
        safe_address: hopr_types::primitive::prelude::Address,
        log: &Log,
        event: SafeContractEvents,
        _is_synced: bool,
    ) -> Result<Vec<IndexerEvent>> {
        let chain_tx_hash = Hash::from(log.tx_hash);

        match event {
            SafeContractEvents::SafeSetup(safe_setup) => {
                let owner_count = safe_setup.owners.len();
                self.db
                    .record_safe_activity(
                        Some(tx),
                        safe_address,
                        SafeActivityKind::SafeSetup,
                        chain_tx_hash,
                        None,
                        None,
                        Some(safe_setup.threshold.to_string()),
                        None,
                        Some(safe_setup.initiator.to_hopr_address()),
                        log.block_number,
                        log.tx_index,
                        log.log_index.as_u64(),
                    )
                    .await?;

                for owner in safe_setup.owners {
                    let owner = owner.to_hopr_address();
                    self.db
                        .upsert_safe_owner_state(
                            Some(tx),
                            safe_address,
                            owner,
                            true,
                            log.block_number,
                            log.tx_index,
                            log.log_index.as_u64(),
                        )
                        .await?;
                }

                info!(
                    safe_address = %safe_address,
                    owner_count,
                    threshold = %safe_setup.threshold.to_string(),
                    block = log.block_number,
                    tx_index = log.tx_index,
                    log_index = %log.log_index,
                    "Persisted Safe SafeSetup event"
                );
            }
            SafeContractEvents::AddedOwner(added_owner) => {
                let owner = added_owner.owner.to_hopr_address();
                self.db
                    .record_safe_activity(
                        Some(tx),
                        safe_address,
                        SafeActivityKind::AddedOwner,
                        chain_tx_hash,
                        None,
                        Some(owner),
                        None,
                        None,
                        None,
                        log.block_number,
                        log.tx_index,
                        log.log_index.as_u64(),
                    )
                    .await?;
                self.db
                    .upsert_safe_owner_state(
                        Some(tx),
                        safe_address,
                        owner,
                        true,
                        log.block_number,
                        log.tx_index,
                        log.log_index.as_u64(),
                    )
                    .await?;
                info!(
                    safe_address = %safe_address,
                    owner = %owner,
                    block = log.block_number,
                    tx_index = log.tx_index,
                    log_index = %log.log_index,
                    "Persisted Safe AddedOwner event"
                );
            }
            SafeContractEvents::RemovedOwner(removed_owner) => {
                let owner = removed_owner.owner.to_hopr_address();
                self.db
                    .record_safe_activity(
                        Some(tx),
                        safe_address,
                        SafeActivityKind::RemovedOwner,
                        chain_tx_hash,
                        None,
                        Some(owner),
                        None,
                        None,
                        None,
                        log.block_number,
                        log.tx_index,
                        log.log_index.as_u64(),
                    )
                    .await?;
                self.db
                    .upsert_safe_owner_state(
                        Some(tx),
                        safe_address,
                        owner,
                        false,
                        log.block_number,
                        log.tx_index,
                        log.log_index.as_u64(),
                    )
                    .await?;
                info!(
                    safe_address = %safe_address,
                    owner = %owner,
                    block = log.block_number,
                    tx_index = log.tx_index,
                    log_index = %log.log_index,
                    "Persisted Safe RemovedOwner event"
                );
            }
            SafeContractEvents::ChangedThreshold(changed_threshold) => {
                self.db
                    .record_safe_activity(
                        Some(tx),
                        safe_address,
                        SafeActivityKind::ChangedThreshold,
                        chain_tx_hash,
                        None,
                        None,
                        Some(changed_threshold.threshold.to_string()),
                        None,
                        None,
                        log.block_number,
                        log.tx_index,
                        log.log_index.as_u64(),
                    )
                    .await?;
                info!(
                    safe_address = %safe_address,
                    threshold = %changed_threshold.threshold.to_string(),
                    block = log.block_number,
                    tx_index = log.tx_index,
                    log_index = %log.log_index,
                    "Persisted Safe ChangedThreshold event"
                );
            }
            SafeContractEvents::ExecutionSuccess(execution) => {
                self.db
                    .record_safe_activity(
                        Some(tx),
                        safe_address,
                        SafeActivityKind::ExecutionSuccess,
                        chain_tx_hash,
                        Some(Hash::from(execution.txHash.0)),
                        None,
                        None,
                        Some(execution.payment.to_string()),
                        None,
                        log.block_number,
                        log.tx_index,
                        log.log_index.as_u64(),
                    )
                    .await?;
                info!(
                    safe_address = %safe_address,
                    safe_tx_hash = %Hash::from(execution.txHash.0),
                    payment = %execution.payment.to_string(),
                    block = log.block_number,
                    tx_index = log.tx_index,
                    log_index = %log.log_index,
                    "Persisted Safe ExecutionSuccess event"
                );
            }
            SafeContractEvents::ExecutionFailure(execution) => {
                self.db
                    .record_safe_activity(
                        Some(tx),
                        safe_address,
                        SafeActivityKind::ExecutionFailure,
                        chain_tx_hash,
                        Some(Hash::from(execution.txHash.0)),
                        None,
                        None,
                        Some(execution.payment.to_string()),
                        None,
                        log.block_number,
                        log.tx_index,
                        log.log_index.as_u64(),
                    )
                    .await?;
                warn!(
                    safe_address = %safe_address,
                    safe_tx_hash = %Hash::from(execution.txHash.0),
                    payment = %execution.payment.to_string(),
                    block = log.block_number,
                    tx_index = log.tx_index,
                    log_index = %log.log_index,
                    "Persisted Safe ExecutionFailure event"
                );
            }
        }

        Ok(Vec::new())
    }
}
