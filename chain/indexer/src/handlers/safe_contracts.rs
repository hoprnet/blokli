use blokli_chain_rpc::Log;
use blokli_chain_types::AlloyAddressExt;
use blokli_db::{BlokliDbAllOperations, OpenTransaction};
use hopr_types::primitive::prelude::Address;
use tracing::{debug, info};

use super::{ContractEventHandlers, u256_to_u64};
use crate::{custom_abis::safe_contract_events::SafeContract::SafeContractEvents, errors::Result, state::IndexerEvent};

impl<T, Db> ContractEventHandlers<T, Db>
where
    T: blokli_chain_rpc::HoprIndexerRpcOperations + Clone + Send + 'static,
    Db: BlokliDbAllOperations + Clone,
{
    /// Reads owners and threshold of a Safe discovered while live, pinned at the discovery block.
    ///
    /// Live owner and threshold events at or below `block` are covered by this snapshot and
    /// ignored from then on.
    pub(super) async fn snapshot_safe_owners_at_discovery(
        &self,
        tx: &OpenTransaction,
        safe_address: Address,
        block: u64,
    ) -> Result<()> {
        let snapshot = self
            ._rpc_operations
            .get_safe_owners_and_threshold(safe_address, block)
            .await?;
        self.db
            .apply_safe_owner_snapshot(
                Some(tx),
                safe_address,
                snapshot.owners,
                snapshot.threshold.to_string(),
                block,
            )
            .await?;
        Ok(())
    }

    /// Whether the owner snapshot of the Safe already covers an event in `block`.
    async fn owner_snapshot_covers(&self, tx: &OpenTransaction, safe_address: Address, block: u64) -> Result<bool> {
        Ok(self
            .db
            .get_safe_owners_synced_block(Some(tx), safe_address)
            .await?
            .is_some_and(|synced_block| block <= synced_block))
    }

    pub(super) async fn on_safe_contract_event(
        &self,
        tx: &OpenTransaction,
        safe_address: Address,
        log: &Log,
        event: SafeContractEvents,
        _is_synced: bool,
    ) -> Result<Vec<IndexerEvent>> {
        let log_index = u256_to_u64(log.log_index, "log_index")?;

        // Owner and threshold events are only applied above the snapshot block of the Safe.
        let owner_events = matches!(
            event,
            SafeContractEvents::SafeSetup(_)
                | SafeContractEvents::AddedOwner(_)
                | SafeContractEvents::RemovedOwner(_)
                | SafeContractEvents::ChangedThreshold(_)
        );
        if owner_events && self.owner_snapshot_covers(tx, safe_address, log.block_number).await? {
            debug!(
                safe_address = %safe_address,
                block = log.block_number,
                "Ignoring Safe event covered by the owner snapshot"
            );
            return Ok(Vec::new());
        }

        match event {
            SafeContractEvents::SafeSetup(safe_setup) => {
                let owner_count = safe_setup.owners.len();
                for owner in &safe_setup.owners {
                    self.db
                        .upsert_safe_owner_state(
                            Some(tx),
                            safe_address,
                            owner.to_hopr_address(),
                            true,
                            log.block_number,
                            log.tx_index,
                            log_index,
                        )
                        .await?;
                }
                self.db
                    .upsert_safe_threshold_state(
                        Some(tx),
                        safe_address,
                        safe_setup.threshold.to_string(),
                        log.block_number,
                        log.tx_index,
                        log_index,
                    )
                    .await?;

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
                    .upsert_safe_owner_state(
                        Some(tx),
                        safe_address,
                        owner,
                        true,
                        log.block_number,
                        log.tx_index,
                        log_index,
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
                    .upsert_safe_owner_state(
                        Some(tx),
                        safe_address,
                        owner,
                        false,
                        log.block_number,
                        log.tx_index,
                        log_index,
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
                    .upsert_safe_threshold_state(
                        Some(tx),
                        safe_address,
                        changed_threshold.threshold.to_string(),
                        log.block_number,
                        log.tx_index,
                        log_index,
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
            // Not indexed: the Safe execution history has no reader and the topics are not requested.
            SafeContractEvents::ExecutionSuccess(_)
            | SafeContractEvents::ExecutionFailure(_)
            | SafeContractEvents::ExecutionFromModuleSuccess(_)
            | SafeContractEvents::ExecutionFromModuleFailure(_) => {}
        }

        Ok(Vec::new())
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use blokli_chain_rpc::{Log, SafeOwnersSnapshot};
    use blokli_db::{
        BlokliDbGeneralModelOperations, TargetDb, db::BlokliDb, safe_contracts::BlokliDbSafeContractOperations,
        safe_history::BlokliDbSafeHistoryOperations,
    };
    use blokli_db_entity::hopr_safe_contract::{Column as SafeContractColumn, Entity as SafeContractEntity};
    use hopr_bindings::exports::alloy::primitives::{Address as AlloyAddress, B256, U256 as AlloyU256};
    use hopr_types::crypto::types::Hash;
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

    async fn setup_safe(db: &BlokliDb, safe_address: Address) -> anyhow::Result<()> {
        db.create_safe_contract(
            None,
            safe_address,
            address_with_byte(2),
            address_with_byte(3),
            100,
            0,
            0,
        )
        .await?;
        Ok(())
    }

    async fn apply(
        handlers: ContractEventHandlers<ClonableMockOperations, BlokliDb>,
        db: &BlokliDb,
        safe_address: Address,
        block: u64,
        tx_index: u64,
        event: SafeContractEvents,
    ) -> anyhow::Result<()> {
        let log = test_rpc_log(safe_address, block, tx_index, 0);
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
        Ok(())
    }

    fn added(owner: Address) -> SafeContractEvents {
        SafeContractEvents::AddedOwner(SafeContract::AddedOwner {
            owner: AlloyAddress::from_hopr_address(owner),
        })
    }

    fn removed(owner: Address) -> SafeContractEvents {
        SafeContractEvents::RemovedOwner(SafeContract::RemovedOwner {
            owner: AlloyAddress::from_hopr_address(owner),
        })
    }

    fn handlers_for(db: &BlokliDb) -> ContractEventHandlers<ClonableMockOperations, BlokliDb> {
        init_handlers(
            ClonableMockOperations {
                inner: Arc::new(MockIndexerRpcOperations::new()),
            },
            db.clone(),
        )
    }

    #[tokio::test]
    async fn test_safe_setup_persists_owners_and_threshold() -> anyhow::Result<()> {
        let db = BlokliDb::new_in_memory().await?;
        let safe_address = address_with_byte(1);
        setup_safe(&db, safe_address).await?;
        let (owner_one, owner_two) = (address_with_byte(5), address_with_byte(6));

        let event = SafeContractEvents::SafeSetup(SafeContract::SafeSetup {
            initiator: AlloyAddress::from_hopr_address(address_with_byte(4)),
            owners: vec![
                AlloyAddress::from_hopr_address(owner_one),
                AlloyAddress::from_hopr_address(owner_two),
            ],
            threshold: AlloyU256::from(2_u64),
            initializer: AlloyAddress::ZERO,
            fallbackHandler: AlloyAddress::ZERO,
        });
        apply(handlers_for(&db), &db, safe_address, 101, 4, event).await?;

        assert_eq!(
            db.get_safe_owners(None, safe_address).await?,
            vec![owner_one, owner_two]
        );
        assert_eq!(db.get_safe_threshold(None, safe_address).await?.as_deref(), Some("2"));

        Ok(())
    }

    #[tokio::test]
    async fn test_live_events_apply_above_the_snapshot_block_only() -> anyhow::Result<()> {
        let db = BlokliDb::new_in_memory().await?;
        let safe_address = address_with_byte(11);
        setup_safe(&db, safe_address).await?;
        let (kept, ignored, applied) = (address_with_byte(14), address_with_byte(15), address_with_byte(16));

        db.apply_safe_owner_snapshot(None, safe_address, vec![kept], "1".into(), 200)
            .await?;

        // At and below the snapshot block: ignored.
        apply(handlers_for(&db), &db, safe_address, 200, 3, added(ignored)).await?;
        apply(handlers_for(&db), &db, safe_address, 150, 0, removed(kept)).await?;
        apply(
            handlers_for(&db),
            &db,
            safe_address,
            200,
            4,
            SafeContractEvents::ChangedThreshold(SafeContract::ChangedThreshold {
                threshold: AlloyU256::from(9_u64),
            }),
        )
        .await?;
        assert_eq!(db.get_safe_owners(None, safe_address).await?, vec![kept]);
        assert_eq!(db.get_safe_threshold(None, safe_address).await?.as_deref(), Some("1"));

        // Above it: applied.
        apply(handlers_for(&db), &db, safe_address, 201, 0, added(applied)).await?;
        apply(
            handlers_for(&db),
            &db,
            safe_address,
            202,
            0,
            SafeContractEvents::ChangedThreshold(SafeContract::ChangedThreshold {
                threshold: AlloyU256::from(2_u64),
            }),
        )
        .await?;
        let mut expected = vec![kept, applied];
        expected.sort();
        assert_eq!(db.get_safe_owners(None, safe_address).await?, expected);
        assert_eq!(db.get_safe_threshold(None, safe_address).await?.as_deref(), Some("2"));

        Ok(())
    }

    #[tokio::test]
    async fn test_replayed_added_owner_after_removal_does_not_restore_the_owner() -> anyhow::Result<()> {
        let db = BlokliDb::new_in_memory().await?;
        let safe_address = address_with_byte(21);
        setup_safe(&db, safe_address).await?;
        let owner = address_with_byte(22);

        apply(handlers_for(&db), &db, safe_address, 101, 0, added(owner)).await?;
        apply(handlers_for(&db), &db, safe_address, 104, 0, removed(owner)).await?;
        assert!(db.get_safe_owners(None, safe_address).await?.is_empty());

        // The same AddedOwner is delivered again after the removal.
        apply(handlers_for(&db), &db, safe_address, 101, 0, added(owner)).await?;
        assert!(db.get_safe_owners(None, safe_address).await?.is_empty());

        Ok(())
    }

    #[tokio::test]
    async fn test_execution_events_are_not_persisted() -> anyhow::Result<()> {
        let db = BlokliDb::new_in_memory().await?;
        let safe_address = address_with_byte(31);
        setup_safe(&db, safe_address).await?;

        apply(
            handlers_for(&db),
            &db,
            safe_address,
            101,
            0,
            SafeContractEvents::ExecutionSuccess(SafeContract::ExecutionSuccess {
                txHash: B256::from_slice(hash_with_byte(15).as_ref()),
                payment: AlloyU256::from(12_u64),
            }),
        )
        .await?;

        assert!(db.get_safe_owners(None, safe_address).await?.is_empty());
        assert_eq!(db.get_safe_threshold(None, safe_address).await?, None);

        let safe_count = SafeContractEntity::find()
            .filter(SafeContractColumn::Address.eq(safe_address.as_ref().to_vec()))
            .count(db.conn(TargetDb::Index))
            .await?;
        assert_eq!(safe_count, 1, "event handling should not duplicate the safe identity");

        Ok(())
    }

    #[tokio::test]
    async fn test_discovery_snapshot_reads_the_pinned_block() -> anyhow::Result<()> {
        let db = BlokliDb::new_in_memory().await?;
        let safe_address = address_with_byte(41);
        setup_safe(&db, safe_address).await?;
        let (owner_one, owner_two) = (address_with_byte(42), address_with_byte(43));

        let mut rpc = MockIndexerRpcOperations::new();
        rpc.expect_get_safe_owners_and_threshold()
            .withf(move |safe, block| *safe == safe_address && *block == 120)
            .times(1)
            .returning(move |_, _| {
                Ok(SafeOwnersSnapshot {
                    owners: vec![owner_one, owner_two],
                    threshold: 2,
                })
            });
        let handlers = init_handlers(ClonableMockOperations { inner: Arc::new(rpc) }, db.clone());

        db.begin_transaction()
            .await?
            .perform(|tx| {
                Box::pin(async move { handlers.snapshot_safe_owners_at_discovery(tx, safe_address, 120).await })
            })
            .await?;

        assert_eq!(
            db.get_safe_owners(None, safe_address).await?,
            vec![owner_one, owner_two]
        );
        assert_eq!(db.get_safe_threshold(None, safe_address).await?.as_deref(), Some("2"));
        assert_eq!(db.get_safe_owners_synced_block(None, safe_address).await?, Some(120));

        Ok(())
    }
}
