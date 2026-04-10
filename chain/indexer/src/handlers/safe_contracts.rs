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

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use blokli_chain_rpc::Log;
    use blokli_db::{
        BlokliDbGeneralModelOperations, TargetDb, db::BlokliDb, safe_contracts::BlokliDbSafeContractOperations,
        safe_history::BlokliDbSafeHistoryOperations,
    };
    use blokli_db_entity::hopr_safe_contract::{Column as SafeContractColumn, Entity as SafeContractEntity};
    use hopr_bindings::exports::alloy::primitives::U256 as AlloyU256;
    use primitive_types::U256 as PrimitiveU256;
    use sea_orm::{ColumnTrait, EntityTrait, PaginatorTrait, QueryFilter};

    use super::*;
    use crate::{
        custom_abis::safe_contract_events::SafeContract,
        handlers::test_utils::test_helpers::{ClonableMockOperations, MockIndexerRpcOperations, init_handlers},
    };

    fn address_with_byte(byte: u8) -> Address {
        Address::from([byte; 20])
    }

    fn hash_with_byte(byte: u8) -> Hash {
        Hash::from([byte; 32])
    }

    fn test_rpc_log(safe_address: Address, block_number: u64, tx_index: u64, log_index: u64) -> Log {
        Log {
            address: safe_address,
            topics: Vec::new(),
            data: Vec::new().into_boxed_slice(),
            tx_index,
            block_number,
            block_hash: hash_with_byte((block_number as u8).saturating_add(1)),
            tx_hash: hash_with_byte((block_number as u8).saturating_add(2)),
            log_index: PrimitiveU256::from(log_index),
            removed: false,
        }
    }

    #[tokio::test]
    async fn test_on_safe_contract_event_safe_setup_persists_owners_and_threshold() -> anyhow::Result<()> {
        let db = BlokliDb::new_in_memory().await?;
        let handlers = init_handlers(
            ClonableMockOperations {
                inner: Arc::new(MockIndexerRpcOperations::new()),
            },
            db.clone(),
        );

        let safe_address = address_with_byte(1);
        let module_address = address_with_byte(2);
        let chain_key = address_with_byte(3);
        let initiator = address_with_byte(4);
        let owner_one = address_with_byte(5);
        let owner_two = address_with_byte(6);

        db.create_safe_contract(None, safe_address, module_address, chain_key, 100, 0, 0)
            .await?;

        let event = SafeContractEvents::SafeSetup(SafeContract::SafeSetup {
            initiator: AlloyAddress::from_hopr_address(initiator),
            owners: vec![
                AlloyAddress::from_hopr_address(owner_one),
                AlloyAddress::from_hopr_address(owner_two),
            ],
            threshold: AlloyU256::from(2_u64),
            initializer: AlloyAddress::ZERO,
            fallbackHandler: AlloyAddress::ZERO,
        });
        let log = test_rpc_log(safe_address, 101, 4, 7);

        db.begin_transaction()
            .await?
            .perform(|tx| {
                Box::pin(async move {
                    handlers
                        .on_safe_contract_event(tx, safe_address, &log, event, true)
                        .await
                })
            })
            .await?;

        let mut owners = db.get_safe_owners(None, safe_address).await?;
        owners.sort_unstable();
        let mut expected = vec![owner_one, owner_two];
        expected.sort_unstable();
        assert_eq!(owners, expected);

        let activity = db.get_safe_activity(None, safe_address).await?;
        assert_eq!(activity.len(), 1);
        assert_eq!(activity[0].event_kind, "SAFE_SETUP");
        assert_eq!(activity[0].threshold.as_deref(), Some("2"));
        assert_eq!(activity[0].initiator_address, Some(initiator.as_ref().to_vec()));

        Ok(())
    }

    #[tokio::test]
    async fn test_on_safe_contract_event_tracks_owner_updates_threshold_and_execution() -> anyhow::Result<()> {
        let db = BlokliDb::new_in_memory().await?;
        let handlers = init_handlers(
            ClonableMockOperations {
                inner: Arc::new(MockIndexerRpcOperations::new()),
            },
            db.clone(),
        );

        let safe_address = address_with_byte(11);
        let module_address = address_with_byte(12);
        let chain_key = address_with_byte(13);
        let owner = address_with_byte(14);
        let safe_tx_hash = hash_with_byte(15);

        db.create_safe_contract(None, safe_address, module_address, chain_key, 100, 0, 0)
            .await?;

        db.begin_transaction()
            .await?
            .perform(|tx| {
                Box::pin(async move {
                    handlers
                        .on_safe_contract_event(
                            tx,
                            safe_address,
                            &test_rpc_log(safe_address, 101, 0, 0),
                            SafeContractEvents::AddedOwner(SafeContract::AddedOwner {
                                owner: AlloyAddress::from_hopr_address(owner),
                            }),
                            true,
                        )
                        .await?;
                    handlers
                        .on_safe_contract_event(
                            tx,
                            safe_address,
                            &test_rpc_log(safe_address, 102, 0, 0),
                            SafeContractEvents::ChangedThreshold(SafeContract::ChangedThreshold {
                                threshold: AlloyU256::from(3_u64),
                            }),
                            true,
                        )
                        .await?;
                    handlers
                        .on_safe_contract_event(
                            tx,
                            safe_address,
                            &test_rpc_log(safe_address, 103, 0, 0),
                            SafeContractEvents::ExecutionSuccess(SafeContract::ExecutionSuccess {
                                txHash: B256::from_slice(safe_tx_hash.as_ref()),
                                payment: AlloyU256::from(12_u64),
                            }),
                            true,
                        )
                        .await?;
                    handlers
                        .on_safe_contract_event(
                            tx,
                            safe_address,
                            &test_rpc_log(safe_address, 104, 0, 0),
                            SafeContractEvents::RemovedOwner(SafeContract::RemovedOwner {
                                owner: AlloyAddress::from_hopr_address(owner),
                            }),
                            true,
                        )
                        .await
                })
            })
            .await?;

        let owners = db.get_safe_owners(None, safe_address).await?;
        assert!(owners.is_empty(), "removed owner should no longer be current");

        let activity = db.get_safe_activity(None, safe_address).await?;
        assert_eq!(activity.len(), 4);
        assert_eq!(activity[0].event_kind, "ADDED_OWNER");
        assert_eq!(activity[0].owner_address, Some(owner.as_ref().to_vec()));
        assert_eq!(activity[1].event_kind, "CHANGED_THRESHOLD");
        assert_eq!(activity[1].threshold.as_deref(), Some("3"));
        assert_eq!(activity[2].event_kind, "EXECUTION_SUCCESS");
        assert_eq!(activity[2].safe_tx_hash, Some(safe_tx_hash.as_ref().to_vec()));
        assert_eq!(activity[2].payment.as_deref(), Some("12"));
        assert_eq!(activity[3].event_kind, "REMOVED_OWNER");

        let safe_count = SafeContractEntity::find()
            .filter(SafeContractColumn::Address.eq(safe_address.as_ref().to_vec()))
            .count(db.conn(TargetDb::Index))
            .await?;
        assert_eq!(safe_count, 1, "event handling should not duplicate the safe identity");

        Ok(())
    }
}
