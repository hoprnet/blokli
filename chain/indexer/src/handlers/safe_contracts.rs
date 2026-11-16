use blokli_chain_rpc::Log;
use blokli_chain_types::AlloyAddressExt;
use blokli_db::{BlokliDbAllOperations, OpenTransaction};
use hopr_types::primitive::prelude::Address;
use tracing::{debug, info};

#[cfg(all(feature = "telemetry", not(test)))]
use super::increment_indexer_contract_log_count;
use super::{ContractEventHandlers, u256_to_u64};
use crate::{custom_abis::safe_contract_events::SafeContract::SafeContractEvents, errors::Result, state::IndexerEvent};

#[cfg(all(feature = "telemetry", not(test)))]
fn safe_contract_event_metric_label(event: &SafeContractEvents) -> &'static str {
    match event {
        SafeContractEvents::SafeSetup(_) => "safe_setup",
        SafeContractEvents::AddedOwner(_) => "safe_owner_added",
        SafeContractEvents::RemovedOwner(_) => "safe_owner_removed",
        SafeContractEvents::ChangedThreshold(_) => "safe_threshold_changed",
        SafeContractEvents::ExecutionSuccess(_) => "safe_execution_success",
        SafeContractEvents::ExecutionFailure(_) => "safe_execution_failure",
        SafeContractEvents::ExecutionFromModuleSuccess(_) => "safe_module_execution_success",
        SafeContractEvents::ExecutionFromModuleFailure(_) => "safe_module_execution_failure",
    }
}

impl<T, Db> ContractEventHandlers<T, Db>
where
    T: blokli_chain_rpc::HoprIndexerRpcOperations + Clone + Send + 'static,
    Db: BlokliDbAllOperations + Clone,
{
    /// Requests an owner snapshot of the Safe that covers `block`, without any RPC call.
    ///
    /// Block processing must not wait for the RPC node while its database transaction is open, so the read
    /// is left to the background refresh task: the Safe is reported to it together with `block`, and the
    /// task reads it pinned at a block at or above `block` (reads pinned below are dropped). Until the
    /// snapshot is stored, live owner and threshold events of the Safe are skipped, because applying them
    /// alone would produce an owner set that looks complete but is not.
    ///
    /// The report is made while the block is still being processed. Should the block's transaction roll
    /// back, the Safe is read anyway (a harmless extra read) and the recorded block only delays the read
    /// until the chain position has reached it.
    pub(super) fn request_safe_owner_snapshot(&self, safe_address: Address, block: u64) {
        self.indexer_state.record_safe_owner_event(safe_address, block);
        self.indexer_state.mark_safe_owner_snapshot_pending(safe_address);
    }

    /// Whether a live owner or threshold event in `block` must not be applied.
    ///
    /// * Safe with a snapshot at or above `block`: the snapshot already includes the event, skip it.
    /// * Safe with an older snapshot: apply the event on top of it.
    /// * Safe without a snapshot: skip the event and request a snapshot covering `block` (see
    ///   [`Self::request_safe_owner_snapshot`]).
    async fn skip_owner_event(&self, tx: &OpenTransaction, safe_address: Address, block: u64) -> Result<bool> {
        match self.db.get_safe_owners_synced_block(Some(tx), safe_address).await? {
            Some(synced_block) => Ok(block <= synced_block),
            None => {
                self.request_safe_owner_snapshot(safe_address, block);
                Ok(true)
            }
        }
    }

    pub(super) async fn on_safe_contract_event(
        &self,
        tx: &OpenTransaction,
        safe_address: Address,
        log: &Log,
        event: SafeContractEvents,
        _is_synced: bool,
    ) -> Result<Vec<IndexerEvent>> {
        #[cfg(all(feature = "telemetry", not(test)))]
        increment_indexer_contract_log_count(safe_contract_event_metric_label(&event));

        let log_index = u256_to_u64(log.log_index, "log_index")?;

        // Owner and threshold events are only applied above the snapshot block of the Safe.
        let owner_events = matches!(
            event,
            SafeContractEvents::SafeSetup(_)
                | SafeContractEvents::AddedOwner(_)
                | SafeContractEvents::RemovedOwner(_)
                | SafeContractEvents::ChangedThreshold(_)
        );
        if owner_events {
            // A refresh read pinned before this block does not contain the event.
            self.indexer_state
                .record_safe_owner_event(safe_address, log.block_number);
        }
        if owner_events && self.skip_owner_event(tx, safe_address, log.block_number).await? {
            debug!(
                safe_address = %safe_address,
                block = log.block_number,
                "Skipping Safe event covered by, or replaced with, an owner snapshot"
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

    use blokli_chain_rpc::{HoprIndexerRpcOperations, Log, SafeOwnersSnapshot, errors::RpcError};
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
        handlers::test_utils::test_helpers::{
            ClonableMockOperations, MockIndexerRpcOperations, init_handlers, init_handlers_with_events, try_recv_event,
        },
        safe_owner_refresh::{SafeSnapshotRead, store_safe_owner_snapshot},
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
        // Live events only apply on top of a snapshot.
        db.apply_safe_owner_snapshot(None, safe_address, vec![], "1".into(), 100)
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
        assert_eq!(db.get_safe_threshold(None, safe_address).await?.as_deref(), Some("1"));

        let safe_count = SafeContractEntity::find()
            .filter(SafeContractColumn::Address.eq(safe_address.as_ref().to_vec()))
            .count(db.conn(TargetDb::Index))
            .await?;
        assert_eq!(safe_count, 1, "event handling should not duplicate the safe identity");

        Ok(())
    }

    #[tokio::test]
    async fn test_default_batch_read_reports_failures_per_safe_in_input_order() -> anyhow::Result<()> {
        let (ok_safe, bad_safe) = (address_with_byte(61), address_with_byte(62));
        let owner = address_with_byte(63);
        let mut rpc = MockIndexerRpcOperations::new();
        rpc.expect_get_safe_owners_and_threshold().returning(move |safe, _| {
            if safe == bad_safe {
                Err(RpcError::Other("execution reverted".into()))
            } else {
                Ok(SafeOwnersSnapshot {
                    owners: vec![owner],
                    threshold: 1,
                })
            }
        });
        let rpc = ClonableMockOperations { inner: Arc::new(rpc) };

        let results = rpc.get_safe_owners_and_thresholds(vec![bad_safe, ok_safe], 10).await?;

        assert_eq!(results.len(), 2);
        assert!(results[0].is_err());
        assert_eq!(results[1].as_ref().expect("ok").owners, vec![owner]);

        Ok(())
    }
    async fn pending_safe(db: &BlokliDb, byte: u8) -> anyhow::Result<Address> {
        let safe_address = address_with_byte(byte);
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
        Ok(safe_address)
    }

    #[tokio::test]
    async fn test_stale_refresh_read_is_dropped_after_a_skipped_owner_event() -> anyhow::Result<()> {
        let db = BlokliDb::new_in_memory().await?;
        let safe_address = pending_safe(&db, 74).await?;
        let (removed_owner, kept_owner) = (address_with_byte(75), address_with_byte(76));

        // No RPC expectation: the handler must not call the node while its transaction is open.
        let rpc = MockIndexerRpcOperations::new();
        let (handlers, state, _events) =
            init_handlers_with_events(ClonableMockOperations { inner: Arc::new(rpc) }, db.clone());

        apply(handlers, &db, safe_address, 201, 0, removed(removed_owner)).await?;
        // The event was skipped (the Safe has no snapshot) and the Safe reported to the refresh task.
        assert_eq!(state.take_pending_safe_owner_snapshots(), vec![safe_address]);

        let stale_read = SafeSnapshotRead {
            safe_address,
            block: 200,
            snapshot: SafeOwnersSnapshot {
                owners: vec![removed_owner, kept_owner],
                threshold: 1,
            },
        };
        store_safe_owner_snapshot(&db, &state, stale_read).await;

        // The block-200 result does not contain the removal at 201: it is not stored and the Safe is
        // reported again to be read at a newer block.
        assert!(db.get_safe_owners(None, safe_address).await?.is_empty());
        assert_eq!(db.get_safe_owners_synced_block(None, safe_address).await?, None);
        assert_eq!(state.take_pending_safe_owner_snapshots(), vec![safe_address]);

        // A read pinned at or above the event block is stored.
        let fresh_read = SafeSnapshotRead {
            safe_address,
            block: 202,
            snapshot: SafeOwnersSnapshot {
                owners: vec![kept_owner],
                threshold: 1,
            },
        };
        store_safe_owner_snapshot(&db, &state, fresh_read).await;
        assert_eq!(db.get_safe_owners(None, safe_address).await?, vec![kept_owner]);
        assert_eq!(state.safe_owner_event_block(safe_address), None);

        Ok(())
    }

    fn read_at(safe_address: Address, block: u64) -> SafeSnapshotRead {
        SafeSnapshotRead {
            safe_address,
            block,
            snapshot: SafeOwnersSnapshot {
                owners: vec![address_with_byte(95)],
                threshold: 1,
            },
        }
    }

    fn deployed_announcements(events: &mut async_broadcast::Receiver<Arc<crate::state::IndexerEvent>>) -> Vec<Address> {
        std::iter::from_fn(|| try_recv_event(events))
            .filter_map(|event| match event {
                crate::state::IndexerEvent::SafeDeployed(address) => Some(address),
                _ => None,
            })
            .collect()
    }

    #[tokio::test]
    async fn test_deployment_is_not_announced_for_a_dropped_read() -> anyhow::Result<()> {
        let db = BlokliDb::new_in_memory().await?;
        let safe_address = pending_safe(&db, 80).await?;
        let state = crate::state::IndexerState::default();
        let mut events = state.subscribe_to_events();

        state.mark_safe_awaiting_deployment_announcement(safe_address);
        state.record_safe_owner_event(safe_address, 201);

        // The read predates a processed owner event: it is dropped and nothing is announced.
        store_safe_owner_snapshot(&db, &state, read_at(safe_address, 200)).await;
        assert!(deployed_announcements(&mut events).is_empty());

        // The flag stays until a snapshot is stored.
        store_safe_owner_snapshot(&db, &state, read_at(safe_address, 202)).await;
        assert_eq!(deployed_announcements(&mut events), vec![safe_address]);

        Ok(())
    }

    #[tokio::test]
    async fn test_deployment_is_not_announced_for_a_failed_store() -> anyhow::Result<()> {
        let db = BlokliDb::new_in_memory().await?;
        // The Safe is not in the database yet, so storing its snapshot fails.
        let safe_address = address_with_byte(81);
        let state = crate::state::IndexerState::default();
        let mut events = state.subscribe_to_events();

        state.mark_safe_awaiting_deployment_announcement(safe_address);
        store_safe_owner_snapshot(&db, &state, read_at(safe_address, 200)).await;
        assert!(deployed_announcements(&mut events).is_empty());

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
        store_safe_owner_snapshot(&db, &state, read_at(safe_address, 201)).await;
        assert_eq!(deployed_announcements(&mut events), vec![safe_address]);

        Ok(())
    }

    #[tokio::test]
    async fn test_deployment_is_not_announced_for_an_unflagged_safe() -> anyhow::Result<()> {
        let db = BlokliDb::new_in_memory().await?;
        let safe_address = pending_safe(&db, 82).await?;
        let state = crate::state::IndexerState::default();
        let mut events = state.subscribe_to_events();

        store_safe_owner_snapshot(&db, &state, read_at(safe_address, 200)).await;
        assert!(deployed_announcements(&mut events).is_empty());

        Ok(())
    }

    #[tokio::test]
    async fn test_discovery_requests_a_snapshot_without_calling_rpc() -> anyhow::Result<()> {
        let db = BlokliDb::new_in_memory().await?;
        let safe_address = pending_safe(&db, 77).await?;

        // A mock without expectations panics on any RPC call.
        let (handlers, state, _events) = init_handlers_with_events(
            ClonableMockOperations {
                inner: Arc::new(MockIndexerRpcOperations::new()),
            },
            db.clone(),
        );

        handlers.request_safe_owner_snapshot(safe_address, 120);

        assert_eq!(state.take_pending_safe_owner_snapshots(), vec![safe_address]);
        assert_eq!(state.safe_owner_event_block(safe_address), Some(120));
        assert_eq!(state.unsynced_safe_owner_snapshots(), 1);

        Ok(())
    }

    #[tokio::test]
    async fn test_event_for_a_safe_without_snapshot_is_skipped_without_rpc() -> anyhow::Result<()> {
        let db = BlokliDb::new_in_memory().await?;
        let safe_address = pending_safe(&db, 78).await?;
        let (handlers, state, _events) = init_handlers_with_events(
            ClonableMockOperations {
                inner: Arc::new(MockIndexerRpcOperations::new()),
            },
            db.clone(),
        );

        apply(handlers, &db, safe_address, 130, 0, added(address_with_byte(79))).await?;

        assert!(
            db.get_safe_owners(None, safe_address).await?.is_empty(),
            "no partial owner set"
        );
        assert_eq!(db.get_safe_owners_synced_block(None, safe_address).await?, None);
        assert_eq!(state.take_pending_safe_owner_snapshots(), vec![safe_address]);
        assert_eq!(state.safe_owner_event_block(safe_address), Some(130));

        // The record is only forgotten once a snapshot covering the event is stored, not by the handler.
        let covering_read = SafeSnapshotRead {
            safe_address,
            block: 131,
            snapshot: SafeOwnersSnapshot {
                owners: vec![address_with_byte(79)],
                threshold: 1,
            },
        };
        store_safe_owner_snapshot(&db, &state, covering_read).await;
        assert_eq!(
            db.get_safe_owners(None, safe_address).await?,
            vec![address_with_byte(79)]
        );
        assert_eq!(state.safe_owner_event_block(safe_address), None);
        assert_eq!(state.unsynced_safe_owner_snapshots(), 0);

        Ok(())
    }
    /// The live owner-event guard and the position-guarded writes on a real PostgreSQL.
    #[tokio::test]
    #[ignore = "needs PostgreSQL, run with `just test-postgres-schema`"]
    async fn test_postgres_live_events_apply_above_the_snapshot_block_only() -> anyhow::Result<()> {
        let scratch = crate::test_postgres::ScratchPostgres::create().await?;
        let db = scratch.db.clone();
        let safe_address = address_with_byte(91);
        setup_safe(&db, safe_address).await?;
        let (kept, ignored, applied) = (address_with_byte(92), address_with_byte(93), address_with_byte(94));

        db.apply_safe_owner_snapshot(None, safe_address, vec![kept], "1".into(), 200)
            .await?;

        // At and below the snapshot block: ignored.
        apply(handlers_for(&db), &db, safe_address, 200, 3, added(ignored)).await?;
        apply(handlers_for(&db), &db, safe_address, 150, 0, removed(kept)).await?;
        assert_eq!(db.get_safe_owners(None, safe_address).await?, vec![kept]);

        // Above it: applied, and a replay of an older event afterwards does not undo it.
        apply(handlers_for(&db), &db, safe_address, 201, 0, added(applied)).await?;
        apply(handlers_for(&db), &db, safe_address, 202, 0, removed(applied)).await?;
        apply(handlers_for(&db), &db, safe_address, 201, 0, added(applied)).await?;
        assert_eq!(db.get_safe_owners(None, safe_address).await?, vec![kept]);

        scratch.drop_database().await
    }
}
