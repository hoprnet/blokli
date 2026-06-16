use blokli_chain_rpc::Log;
use blokli_chain_types::AlloyAddressExt;
use blokli_db::{
    BlokliDbAllOperations, OpenTransaction,
    safe_history::{StagedSafeMutation, StagedSafeMutationKind},
};
use hopr_bindings::exports::alloy::{
    primitives::{Address as AlloyAddress, B256, Log as AlloyLog},
    sol_types::SolEventInterface,
};
use hopr_types::{
    crypto::types::Hash,
    primitive::prelude::{Address, SerializableLog},
};
use tracing::{debug, info, warn};

use super::{ContractEventHandlers, PendingSafeMutation};
use crate::{custom_abis::safe_contract_events::SafeContract::SafeContractEvents, errors::Result, state::IndexerEvent};

impl<T, Db> ContractEventHandlers<T, Db>
where
    T: blokli_chain_rpc::HoprIndexerRpcOperations + Clone + Send + 'static,
    Db: BlokliDbAllOperations + Clone,
{
    fn pending_safe_mutation_key(tx: &OpenTransaction) -> usize {
        tx.as_ref() as *const _ as usize
    }

    async fn append_pending_safe_mutations_for_tx(
        &self,
        tx: &OpenTransaction,
        staged_mutations: Vec<PendingSafeMutation>,
    ) {
        if staged_mutations.is_empty() {
            return;
        }

        let mut pending_safe_mutations = self.pending_safe_mutations.lock().await;
        pending_safe_mutations
            .entry(Self::pending_safe_mutation_key(tx))
            .or_default()
            .extend(staged_mutations);
    }

    pub(super) async fn merge_pending_safe_mutations_for_tx_key(&self, from_key: usize, tx: &OpenTransaction) {
        let mut pending_safe_mutations = self.pending_safe_mutations.lock().await;
        let from_mutations = pending_safe_mutations.remove(&from_key).unwrap_or_default();
        if from_mutations.is_empty() {
            return;
        }

        pending_safe_mutations
            .entry(Self::pending_safe_mutation_key(tx))
            .or_default()
            .extend(from_mutations);
    }

    pub(super) async fn clear_pending_safe_mutations_for_tx(&self, tx: &OpenTransaction) {
        self.pending_safe_mutations
            .lock()
            .await
            .remove(&Self::pending_safe_mutation_key(tx));
    }

    pub(super) async fn discard_pending_safe_mutations_for_tx(&self, tx: &OpenTransaction) {
        self.clear_pending_safe_mutations_for_tx(tx).await;
    }

    pub(super) async fn flush_pending_safe_mutations_for_tx(&self, tx: &OpenTransaction) -> Result<()> {
        let pending_mutations = self
            .pending_safe_mutations
            .lock()
            .await
            .remove(&Self::pending_safe_mutation_key(tx))
            .unwrap_or_default();
        self.db
            .persist_staged_safe_mutations(Some(tx), pending_mutations)
            .await?;
        Ok(())
    }

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

        for (log, slog) in safe_logs.iter().zip(serialized_logs.iter()) {
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
            self.on_safe_contract_event(tx, safe_address, log, event.data, false)
                .await?;
        }

        self.flush_pending_safe_mutations_for_tx(tx).await?;

        self.db
            .set_logs_processed_explicit(serialized_logs)
            .await
            .map_err(|e| {
                crate::errors::CoreEthereumIndexerError::ProcessError(format!(
                    "failed to mark Safe discovery block logs as processed: {e}"
                ))
            })?;

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
        let mut staged_mutations = Vec::new();

        match event {
            SafeContractEvents::SafeSetup(safe_setup) => {
                let owner_count = safe_setup.owners.len();
                let owners = safe_setup
                    .owners
                    .iter()
                    .map(|owner| owner.to_hopr_address())
                    .collect::<Vec<_>>();
                staged_mutations.push(StagedSafeMutation {
                    safe_address,
                    chain_tx_hash,
                    block: log.block_number,
                    tx_index: log.tx_index,
                    log_index: log.log_index.as_u64(),
                    event: StagedSafeMutationKind::SafeSetup {
                        owners,
                        threshold: safe_setup.threshold.to_string(),
                        initiator_address: Some(safe_setup.initiator.to_hopr_address()),
                    },
                });

                info!(
                    safe_address = %safe_address,
                    owner_count,
                    threshold = %safe_setup.threshold.to_string(),
                    block = log.block_number,
                    tx_index = log.tx_index,
                    log_index = %log.log_index,
                    "Staged Safe SafeSetup event"
                );
            }
            SafeContractEvents::AddedOwner(added_owner) => {
                let owner = added_owner.owner.to_hopr_address();
                staged_mutations.push(StagedSafeMutation {
                    safe_address,
                    chain_tx_hash,
                    block: log.block_number,
                    tx_index: log.tx_index,
                    log_index: log.log_index.as_u64(),
                    event: StagedSafeMutationKind::AddedOwner { owner_address: owner },
                });
                info!(
                    safe_address = %safe_address,
                    owner = %owner,
                    block = log.block_number,
                    tx_index = log.tx_index,
                    log_index = %log.log_index,
                    "Staged Safe AddedOwner event"
                );
            }
            SafeContractEvents::RemovedOwner(removed_owner) => {
                let owner = removed_owner.owner.to_hopr_address();
                staged_mutations.push(StagedSafeMutation {
                    safe_address,
                    chain_tx_hash,
                    block: log.block_number,
                    tx_index: log.tx_index,
                    log_index: log.log_index.as_u64(),
                    event: StagedSafeMutationKind::RemovedOwner { owner_address: owner },
                });
                info!(
                    safe_address = %safe_address,
                    owner = %owner,
                    block = log.block_number,
                    tx_index = log.tx_index,
                    log_index = %log.log_index,
                    "Staged Safe RemovedOwner event"
                );
            }
            SafeContractEvents::ChangedThreshold(changed_threshold) => {
                staged_mutations.push(StagedSafeMutation {
                    safe_address,
                    chain_tx_hash,
                    block: log.block_number,
                    tx_index: log.tx_index,
                    log_index: log.log_index.as_u64(),
                    event: StagedSafeMutationKind::ChangedThreshold {
                        threshold: changed_threshold.threshold.to_string(),
                    },
                });
                info!(
                    safe_address = %safe_address,
                    threshold = %changed_threshold.threshold.to_string(),
                    block = log.block_number,
                    tx_index = log.tx_index,
                    log_index = %log.log_index,
                    "Staged Safe ChangedThreshold event"
                );
            }
            SafeContractEvents::ExecutionSuccess(execution) => {
                staged_mutations.push(StagedSafeMutation {
                    safe_address,
                    chain_tx_hash,
                    block: log.block_number,
                    tx_index: log.tx_index,
                    log_index: log.log_index.as_u64(),
                    event: StagedSafeMutationKind::ExecutionSuccess {
                        safe_tx_hash: Hash::from(execution.txHash.0),
                        payment: execution.payment.to_string(),
                    },
                });
                info!(
                    safe_address = %safe_address,
                    safe_tx_hash = %Hash::from(execution.txHash.0),
                    payment = %execution.payment.to_string(),
                    block = log.block_number,
                    tx_index = log.tx_index,
                    log_index = %log.log_index,
                    "Staged Safe ExecutionSuccess event"
                );
            }
            SafeContractEvents::ExecutionFailure(execution) => {
                staged_mutations.push(StagedSafeMutation {
                    safe_address,
                    chain_tx_hash,
                    block: log.block_number,
                    tx_index: log.tx_index,
                    log_index: log.log_index.as_u64(),
                    event: StagedSafeMutationKind::ExecutionFailure {
                        safe_tx_hash: Hash::from(execution.txHash.0),
                        payment: execution.payment.to_string(),
                    },
                });
                warn!(
                    safe_address = %safe_address,
                    safe_tx_hash = %Hash::from(execution.txHash.0),
                    payment = %execution.payment.to_string(),
                    block = log.block_number,
                    tx_index = log.tx_index,
                    log_index = %log.log_index,
                    "Staged Safe ExecutionFailure event"
                );
            }
        }

        self.append_pending_safe_mutations_for_tx(tx, staged_mutations).await;

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
        let db_check = db.clone();

        db.begin_transaction()
            .await?
            .perform(|tx| {
                Box::pin(async move {
                    handlers
                        .on_safe_contract_event(tx, safe_address, &log, event, true)
                        .await?;
                    assert!(db_check.get_safe_activity(Some(tx), safe_address).await?.is_empty());
                    handlers.flush_pending_safe_mutations_for_tx(tx).await?;
                    Ok::<Vec<IndexerEvent>, crate::errors::CoreEthereumIndexerError>(Vec::new())
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
                        .await?;
                    handlers.flush_pending_safe_mutations_for_tx(tx).await?;
                    Ok::<Vec<IndexerEvent>, crate::errors::CoreEthereumIndexerError>(Vec::new())
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

    #[tokio::test]
    async fn test_discard_pending_safe_mutations_drops_staged_safe_writes() -> anyhow::Result<()> {
        let db = BlokliDb::new_in_memory().await?;
        let handlers = init_handlers(
            ClonableMockOperations {
                inner: Arc::new(MockIndexerRpcOperations::new()),
            },
            db.clone(),
        );

        let safe_address = address_with_byte(21);
        let module_address = address_with_byte(22);
        let chain_key = address_with_byte(23);
        let owner = address_with_byte(24);

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
                    handlers.discard_pending_safe_mutations_for_tx(tx).await;
                    handlers.flush_pending_safe_mutations_for_tx(tx).await?;
                    Ok::<Vec<IndexerEvent>, crate::errors::CoreEthereumIndexerError>(Vec::new())
                })
            })
            .await?;

        assert!(db.get_safe_owners(None, safe_address).await?.is_empty());
        assert!(db.get_safe_activity(None, safe_address).await?.is_empty());

        Ok(())
    }
}
