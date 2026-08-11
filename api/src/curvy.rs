use blokli_api_types::{
    CurvyCommittedNote, CurvyCommittedNullifier, CurvyEventCursor, CurvyEventPosition, CurvyPendingNote,
    CurvyShardRoot, CurvySyncCheckpoint, CurvySyncNote, Hex32, UInt64, UInt256,
};
use blokli_db_entity::{
    curvy_committed_note::{self, Model as DbCurvyCommittedNote},
    curvy_committed_nullifier,
    curvy_pending_note::{self, Model as DbCurvyPendingNote},
    curvy_shard_root,
    curvy_sync_checkpoint::{self, Model as DbCurvySyncCheckpoint},
};
use hopr_bindings::exports::alloy::primitives::U256;
use hopr_types::{
    crypto::types::Hash,
    primitive::{primitives::Address, traits::ToHex},
};
use sea_orm::{ColumnTrait, Condition, DatabaseConnection, EntityTrait, QueryFilter, QueryOrder};

use crate::{errors, validation::validate_eth_address};

type CurvyResult<T> = std::result::Result<T, blokli_api_types::QueryFailedError>;

const PAGE_DEFAULT_LIMIT: u64 = 100;
const PAGE_MAX_LIMIT: u64 = 1000;
pub const NOTE_LOOKUP_BATCH_SIZE: usize = 900;

#[derive(Clone, Copy)]
pub struct DatabaseEventCursor {
    pub block: i64,
    pub transaction_index: i64,
    pub log_index: i64,
    pub event_item_index: i64,
    /// Branch the cursor was issued on, when the client supplied one.
    pub block_hash: Option<[u8; 32]>,
}

pub fn page_limit(first: Option<i32>) -> CurvyResult<u64> {
    match first {
        None => Ok(PAGE_DEFAULT_LIMIT),
        Some(first) if first > 0 && u64::try_from(first).is_ok_and(|first| first <= PAGE_MAX_LIMIT) => {
            u64::try_from(first).map_err(|error| errors::invalid_pagination(&error.to_string()))
        }
        Some(_) => Err(errors::invalid_pagination("first must be between 1 and 1000")),
    }
}

pub fn from_block(from_block: Option<UInt64>) -> CurvyResult<Option<i64>> {
    from_block
        .map(|block| {
            i64::try_from(block.0)
                .map_err(|_| errors::invalid_pagination("fromBlock exceeds the supported database range"))
        })
        .transpose()
}

pub fn event_cursor(cursor: Option<CurvyEventCursor>) -> CurvyResult<Option<DatabaseEventCursor>> {
    cursor
        .map(|cursor| {
            let convert = |value: UInt64| {
                i64::try_from(value.0)
                    .map_err(|_| errors::invalid_pagination("cursor exceeds the supported database range"))
            };
            let block_hash = cursor
                .block_hash
                .as_ref()
                .map(|hash| bytes32(&hex32_bytes(hash)?, "cursor blockHash"))
                .transpose()?;
            Ok(DatabaseEventCursor {
                block: convert(cursor.block)?,
                transaction_index: convert(cursor.transaction_index)?,
                log_index: convert(cursor.log_index)?,
                event_item_index: convert(cursor.event_item_index)?,
                block_hash,
            })
        })
        .transpose()
}

/// Rejects a cursor whose anchoring block is no longer the one at that position.
///
/// `stored` is the block hash currently recorded at the cursor's exact position, or
/// `None` when no row sits there any more. Either a mismatch or an absent row means the
/// branch the cursor was issued on was reorganized away, so continuing with an exclusive
/// `>` comparison would silently skip the canonical replacement at that position.
///
/// Cursors without an anchor are accepted unchanged.
pub fn ensure_cursor_anchor(cursor: &DatabaseEventCursor, stored: Option<&[u8]>) -> CurvyResult<()> {
    let Some(expected) = cursor.block_hash.as_ref() else {
        return Ok(());
    };
    if stored == Some(expected.as_slice()) {
        Ok(())
    } else {
        Err(errors::invalid_pagination(
            "cursor is anchored to a block that is no longer canonical; re-read the event history from an earlier \
             position",
        ))
    }
}

pub fn event_cursor_condition<C>(
    block: C,
    transaction_index: C,
    log_index: C,
    event_item_index: C,
    cursor: DatabaseEventCursor,
) -> Condition
where
    C: ColumnTrait + Copy,
{
    Condition::any()
        .add(block.gt(cursor.block))
        .add(
            Condition::all()
                .add(block.eq(cursor.block))
                .add(transaction_index.gt(cursor.transaction_index)),
        )
        .add(
            Condition::all()
                .add(block.eq(cursor.block))
                .add(transaction_index.eq(cursor.transaction_index))
                .add(log_index.gt(cursor.log_index)),
        )
        .add(
            Condition::all()
                .add(block.eq(cursor.block))
                .add(transaction_index.eq(cursor.transaction_index))
                .add(log_index.eq(cursor.log_index))
                .add(event_item_index.gt(cursor.event_item_index)),
        )
}

pub fn dense_start(from_index: Option<UInt64>) -> CurvyResult<i64> {
    i64::try_from(from_index.unwrap_or(UInt64(0)).0)
        .map_err(|_| errors::invalid_pagination("fromIndex exceeds the supported database range"))
}

pub fn page_next_index(start: i64, total: i64, last: Option<i64>) -> CurvyResult<UInt64> {
    if total < 0 {
        return Err(errors::invalid_db_data(
            "checkpoint count",
            "expected a non-negative value",
        ));
    }
    let next = match last {
        Some(last) => last
            .checked_add(1)
            .ok_or_else(|| errors::invalid_db_data("dense index", "next index exceeds the database range"))?,
        None => start.min(total),
    }
    .min(total);
    u64::try_from(next)
        .map(UInt64)
        .map_err(|_| errors::invalid_db_data("dense index", "expected a non-negative value"))
}

pub fn page_total(total: i64) -> CurvyResult<UInt64> {
    u64::try_from(total)
        .map(UInt64)
        .map_err(|_| errors::invalid_db_data("checkpoint count", "expected a non-negative value"))
}

pub fn validate_page_start(start: i64, total: i64) -> CurvyResult<()> {
    if total < 0 {
        return Err(errors::invalid_db_data(
            "checkpoint count",
            "expected a non-negative value",
        ));
    }
    if start > total {
        return Err(errors::invalid_pagination(
            "fromIndex must not exceed the checkpoint total",
        ));
    }
    Ok(())
}

pub fn validate_dense_page(
    indices: impl IntoIterator<Item = i64>,
    start: i64,
    total: i64,
    field: &str,
) -> CurvyResult<()> {
    if total < 0 {
        return Err(errors::invalid_db_data(
            "checkpoint count",
            "expected a non-negative value",
        ));
    }
    let mut expected = start;
    for index in indices {
        if index != expected {
            return Err(errors::invalid_db_data(
                field,
                &format!("expected dense index {expected}, found {index}"),
            ));
        }
        expected = expected
            .checked_add(1)
            .ok_or_else(|| errors::invalid_db_data(field, "next index exceeds the database range"))?;
    }
    if expected == start && start < total {
        return Err(errors::invalid_db_data(
            field,
            &format!("dense index {start} is missing before checkpoint total {total}"),
        ));
    }
    Ok(())
}

/// Confirms a pinned checkpoint still exists once its page has been read.
///
/// Curvy rollback deletes event rows, shard roots and checkpoints at or after the first
/// affected block in a single transaction. A checkpoint that survives that deletion is
/// therefore older than the reorganized suffix, and every row below its counts was
/// written before it, so nothing the page returned can have been replaced. A checkpoint
/// that is gone pinned an orphaned branch, and its page must not be served: the dense
/// indexes alone cannot distinguish replacement rows, because replay rebuilds them with
/// the same values.
///
/// Callers must invoke this *after* reading the page, not before.
pub async fn ensure_checkpoint_still_pinned(
    db: &DatabaseConnection,
    checkpoint: &DbCurvySyncCheckpoint,
) -> CurvyResult<()> {
    let survived = curvy_sync_checkpoint::Entity::find()
        .filter(curvy_sync_checkpoint::Column::BlockHash.eq(checkpoint.block_hash.clone()))
        .one(db)
        .await
        .map_err(|error| errors::query_failed("re-check Curvy sync checkpoint", error))?
        .is_some();

    if survived {
        Ok(())
    } else {
        Err(errors::query_failed(
            "serve checkpoint-pinned Curvy page",
            "the pinned checkpoint was removed by a chain reorganization; re-read the checkpoint and retry",
        ))
    }
}

pub async fn load_checkpoint(
    db: &DatabaseConnection,
    block_hash: Option<&Hex32>,
) -> CurvyResult<DbCurvySyncCheckpoint> {
    let mut query = curvy_sync_checkpoint::Entity::find();
    let identifier = if let Some(block_hash) = block_hash {
        query = query.filter(curvy_sync_checkpoint::Column::BlockHash.eq(hex32_bytes(block_hash)?));
        block_hash.0.clone()
    } else {
        query = query.order_by_desc(curvy_sync_checkpoint::Column::BlockNumber);
        "latest".to_string()
    };
    query
        .one(db)
        .await
        .map_err(|error| errors::query_failed("fetch Curvy sync checkpoint", error))?
        .ok_or_else(|| errors::not_found("Curvy sync checkpoint", identifier))
}

pub fn pending_precedes_commit(pending: &DbCurvyPendingNote, committed: &DbCurvyCommittedNote) -> bool {
    (
        pending.published_block,
        pending.published_tx_index,
        pending.published_log_index,
        pending.event_item_index,
    ) <= (
        committed.published_block,
        committed.published_tx_index,
        committed.published_log_index,
        committed.event_item_index,
    )
}

pub fn hex32_bytes(value: &Hex32) -> CurvyResult<Vec<u8>> {
    let value = value.0.strip_prefix("0x").unwrap_or(&value.0);
    hex::decode(value).map_err(|error| errors::validation_failed(&format!("invalid Hex32 value: {error}")))
}

pub fn hex32_array(value: &Hex32) -> CurvyResult<[u8; 32]> {
    bytes32(&hex32_bytes(value)?, "Hex32")
}

pub fn uint256_bytes(value: &UInt256, field: &str) -> CurvyResult<[u8; 32]> {
    value
        .0
        .parse::<U256>()
        .map(|value| value.to_be_bytes())
        .map_err(|error| errors::validation_failed(&format!("invalid {field}: {error}")))
}

pub fn require_contract(address: Address, contract: &str) -> CurvyResult<()> {
    if address == Address::default() {
        Err(errors::internal_error(
            "Curvy configuration",
            format!("{contract} address is not configured"),
        ))
    } else {
        Ok(())
    }
}

pub fn address(value: &str) -> std::result::Result<Address, blokli_api_types::InvalidAddressError> {
    validate_eth_address(value).map_err(|error| errors::invalid_address_from_message(value, error.message))?;
    Address::from_hex(value).map_err(|error| errors::invalid_address_error(value, error))
}

pub fn bytes32(value: &[u8], field: &str) -> CurvyResult<[u8; 32]> {
    value
        .try_into()
        .map_err(|_| errors::invalid_db_data(field, "expected exactly 32 bytes"))
}

fn hex32(value: &[u8], field: &str) -> CurvyResult<Hex32> {
    Ok(Hex32(Hash::from(bytes32(value, field)?).to_hex()))
}

fn uint256(value: &[u8], field: &str) -> CurvyResult<UInt256> {
    Ok(UInt256(U256::from_be_bytes(bytes32(value, field)?).to_string()))
}

pub fn uint256_value(value: [u8; 32]) -> UInt256 {
    UInt256(U256::from_be_bytes(value).to_string())
}

fn uint64(value: i64, field: &str) -> CurvyResult<UInt64> {
    u64::try_from(value)
        .map(UInt64)
        .map_err(|_| errors::invalid_db_data(field, "expected a non-negative 64-bit integer"))
}

fn int32(value: i64, field: &str) -> CurvyResult<i32> {
    i32::try_from(value).map_err(|_| errors::invalid_db_data(field, "value exceeds the GraphQL Int range"))
}

fn dense_index(value: i64, field: &str) -> CurvyResult<UInt64> {
    uint64(value, field)
}

fn position(
    chain_tx_hash: &[u8],
    block_hash: &[u8],
    block: i64,
    transaction_index: i64,
    log_index: i64,
    event_item_index: i64,
) -> CurvyResult<CurvyEventPosition> {
    Ok(CurvyEventPosition {
        transaction_hash: hex32(chain_tx_hash, "chain_tx_hash")?,
        block_hash: hex32(block_hash, "block_hash")?,
        block: uint64(block, "published_block")?,
        transaction_index: uint64(transaction_index, "published_tx_index")?,
        log_index: uint64(log_index, "published_log_index")?,
        event_item_index: uint64(event_item_index, "event_item_index")?,
    })
}

pub fn pending_note(model: curvy_pending_note::Model) -> CurvyResult<CurvyPendingNote> {
    Ok(CurvyPendingNote {
        note_id: hex32(&model.note_id, "note_id")?,
        ephemeral_key: vec![
            uint256(&model.ephemeral_key_x, "ephemeral_key_x")?,
            uint256(&model.ephemeral_key_y, "ephemeral_key_y")?,
        ],
        view_tag: int32(model.view_tag, "view_tag")?,
        token_id: uint256(&model.token_id, "token_id")?,
        amount: uint256(&model.amount, "amount")?,
        is_plaintext: model.is_plaintext,
        position: position(
            &model.chain_tx_hash,
            &model.block_hash,
            model.published_block,
            model.published_tx_index,
            model.published_log_index,
            model.event_item_index,
        )?,
    })
}

pub fn committed_note(model: curvy_committed_note::Model) -> CurvyResult<CurvyCommittedNote> {
    Ok(CurvyCommittedNote {
        batch_index: hex32(&model.batch_index, "batch_index")?,
        note_id: hex32(&model.note_id, "note_id")?,
        leaf_index: dense_index(model.leaf_index, "leaf_index")?,
        position: position(
            &model.chain_tx_hash,
            &model.block_hash,
            model.published_block,
            model.published_tx_index,
            model.published_log_index,
            model.event_item_index,
        )?,
    })
}

pub fn committed_nullifier(model: curvy_committed_nullifier::Model) -> CurvyResult<CurvyCommittedNullifier> {
    Ok(CurvyCommittedNullifier {
        batch_index: hex32(&model.batch_index, "batch_index")?,
        nullifier: hex32(&model.nullifier, "nullifier")?,
        nullifier_index: dense_index(model.nullifier_index, "nullifier_index")?,
        position: position(
            &model.chain_tx_hash,
            &model.block_hash,
            model.published_block,
            model.published_tx_index,
            model.published_log_index,
            model.event_item_index,
        )?,
    })
}

pub fn sync_checkpoint(model: curvy_sync_checkpoint::Model) -> CurvyResult<CurvySyncCheckpoint> {
    let shard_height = uint64(model.shard_height, "shard_height")?;
    let shard_height_bits =
        u32::try_from(shard_height.0).map_err(|_| errors::invalid_db_data("shard_height", "value is too large"))?;
    let shard_size = 1_u64
        .checked_shl(shard_height_bits)
        .ok_or_else(|| errors::invalid_db_data("shard_height", "shard size overflows UInt64"))?;
    let aggregator_address = Address::try_from(model.aggregator_address.as_slice())
        .map_err(|_| errors::invalid_db_data("aggregator_address", "expected exactly 20 bytes"))?;

    Ok(CurvySyncCheckpoint {
        block_number: uint64(model.block_number, "block_number")?,
        block_hash: hex32(&model.block_hash, "block_hash")?,
        aggregator_address: aggregator_address.to_hex(),
        tree_version: int32(model.tree_version, "tree_version")?,
        tree_depth: int32(model.tree_depth, "tree_depth")?,
        shard_height: int32(model.shard_height, "shard_height")?,
        shard_size: UInt64(shard_size),
        note_count: uint64(model.leaf_count, "leaf_count")?,
        nullifier_count: uint64(model.nullifier_count, "nullifier_count")?,
        shard_count: uint64(model.shard_count, "shard_count")?,
        notes_root: hex32(&model.root, "root")?,
    })
}

pub fn sync_note(
    model: curvy_committed_note::Model,
    announcement: Option<curvy_pending_note::Model>,
) -> CurvyResult<CurvySyncNote> {
    Ok(CurvySyncNote {
        leaf_index: dense_index(model.leaf_index, "leaf_index")?,
        note_id: hex32(&model.note_id, "note_id")?,
        batch_index: hex32(&model.batch_index, "batch_index")?,
        announcement: announcement.map(pending_note).transpose()?,
        commit_position: position(
            &model.chain_tx_hash,
            &model.block_hash,
            model.published_block,
            model.published_tx_index,
            model.published_log_index,
            model.event_item_index,
        )?,
    })
}

pub fn shard_root(model: curvy_shard_root::Model) -> CurvyResult<CurvyShardRoot> {
    Ok(CurvyShardRoot {
        shard_index: uint64(model.shard_index, "shard_index")?,
        root: hex32(&model.root, "root")?,
        completion_position: position(
            &model.chain_tx_hash,
            &model.block_hash,
            model.completion_block,
            model.completion_tx_index,
            model.completion_log_index,
            model.completion_event_item_index,
        )?,
    })
}
