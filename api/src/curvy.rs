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

#[cfg(test)]
mod tests {
    use blokli_api_types::{CurvyEventCursor, Hex32, QueryFailedError, UInt64, UInt256};
    use blokli_db::{BlokliDbGeneralModelOperations, db::BlokliDb};
    use blokli_db_entity::{curvy_committed_note, curvy_pending_note, curvy_shard_root, curvy_sync_checkpoint};
    use hopr_types::primitive::{primitives::Address, traits::ToHex};
    use sea_orm::{ActiveModelTrait, ActiveValue::Set, EntityTrait};

    use super::{
        DatabaseEventCursor, address, committed_note, dense_start, ensure_checkpoint_still_pinned,
        ensure_cursor_anchor, event_cursor, from_block, hex32_array, load_checkpoint, page_limit, page_next_index,
        page_total, pending_note, pending_precedes_commit, require_contract, shard_root, sync_checkpoint, sync_note,
        uint256_bytes, validate_dense_page, validate_page_start,
    };
    use crate::errors::codes;

    fn bytes(value: u8, len: usize) -> Vec<u8> {
        vec![value; len]
    }

    fn api_result<T>(result: Result<T, QueryFailedError>) -> anyhow::Result<T> {
        result.map_err(|error| anyhow::anyhow!(error.message))
    }

    fn pending_model() -> curvy_pending_note::Model {
        curvy_pending_note::Model {
            id: 1,
            note_id: bytes(1, 32),
            ephemeral_key_x: bytes(0, 32),
            ephemeral_key_y: bytes(0, 31).into_iter().chain([2]).collect(),
            view_tag: 3,
            token_id: bytes(0, 32),
            amount: bytes(0, 31).into_iter().chain([4]).collect(),
            is_plaintext: true,
            event_item_index: 5,
            chain_tx_hash: bytes(6, 32),
            block_hash: bytes(7, 32),
            published_block: 8,
            published_tx_index: 9,
            published_log_index: 10,
        }
    }

    fn committed_model() -> curvy_committed_note::Model {
        curvy_committed_note::Model {
            id: 1,
            batch_index: bytes(11, 32),
            note_id: bytes(1, 32),
            event_item_index: 12,
            chain_tx_hash: bytes(13, 32),
            block_hash: bytes(14, 32),
            published_block: 15,
            published_tx_index: 16,
            published_log_index: 17,
            leaf_index: 18,
        }
    }

    fn checkpoint_model(block_number: i64, block_hash: Vec<u8>) -> curvy_sync_checkpoint::Model {
        curvy_sync_checkpoint::Model {
            id: block_number,
            block_number,
            block_hash,
            aggregator_address: bytes(20, 20),
            tree_version: 1,
            tree_depth: 32,
            shard_height: 4,
            leaf_count: 21,
            nullifier_count: 22,
            shard_count: 2,
            root: bytes(23, 32),
            frontier_snapshot: Vec::new(),
        }
    }

    #[test]
    fn test_pagination_boundaries() -> anyhow::Result<()> {
        assert_eq!(api_result(page_limit(None))?, 100);
        assert_eq!(api_result(page_limit(Some(1)))?, 1);
        assert_eq!(api_result(page_limit(Some(1000)))?, 1000);
        for invalid in [0, -1, 1001] {
            assert_eq!(
                page_limit(Some(invalid)).expect_err("limit must be rejected").code,
                codes::INVALID_PAGINATION
            );
        }

        assert_eq!(api_result(from_block(None))?, None);
        assert_eq!(api_result(from_block(Some(UInt64(i64::MAX as u64))))?, Some(i64::MAX));
        assert_eq!(
            from_block(Some(UInt64(i64::MAX as u64 + 1)))
                .expect_err("oversized block must be rejected")
                .code,
            codes::INVALID_PAGINATION
        );
        assert_eq!(api_result(dense_start(None))?, 0);
        assert!(dense_start(Some(UInt64(u64::MAX))).is_err());
        Ok(())
    }

    #[test]
    fn test_dense_page_validation_and_next_index() -> anyhow::Result<()> {
        assert_eq!(api_result(page_next_index(3, 10, Some(5)))?, UInt64(6));
        assert_eq!(api_result(page_next_index(12, 10, None))?, UInt64(10));
        assert_eq!(api_result(page_total(10))?, UInt64(10));
        api_result(validate_page_start(10, 10))?;
        api_result(validate_dense_page([3, 4, 5], 3, 10, "leaf_index"))?;

        assert!(page_next_index(0, -1, None).is_err());
        assert!(page_next_index(0, i64::MAX, Some(i64::MAX)).is_err());
        assert!(page_total(-1).is_err());
        assert!(validate_page_start(11, 10).is_err());
        assert!(validate_page_start(0, -1).is_err());
        assert!(validate_dense_page([3, 5], 3, 10, "leaf_index").is_err());
        assert!(validate_dense_page([], 3, 10, "leaf_index").is_err());
        assert!(validate_dense_page([], 10, 10, "leaf_index").is_ok());
        assert!(validate_dense_page([i64::MAX], i64::MAX, i64::MAX, "leaf_index").is_err());
        Ok(())
    }

    #[test]
    fn test_cursor_conversion_and_anchor_validation() -> anyhow::Result<()> {
        let hash = Hex32(format!("0x{}", "ab".repeat(32)));
        let cursor = api_result(event_cursor(Some(CurvyEventCursor {
            block: UInt64(1),
            transaction_index: UInt64(2),
            log_index: UInt64(3),
            event_item_index: UInt64(4),
            block_hash: Some(hash),
        })))?
        .ok_or_else(|| anyhow::anyhow!("cursor unexpectedly missing"))?;
        assert_eq!(
            (
                cursor.block,
                cursor.transaction_index,
                cursor.log_index,
                cursor.event_item_index
            ),
            (1, 2, 3, 4)
        );
        assert_eq!(cursor.block_hash, Some([0xab; 32]));
        api_result(ensure_cursor_anchor(&cursor, Some(&[0xab; 32])))?;
        assert!(ensure_cursor_anchor(&cursor, Some(&[0xcd; 32])).is_err());
        assert!(ensure_cursor_anchor(&cursor, None).is_err());

        let unanchored = DatabaseEventCursor {
            block_hash: None,
            ..cursor
        };
        api_result(ensure_cursor_anchor(&unanchored, None))?;
        assert!(
            event_cursor(Some(CurvyEventCursor {
                block: UInt64(u64::MAX),
                transaction_index: UInt64(0),
                log_index: UInt64(0),
                event_item_index: UInt64(0),
                block_hash: None,
            }))
            .is_err()
        );
        Ok(())
    }

    #[test]
    fn test_external_scalar_and_contract_validation() -> anyhow::Result<()> {
        let value = Hex32(format!("0x{}", "01".repeat(32)));
        assert_eq!(api_result(hex32_array(&value))?, [1; 32]);
        assert!(hex32_array(&Hex32("not-hex".to_string())).is_err());
        assert!(hex32_array(&Hex32("01".repeat(31))).is_err());
        assert_eq!(
            api_result(uint256_bytes(&UInt256("255".to_string()), "amount"))?[31],
            255
        );
        assert!(uint256_bytes(&UInt256("invalid".to_string()), "amount").is_err());

        let configured =
            address("0x1111111111111111111111111111111111111111").map_err(|error| anyhow::anyhow!(error.message))?;
        assert_eq!(configured.to_hex(), "0x1111111111111111111111111111111111111111");
        assert!(address("invalid").is_err());
        api_result(require_contract(configured, "Aggregator"))?;
        assert!(require_contract(Address::default(), "Aggregator").is_err());
        Ok(())
    }

    #[test]
    fn test_event_model_conversions() -> anyhow::Result<()> {
        let pending_model = pending_model();
        let pending = api_result(pending_note(pending_model.clone()))?;
        assert_eq!(pending.view_tag, 3);
        assert_eq!(pending.amount, UInt256("4".to_string()));
        assert_eq!(pending.position.event_item_index, UInt64(5));

        let committed_model = committed_model();
        let committed = api_result(committed_note(committed_model.clone()))?;
        assert_eq!(committed.leaf_index, UInt64(18));
        assert!(pending_precedes_commit(&pending_model, &committed_model));

        let sync = api_result(sync_note(committed_model, Some(pending_model)))?;
        assert_eq!(sync.leaf_index, UInt64(18));
        assert!(sync.announcement.is_some());
        Ok(())
    }

    #[test]
    fn test_event_model_conversions_reject_invalid_database_values() {
        let mut pending = pending_model();
        pending.chain_tx_hash.pop();
        assert_eq!(
            pending_note(pending).expect_err("short hash must be rejected").code,
            codes::INVALID_DB_DATA
        );

        let mut committed = committed_model();
        committed.leaf_index = -1;
        assert_eq!(
            committed_note(committed)
                .expect_err("negative index must be rejected")
                .code,
            codes::INVALID_DB_DATA
        );
    }

    #[test]
    fn test_checkpoint_and_shard_conversions() -> anyhow::Result<()> {
        let checkpoint = api_result(sync_checkpoint(checkpoint_model(24, bytes(25, 32))))?;
        assert_eq!(checkpoint.block_number, UInt64(24));
        assert_eq!(checkpoint.shard_size, UInt64(16));
        assert_eq!(checkpoint.note_count, UInt64(21));

        let shard = api_result(shard_root(curvy_shard_root::Model {
            id: 1,
            tree_version: 1,
            shard_height: 4,
            shard_index: 2,
            root: bytes(26, 32),
            block_hash: bytes(27, 32),
            chain_tx_hash: bytes(28, 32),
            completion_block: 29,
            completion_tx_index: 30,
            completion_log_index: 31,
            completion_event_item_index: 32,
        }))?;
        assert_eq!(shard.shard_index, UInt64(2));
        assert_eq!(shard.completion_position.block, UInt64(29));

        let mut invalid_checkpoint = checkpoint_model(1, bytes(1, 32));
        invalid_checkpoint.shard_height = 64;
        assert!(sync_checkpoint(invalid_checkpoint).is_err());
        Ok(())
    }

    #[tokio::test]
    async fn test_checkpoint_loading_and_reorg_pin() -> anyhow::Result<()> {
        let db = BlokliDb::new_in_memory().await?;
        let first = checkpoint_model(1, bytes(1, 32));
        let second = checkpoint_model(2, bytes(2, 32));
        for checkpoint in [first.clone(), second.clone()] {
            curvy_sync_checkpoint::ActiveModel {
                id: Set(checkpoint.id),
                block_number: Set(checkpoint.block_number),
                block_hash: Set(checkpoint.block_hash),
                aggregator_address: Set(checkpoint.aggregator_address),
                tree_version: Set(checkpoint.tree_version),
                tree_depth: Set(checkpoint.tree_depth),
                shard_height: Set(checkpoint.shard_height),
                leaf_count: Set(checkpoint.leaf_count),
                nullifier_count: Set(checkpoint.nullifier_count),
                shard_count: Set(checkpoint.shard_count),
                root: Set(checkpoint.root),
                frontier_snapshot: Set(checkpoint.frontier_snapshot),
            }
            .insert(db.conn(Default::default()))
            .await?;
        }

        let latest = api_result(load_checkpoint(db.conn(Default::default()), None).await)?;
        assert_eq!(latest.block_number, 2);
        let first_hash = Hex32(format!("0x{}", "01".repeat(32)));
        let loaded_first = api_result(load_checkpoint(db.conn(Default::default()), Some(&first_hash)).await)?;
        assert_eq!(loaded_first.block_number, 1);
        api_result(ensure_checkpoint_still_pinned(db.conn(Default::default()), &loaded_first).await)?;

        curvy_sync_checkpoint::Entity::delete_by_id(loaded_first.id)
            .exec(db.conn(Default::default()))
            .await?;
        assert!(
            ensure_checkpoint_still_pinned(db.conn(Default::default()), &loaded_first)
                .await
                .is_err()
        );
        assert!(
            load_checkpoint(db.conn(Default::default()), Some(&first_hash))
                .await
                .is_err()
        );
        Ok(())
    }
}
