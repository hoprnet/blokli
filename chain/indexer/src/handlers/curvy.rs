use blokli_api_types::{
    CurvyCommittedNote, CurvyCommittedNullifier, CurvyEventPosition, CurvyPendingNote, Hex32, UInt64, UInt256,
};
use blokli_chain_rpc::Log;
use blokli_chain_types::curvy_tree::{
    NOTES_SHARD_HEIGHT, NOTES_TREE_DEPTH, NOTES_TREE_VERSION, NotesFrontier, fr_to_be_32,
};
use blokli_db::{BlokliDbAllOperations, OpenTransaction, errors::DbSqlError};
use blokli_db_entity::{
    curvy_committed_note, curvy_committed_nullifier, curvy_pending_note, curvy_shard_root, curvy_sync_checkpoint,
};
use curvy_bindings::curvy_aggregator_alpha_v2::CurvyAggregatorAlphaV2::{
    CommittedNotes, CommittedNullifiers, CurvyAggregatorAlphaV2Events, PendingNotes,
};
use hopr_bindings::exports::alloy::primitives::U256;
use hopr_types::primitive::traits::ToHex;
use sea_orm::{ActiveValue::Set, ColumnTrait, EntityTrait, QueryFilter, QueryOrder, sea_query::OnConflict};
use tracing::info;

use super::ContractEventHandlers;
#[cfg(all(feature = "telemetry", not(test)))]
use super::increment_indexer_contract_log_count;
use crate::{
    errors::{CoreEthereumIndexerError, Result},
    state::IndexerEvent,
};

#[derive(Clone, Copy)]
enum CurvyEventKind {
    PendingNote,
    CommittedNote,
    CommittedNullifier,
}

fn invalid_state(message: impl Into<String>) -> CoreEthereumIndexerError {
    CoreEthereumIndexerError::CurvyStateInvariant(message.into())
}

fn u256_bytes(value: U256) -> Vec<u8> {
    value.to_be_bytes::<32>().to_vec()
}

fn u256_scalar(value: U256) -> UInt256 {
    UInt256(value.to_string())
}

fn u256_hex(value: U256) -> Hex32 {
    Hex32(format!("{value:#066x}"))
}

fn position(log: &Log, event_item_index: i64) -> Result<CurvyEventPosition> {
    Ok(CurvyEventPosition {
        transaction_hash: Hex32(log.tx_hash.to_hex()),
        block_hash: Hex32(log.block_hash.to_hex()),
        block: UInt64(log.block_number),
        transaction_index: UInt64(log.tx_index),
        log_index: UInt64(log.log_index.as_u64()),
        event_item_index: UInt64(u64::try_from(event_item_index)?),
    })
}

fn coordinates(log: &Log) -> Result<(i64, i64, i64)> {
    Ok((
        i64::try_from(log.block_number)?,
        i64::try_from(log.tx_index)?,
        i64::try_from(log.log_index.as_u64())?,
    ))
}

fn validate_pending_notes(event: &PendingNotes) -> Result<usize> {
    let len = event.noteIds.len();
    if event.ephemeralKeys[0].len() != len
        || event.ephemeralKeys[1].len() != len
        || event.viewTags.len() != len
        || event.tokens.len() != len
        || event.amounts.len() != len
        || event.isPlaintext.len() != len
    {
        return Err(invalid_state("PendingNotes contains arrays with different lengths"));
    }
    Ok(len)
}

async fn log_already_indexed(
    tx: &OpenTransaction,
    kind: CurvyEventKind,
    block: i64,
    tx_index: i64,
    log_index: i64,
) -> Result<bool> {
    let exists = match kind {
        CurvyEventKind::PendingNote => curvy_pending_note::Entity::find()
            .filter(curvy_pending_note::Column::PublishedBlock.eq(block))
            .filter(curvy_pending_note::Column::PublishedTxIndex.eq(tx_index))
            .filter(curvy_pending_note::Column::PublishedLogIndex.eq(log_index))
            .one(tx.as_ref())
            .await
            .map_err(DbSqlError::from)?
            .is_some(),
        CurvyEventKind::CommittedNote => curvy_committed_note::Entity::find()
            .filter(curvy_committed_note::Column::PublishedBlock.eq(block))
            .filter(curvy_committed_note::Column::PublishedTxIndex.eq(tx_index))
            .filter(curvy_committed_note::Column::PublishedLogIndex.eq(log_index))
            .one(tx.as_ref())
            .await
            .map_err(DbSqlError::from)?
            .is_some(),
        CurvyEventKind::CommittedNullifier => curvy_committed_nullifier::Entity::find()
            .filter(curvy_committed_nullifier::Column::PublishedBlock.eq(block))
            .filter(curvy_committed_nullifier::Column::PublishedTxIndex.eq(tx_index))
            .filter(curvy_committed_nullifier::Column::PublishedLogIndex.eq(log_index))
            .one(tx.as_ref())
            .await
            .map_err(DbSqlError::from)?
            .is_some(),
    };
    Ok(exists)
}

async fn retained_dense_counts(tx: &OpenTransaction) -> Result<(usize, usize)> {
    let leaf_count = curvy_committed_note::Entity::find()
        .order_by_desc(curvy_committed_note::Column::LeafIndex)
        .one(tx.as_ref())
        .await
        .map_err(DbSqlError::from)?
        .map(|model| usize::try_from(model.leaf_index))
        .transpose()?
        .map(|index| index + 1)
        .unwrap_or_default();
    let nullifier_count = curvy_committed_nullifier::Entity::find()
        .order_by_desc(curvy_committed_nullifier::Column::NullifierIndex)
        .one(tx.as_ref())
        .await
        .map_err(DbSqlError::from)?
        .map(|model| usize::try_from(model.nullifier_index))
        .transpose()?
        .map(|index| index + 1)
        .unwrap_or_default();
    Ok((leaf_count, nullifier_count))
}

fn restore_checkpoint(
    checkpoint: &curvy_sync_checkpoint::Model,
    retained_leaf_count: usize,
    retained_nullifier_count: usize,
) -> Result<(NotesFrontier, usize)> {
    if checkpoint.tree_version != NOTES_TREE_VERSION
        || checkpoint.tree_depth != i64::try_from(NOTES_TREE_DEPTH)?
        || checkpoint.shard_height != i64::try_from(NOTES_SHARD_HEIGHT)?
    {
        return Err(invalid_state("stored Curvy checkpoint uses unsupported tree geometry"));
    }

    let checkpoint_leaf_count = usize::try_from(checkpoint.leaf_count)?;
    let checkpoint_nullifier_count = usize::try_from(checkpoint.nullifier_count)?;
    if checkpoint_leaf_count != retained_leaf_count || checkpoint_nullifier_count != retained_nullifier_count {
        return Err(invalid_state(
            "stored Curvy checkpoint counts do not match retained dense event indexes",
        ));
    }

    let frontier = NotesFrontier::from_snapshot_bytes(&checkpoint.frontier_snapshot)?;
    if frontier.leaf_count() != checkpoint_leaf_count
        || frontier.shard_count() != usize::try_from(checkpoint.shard_count)?
        || frontier.root_be_32().as_slice() != checkpoint.root.as_slice()
    {
        return Err(invalid_state(
            "stored Curvy checkpoint is inconsistent with its frontier snapshot",
        ));
    }
    Ok((frontier, checkpoint_nullifier_count))
}

async fn load_frontier(tx: &OpenTransaction) -> Result<(NotesFrontier, usize)> {
    let retained_counts = retained_dense_counts(tx).await?;
    let checkpoint = curvy_sync_checkpoint::Entity::find()
        .order_by_desc(curvy_sync_checkpoint::Column::BlockNumber)
        .one(tx.as_ref())
        .await
        .map_err(DbSqlError::from)?;

    match checkpoint {
        Some(checkpoint) => restore_checkpoint(&checkpoint, retained_counts.0, retained_counts.1),
        None if retained_counts == (0, 0) => Ok((NotesFrontier::production(), 0)),
        None => Err(invalid_state(
            "stored Curvy event history exists but its checkpoint is missing",
        )),
    }
}

async fn persist_checkpoint(
    tx: &OpenTransaction,
    log: &Log,
    aggregator_address: Vec<u8>,
    frontier: &NotesFrontier,
    nullifier_count: usize,
) -> Result<()> {
    let checkpoint = curvy_sync_checkpoint::ActiveModel {
        block_number: Set(i64::try_from(log.block_number)?),
        block_hash: Set(log.block_hash.as_ref().to_vec()),
        aggregator_address: Set(aggregator_address),
        tree_version: Set(NOTES_TREE_VERSION),
        tree_depth: Set(i64::try_from(NOTES_TREE_DEPTH)?),
        shard_height: Set(i64::try_from(NOTES_SHARD_HEIGHT)?),
        leaf_count: Set(i64::try_from(frontier.leaf_count())?),
        nullifier_count: Set(i64::try_from(nullifier_count)?),
        shard_count: Set(i64::try_from(frontier.shard_count())?),
        root: Set(frontier.root_be_32().to_vec()),
        frontier_snapshot: Set(frontier.encode_snapshot()),
        ..Default::default()
    };
    curvy_sync_checkpoint::Entity::insert(checkpoint)
        .on_conflict(
            OnConflict::column(curvy_sync_checkpoint::Column::BlockNumber)
                .update_columns([
                    curvy_sync_checkpoint::Column::BlockHash,
                    curvy_sync_checkpoint::Column::AggregatorAddress,
                    curvy_sync_checkpoint::Column::TreeVersion,
                    curvy_sync_checkpoint::Column::TreeDepth,
                    curvy_sync_checkpoint::Column::ShardHeight,
                    curvy_sync_checkpoint::Column::LeafCount,
                    curvy_sync_checkpoint::Column::NullifierCount,
                    curvy_sync_checkpoint::Column::ShardCount,
                    curvy_sync_checkpoint::Column::Root,
                    curvy_sync_checkpoint::Column::FrontierSnapshot,
                ])
                .to_owned(),
        )
        .exec_without_returning(tx.as_ref())
        .await
        .map_err(DbSqlError::from)?;
    Ok(())
}

impl<T, Db> ContractEventHandlers<T, Db>
where
    Db: BlokliDbAllOperations + Clone,
{
    pub(super) async fn on_curvy_aggregator_event(
        &self,
        tx: &OpenTransaction,
        log: &Log,
        event: CurvyAggregatorAlphaV2Events,
    ) -> Result<Vec<IndexerEvent>> {
        #[cfg(all(feature = "telemetry", not(test)))]
        increment_indexer_contract_log_count("curvy_aggregator");
        match event {
            CurvyAggregatorAlphaV2Events::PendingNotes(event) => self.on_curvy_pending_notes(tx, log, event).await,
            CurvyAggregatorAlphaV2Events::CommittedNotes(event) => self.on_curvy_committed_notes(tx, log, event).await,
            CurvyAggregatorAlphaV2Events::CommittedNullifiers(event) => {
                self.on_curvy_committed_nullifiers(tx, log, event).await
            }
            CurvyAggregatorAlphaV2Events::CommitmentGasFeeRootUpdated(_)
            | CurvyAggregatorAlphaV2Events::DirectShieldEnabledUpdated(_)
            | CurvyAggregatorAlphaV2Events::Initialized(_)
            | CurvyAggregatorAlphaV2Events::OwnershipTransferred(_)
            | CurvyAggregatorAlphaV2Events::RoleAdminChanged(_)
            | CurvyAggregatorAlphaV2Events::RoleGranted(_)
            | CurvyAggregatorAlphaV2Events::RoleRevoked(_)
            | CurvyAggregatorAlphaV2Events::Upgraded(_) => Ok(Vec::new()),
        }
    }

    async fn on_curvy_pending_notes(
        &self,
        tx: &OpenTransaction,
        log: &Log,
        event: PendingNotes,
    ) -> Result<Vec<IndexerEvent>> {
        let len = validate_pending_notes(&event)?;
        if len == 0 {
            return Ok(Vec::new());
        }
        let (block, tx_index, log_index) = coordinates(log)?;
        if log_already_indexed(tx, CurvyEventKind::PendingNote, block, tx_index, log_index).await? {
            return Ok(Vec::new());
        }

        let chain_tx_hash = log.tx_hash.as_ref().to_vec();
        let block_hash = log.block_hash.as_ref().to_vec();
        let mut models = Vec::with_capacity(len);
        let mut events = Vec::with_capacity(len);
        for item_index in 0..len {
            if event.noteIds[item_index] == U256::ZERO {
                continue;
            }
            let event_item_index = i64::try_from(item_index)?;
            models.push(curvy_pending_note::ActiveModel {
                note_id: Set(u256_bytes(event.noteIds[item_index])),
                ephemeral_key_x: Set(u256_bytes(event.ephemeralKeys[0][item_index])),
                ephemeral_key_y: Set(u256_bytes(event.ephemeralKeys[1][item_index])),
                view_tag: Set(i64::from(event.viewTags[item_index])),
                token_id: Set(u256_bytes(event.tokens[item_index])),
                amount: Set(u256_bytes(event.amounts[item_index])),
                is_plaintext: Set(event.isPlaintext[item_index]),
                event_item_index: Set(event_item_index),
                chain_tx_hash: Set(chain_tx_hash.clone()),
                block_hash: Set(block_hash.clone()),
                published_block: Set(block),
                published_tx_index: Set(tx_index),
                published_log_index: Set(log_index),
                ..Default::default()
            });
            events.push(IndexerEvent::CurvyPendingNote(CurvyPendingNote {
                note_id: u256_hex(event.noteIds[item_index]),
                ephemeral_key: vec![
                    u256_scalar(event.ephemeralKeys[0][item_index]),
                    u256_scalar(event.ephemeralKeys[1][item_index]),
                ],
                view_tag: i32::from(event.viewTags[item_index]),
                token_id: u256_scalar(event.tokens[item_index]),
                amount: u256_scalar(event.amounts[item_index]),
                is_plaintext: event.isPlaintext[item_index],
                position: position(log, event_item_index)?,
            }));
        }
        if !models.is_empty() {
            curvy_pending_note::Entity::insert_many(models)
                .exec_without_returning(tx.as_ref())
                .await
                .map_err(DbSqlError::from)?;
        }
        Ok(events)
    }

    async fn on_curvy_committed_notes(
        &self,
        tx: &OpenTransaction,
        log: &Log,
        event: CommittedNotes,
    ) -> Result<Vec<IndexerEvent>> {
        let (block, tx_index, log_index) = coordinates(log)?;
        if log_already_indexed(tx, CurvyEventKind::CommittedNote, block, tx_index, log_index).await? {
            return Ok(Vec::new());
        }

        let chain_tx_hash = log.tx_hash.as_ref().to_vec();
        let block_hash = log.block_hash.as_ref().to_vec();
        let (mut frontier, nullifier_count) = load_frontier(tx).await?;
        let mut models = Vec::with_capacity(event.noteIds.len());
        let mut shard_roots = Vec::new();
        let mut events = Vec::with_capacity(event.noteIds.len());
        for (item_index, note_id) in event.noteIds.into_iter().enumerate() {
            if note_id == U256::ZERO {
                continue;
            }
            let event_item_index = i64::try_from(item_index)?;
            let note_id_bytes = note_id.to_be_bytes::<32>();
            let appended = frontier.append_be_32(&note_id_bytes)?;
            let leaf_index = i64::try_from(appended.leaf_index)?;
            models.push(curvy_committed_note::ActiveModel {
                batch_index: Set(u256_bytes(event.batchIndex)),
                note_id: Set(note_id_bytes.to_vec()),
                event_item_index: Set(event_item_index),
                chain_tx_hash: Set(chain_tx_hash.clone()),
                block_hash: Set(block_hash.clone()),
                published_block: Set(block),
                published_tx_index: Set(tx_index),
                published_log_index: Set(log_index),
                leaf_index: Set(leaf_index),
                ..Default::default()
            });
            if let Some(shard) = appended.completed_shard {
                shard_roots.push(curvy_shard_root::ActiveModel {
                    tree_version: Set(NOTES_TREE_VERSION),
                    shard_height: Set(i64::try_from(NOTES_SHARD_HEIGHT)?),
                    shard_index: Set(i64::try_from(shard.shard_index)?),
                    root: Set(fr_to_be_32(&shard.root).to_vec()),
                    block_hash: Set(block_hash.clone()),
                    chain_tx_hash: Set(chain_tx_hash.clone()),
                    completion_block: Set(block),
                    completion_tx_index: Set(tx_index),
                    completion_log_index: Set(log_index),
                    completion_event_item_index: Set(event_item_index),
                    ..Default::default()
                });
            }
            events.push(IndexerEvent::CurvyCommittedNote(CurvyCommittedNote {
                batch_index: u256_hex(event.batchIndex),
                note_id: u256_hex(note_id),
                leaf_index: UInt64(u64::try_from(leaf_index)?),
                position: position(log, event_item_index)?,
            }));
        }
        if !models.is_empty() {
            curvy_committed_note::Entity::insert_many(models)
                .exec_without_returning(tx.as_ref())
                .await
                .map_err(DbSqlError::from)?;
        }
        if !shard_roots.is_empty() {
            curvy_shard_root::Entity::insert_many(shard_roots)
                .exec_without_returning(tx.as_ref())
                .await
                .map_err(DbSqlError::from)?;
        }
        persist_checkpoint(
            tx,
            log,
            self.addresses.curvy_aggregator.as_ref().to_vec(),
            &frontier,
            nullifier_count,
        )
        .await?;
        Ok(events)
    }

    async fn on_curvy_committed_nullifiers(
        &self,
        tx: &OpenTransaction,
        log: &Log,
        event: CommittedNullifiers,
    ) -> Result<Vec<IndexerEvent>> {
        let (block, tx_index, log_index) = coordinates(log)?;
        if log_already_indexed(tx, CurvyEventKind::CommittedNullifier, block, tx_index, log_index).await? {
            return Ok(Vec::new());
        }

        let chain_tx_hash = log.tx_hash.as_ref().to_vec();
        let block_hash = log.block_hash.as_ref().to_vec();
        let (frontier, nullifier_count) = load_frontier(tx).await?;
        let mut models = Vec::with_capacity(event.nullifiers.len());
        let mut events = Vec::with_capacity(event.nullifiers.len());
        for (item_index, nullifier) in event.nullifiers.into_iter().enumerate() {
            if nullifier == U256::ZERO {
                continue;
            }
            let event_item_index = i64::try_from(item_index)?;
            let nullifier_index = nullifier_count
                .checked_add(models.len())
                .ok_or_else(|| invalid_state("Curvy nullifier index overflow"))?;
            models.push(curvy_committed_nullifier::ActiveModel {
                batch_index: Set(u256_bytes(event.batchIndex)),
                nullifier: Set(u256_bytes(nullifier)),
                event_item_index: Set(event_item_index),
                chain_tx_hash: Set(chain_tx_hash.clone()),
                block_hash: Set(block_hash.clone()),
                published_block: Set(block),
                published_tx_index: Set(tx_index),
                published_log_index: Set(log_index),
                nullifier_index: Set(i64::try_from(nullifier_index)?),
                ..Default::default()
            });
            events.push(IndexerEvent::CurvyCommittedNullifier(CurvyCommittedNullifier {
                batch_index: u256_hex(event.batchIndex),
                nullifier: u256_hex(nullifier),
                nullifier_index: UInt64(u64::try_from(nullifier_index)?),
                position: position(log, event_item_index)?,
            }));
        }
        let new_nullifier_count = nullifier_count
            .checked_add(models.len())
            .ok_or_else(|| invalid_state("Curvy nullifier count overflow"))?;
        if !models.is_empty() {
            curvy_committed_nullifier::Entity::insert_many(models)
                .exec_without_returning(tx.as_ref())
                .await
                .map_err(DbSqlError::from)?;
        }
        persist_checkpoint(
            tx,
            log,
            self.addresses.curvy_aggregator.as_ref().to_vec(),
            &frontier,
            new_nullifier_count,
        )
        .await?;
        Ok(events)
    }
}

impl<T, Db> ContractEventHandlers<T, Db>
where
    Db: BlokliDbAllOperations + Clone + Send + Sync,
{
    pub(super) async fn revert_curvy_state(&self, from_block: u64) -> Result<()> {
        let from_block = i64::try_from(from_block)?;
        self.db
            .begin_transaction()
            .await?
            .perform(|tx| {
                Box::pin(async move {
                    let pending_notes = curvy_pending_note::Entity::delete_many()
                        .filter(curvy_pending_note::Column::PublishedBlock.gte(from_block))
                        .exec(tx.as_ref())
                        .await
                        .map_err(DbSqlError::from)?
                        .rows_affected;
                    let committed_notes = curvy_committed_note::Entity::delete_many()
                        .filter(curvy_committed_note::Column::PublishedBlock.gte(from_block))
                        .exec(tx.as_ref())
                        .await
                        .map_err(DbSqlError::from)?
                        .rows_affected;
                    let committed_nullifiers = curvy_committed_nullifier::Entity::delete_many()
                        .filter(curvy_committed_nullifier::Column::PublishedBlock.gte(from_block))
                        .exec(tx.as_ref())
                        .await
                        .map_err(DbSqlError::from)?
                        .rows_affected;
                    curvy_shard_root::Entity::delete_many()
                        .filter(curvy_shard_root::Column::CompletionBlock.gte(from_block))
                        .exec(tx.as_ref())
                        .await
                        .map_err(DbSqlError::from)?;
                    curvy_sync_checkpoint::Entity::delete_many()
                        .filter(curvy_sync_checkpoint::Column::BlockNumber.gte(from_block))
                        .exec(tx.as_ref())
                        .await
                        .map_err(DbSqlError::from)?;

                    let retained_counts = retained_dense_counts(tx).await?;
                    let checkpoint = curvy_sync_checkpoint::Entity::find()
                        .order_by_desc(curvy_sync_checkpoint::Column::BlockNumber)
                        .one(tx.as_ref())
                        .await
                        .map_err(DbSqlError::from)?;
                    match checkpoint {
                        Some(checkpoint) => {
                            restore_checkpoint(&checkpoint, retained_counts.0, retained_counts.1)?;
                        }
                        None if retained_counts == (0, 0) => {}
                        None => {
                            return Err(invalid_state(
                                "cannot restore retained Curvy history because its checkpoint is missing",
                            ));
                        }
                    }

                    info!(
                        deleted_events = pending_notes + committed_notes + committed_nullifiers,
                        "removed orphaned Curvy event history"
                    );
                    Ok(())
                })
            })
            .await
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use blokli_chain_types::curvy_tree::{NOTES_SHARD_HEIGHT, NOTES_TREE_DEPTH, NOTES_TREE_VERSION, NotesFrontier};
    use blokli_db::{BlokliDbGeneralModelOperations, db::BlokliDb};
    use blokli_db_entity::{curvy_committed_note, curvy_sync_checkpoint};
    use curvy_bindings::curvy_aggregator_alpha_v2::CurvyAggregatorAlphaV2::PendingNotes;
    use hopr_bindings::exports::alloy::primitives::U256;
    use sea_orm::{ActiveValue::Set, EntityTrait};

    use super::{load_frontier, restore_checkpoint, validate_pending_notes};
    use crate::{
        errors::CoreEthereumIndexerError,
        handlers::test_utils::test_helpers::{ClonableMockOperations, MockIndexerRpcOperations, init_handlers},
    };

    fn empty_checkpoint() -> anyhow::Result<curvy_sync_checkpoint::Model> {
        let frontier = NotesFrontier::production();
        Ok(curvy_sync_checkpoint::Model {
            id: 1,
            block_number: 1,
            block_hash: vec![1; 32],
            aggregator_address: vec![2; 20],
            tree_version: NOTES_TREE_VERSION,
            tree_depth: i64::try_from(NOTES_TREE_DEPTH)?,
            shard_height: i64::try_from(NOTES_SHARD_HEIGHT)?,
            leaf_count: 0,
            nullifier_count: 0,
            shard_count: 0,
            root: frontier.root_be_32().to_vec(),
            frontier_snapshot: frontier.encode_snapshot(),
        })
    }

    async fn insert_committed_note(db: &BlokliDb, published_block: i64) -> anyhow::Result<()> {
        curvy_committed_note::Entity::insert(curvy_committed_note::ActiveModel {
            batch_index: Set(vec![0; 32]),
            note_id: Set(vec![1; 32]),
            event_item_index: Set(0),
            chain_tx_hash: Set(vec![2; 32]),
            block_hash: Set(vec![3; 32]),
            published_block: Set(published_block),
            published_tx_index: Set(0),
            published_log_index: Set(0),
            leaf_index: Set(0),
            ..Default::default()
        })
        .exec_without_returning(db.conn(Default::default()))
        .await?;
        Ok(())
    }

    fn pending_notes() -> PendingNotes {
        PendingNotes {
            noteIds: vec![U256::from(1)],
            ephemeralKeys: [vec![U256::from(2)], vec![U256::from(3)]],
            viewTags: vec![4],
            tokens: vec![U256::from(5)],
            amounts: vec![U256::from(6)],
            isPlaintext: vec![true],
        }
    }

    #[test]
    fn test_pending_notes_accepts_aligned_arrays() {
        assert_eq!(validate_pending_notes(&pending_notes()).unwrap(), 1);
    }

    #[test]
    fn test_pending_notes_rejects_misaligned_arrays() {
        let mut event = pending_notes();
        event.amounts.clear();

        assert!(validate_pending_notes(&event).is_err());
    }

    #[test]
    fn test_restore_checkpoint_rejects_unsupported_tree_geometry() -> anyhow::Result<()> {
        let mut checkpoint = empty_checkpoint()?;
        checkpoint.tree_depth -= 1;

        assert!(matches!(
            restore_checkpoint(&checkpoint, 0, 0),
            Err(CoreEthereumIndexerError::CurvyStateInvariant(message))
                if message == "stored Curvy checkpoint uses unsupported tree geometry"
        ));
        Ok(())
    }

    #[test]
    fn test_restore_checkpoint_rejects_counts_mismatching_dense_indexes() -> anyhow::Result<()> {
        let checkpoint = empty_checkpoint()?;

        for retained_counts in [(1, 0), (0, 1)] {
            assert!(matches!(
                restore_checkpoint(&checkpoint, retained_counts.0, retained_counts.1),
                Err(CoreEthereumIndexerError::CurvyStateInvariant(message))
                    if message == "stored Curvy checkpoint counts do not match retained dense event indexes"
            ));
        }
        Ok(())
    }

    #[tokio::test]
    async fn test_load_frontier_rejects_history_without_checkpoint() -> anyhow::Result<()> {
        let db = BlokliDb::new_in_memory().await?;
        insert_committed_note(&db, 1).await?;

        let result = db
            .begin_transaction()
            .await?
            .perform(|tx| Box::pin(async move { load_frontier(tx).await }))
            .await;

        assert!(matches!(
            result,
            Err(CoreEthereumIndexerError::CurvyStateInvariant(message))
                if message == "stored Curvy event history exists but its checkpoint is missing"
        ));
        Ok(())
    }

    #[tokio::test]
    async fn test_revert_curvy_state_rejects_retained_history_without_checkpoint() -> anyhow::Result<()> {
        let db = BlokliDb::new_in_memory().await?;
        insert_committed_note(&db, 1).await?;
        let handlers = init_handlers(
            ClonableMockOperations {
                inner: Arc::new(MockIndexerRpcOperations::new()),
            },
            db,
        );

        let result = handlers.revert_curvy_state(2).await;

        assert!(matches!(
            result,
            Err(CoreEthereumIndexerError::CurvyStateInvariant(message))
                if message == "cannot restore retained Curvy history because its checkpoint is missing"
        ));
        Ok(())
    }
}
