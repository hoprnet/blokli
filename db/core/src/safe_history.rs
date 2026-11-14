use std::collections::HashSet;

use async_trait::async_trait;
use blokli_db_entity::{
    hopr_safe_contract, hopr_safe_contract_state, hopr_safe_owner_state, hopr_safe_threshold_state,
    prelude::{HoprSafeContract, HoprSafeContractState, HoprSafeOwnerState, HoprSafeThresholdState},
};
use hopr_types::primitive::prelude::{Address, ToHex};
use sea_orm::{ColumnTrait, Condition, ConnectionTrait, DbErr, EntityTrait, QueryFilter, QueryOrder, Set};
use sea_query::{Expr, OnConflict, Query};

use crate::{
    BlokliDb, BlokliDbGeneralModelOperations, DbSqlError, OptTx, Result, safe_contracts::BlokliDbSafeContractOperations,
};

fn ignore_record_not_inserted<T>(result: std::result::Result<T, DbErr>) -> Result<()> {
    match result {
        Ok(_) | Err(DbErr::RecordNotInserted) => Ok(()),
        Err(err) => Err(err.into()),
    }
}

async fn get_safe_contract<C: ConnectionTrait>(
    conn: &C,
    safe_address: Address,
) -> Result<Option<hopr_safe_contract::Model>> {
    HoprSafeContract::find()
        .filter(hopr_safe_contract::Column::Address.eq(safe_address.as_ref().to_vec()))
        .one(conn)
        .await
        .map_err(Into::into)
}

async fn require_safe_contract<C: ConnectionTrait>(
    conn: &C,
    safe_address: Address,
) -> Result<hopr_safe_contract::Model> {
    get_safe_contract(conn, safe_address)
        .await?
        .ok_or_else(|| DbSqlError::EntityNotFound(format!("safe contract not found for {}", safe_address.to_hex())))
}

/// Highest owner snapshot block across all state rows of a Safe.
///
/// The value lives on `hopr_safe_contract_state`, which is versioned by position. Taking the
/// maximum keeps it correct when a later state row (e.g. a module address update) is appended.
async fn owners_synced_block_of<C: ConnectionTrait>(conn: &C, safe_contract_id: i64) -> Result<Option<i64>> {
    Ok(HoprSafeContractState::find()
        .filter(hopr_safe_contract_state::Column::HoprSafeContractId.eq(safe_contract_id))
        .all(conn)
        .await?
        .into_iter()
        .filter_map(|state| state.owners_synced_block)
        .max())
}

/// Writes the owner row of `(safe, owner)` unless the stored row is at or past the given limit.
///
/// `guard` is the SQL condition on the stored row that must hold for the row to be replaced.
async fn write_owner_row<C: ConnectionTrait>(
    conn: &C,
    safe_contract_id: i64,
    owner_address: Address,
    is_current_owner: bool,
    position: (i64, i64, i64),
    guard: Expr,
) -> Result<()> {
    ignore_record_not_inserted(
        HoprSafeOwnerState::insert(hopr_safe_owner_state::ActiveModel {
            hopr_safe_contract_id: Set(safe_contract_id),
            owner_address: Set(owner_address.as_ref().to_vec()),
            is_current_owner: Set(is_current_owner),
            published_block: Set(position.0),
            published_tx_index: Set(position.1),
            published_log_index: Set(position.2),
            ..Default::default()
        })
        .on_conflict(
            OnConflict::columns([
                hopr_safe_owner_state::Column::HoprSafeContractId,
                hopr_safe_owner_state::Column::OwnerAddress,
            ])
            .update_columns([
                hopr_safe_owner_state::Column::IsCurrentOwner,
                hopr_safe_owner_state::Column::PublishedBlock,
                hopr_safe_owner_state::Column::PublishedTxIndex,
                hopr_safe_owner_state::Column::PublishedLogIndex,
            ])
            .action_and_where(guard)
            .to_owned(),
        )
        .exec(conn)
        .await,
    )
}

async fn write_threshold_row<C: ConnectionTrait>(
    conn: &C,
    safe_contract_id: i64,
    threshold: String,
    position: (i64, i64, i64),
    guard: Expr,
) -> Result<()> {
    ignore_record_not_inserted(
        HoprSafeThresholdState::insert(hopr_safe_threshold_state::ActiveModel {
            hopr_safe_contract_id: Set(safe_contract_id),
            threshold: Set(threshold),
            published_block: Set(position.0),
            published_tx_index: Set(position.1),
            published_log_index: Set(position.2),
            ..Default::default()
        })
        .on_conflict(
            OnConflict::column(hopr_safe_threshold_state::Column::HoprSafeContractId)
                .update_columns([
                    hopr_safe_threshold_state::Column::Threshold,
                    hopr_safe_threshold_state::Column::PublishedBlock,
                    hopr_safe_threshold_state::Column::PublishedTxIndex,
                    hopr_safe_threshold_state::Column::PublishedLogIndex,
                ])
                .action_and_where(guard)
                .to_owned(),
        )
        .exec(conn)
        .await,
    )
}

/// Matches a stored row whose position is strictly before `position`.
fn stored_position_before(table: &str, position: (i64, i64, i64)) -> Expr {
    Expr::cust_with_values(
        format!("({table}.published_block, {table}.published_tx_index, {table}.published_log_index) < (?, ?, ?)"),
        [position.0, position.1, position.2],
    )
}

/// Matches a stored row published at or before `block`.
fn stored_block_at_or_before(table: &str, block: i64) -> Expr {
    Expr::cust_with_values(format!("{table}.published_block <= ?"), [block])
}

fn block_to_i64(block: u64, what: &str) -> Result<i64> {
    i64::try_from(block).map_err(|_| DbSqlError::Construction(format!("{what} {block} does not fit into i64")))
}

#[async_trait]
pub trait BlokliDbSafeHistoryOperations: BlokliDbGeneralModelOperations + BlokliDbSafeContractOperations {
    /// Records the membership of `owner_address` as of the given event position.
    ///
    /// Keeps a single row per `(safe, owner)`. An event at or before the stored position is
    /// ignored, so replaying older events cannot revive a removed owner. Removed owners stay as
    /// `is_current_owner = false`.
    ///
    /// This does not look at `owners_synced_block`; callers applying live events check
    /// [`Self::get_safe_owners_synced_block`] first.
    #[allow(clippy::too_many_arguments)]
    async fn upsert_safe_owner_state<'a>(
        &'a self,
        tx: OptTx<'a>,
        safe_address: Address,
        owner_address: Address,
        is_current_owner: bool,
        block: u64,
        tx_index: u64,
        log_index: u64,
    ) -> Result<()>;

    /// Records the threshold of a Safe as of the given event position (one row per Safe).
    /// An event at or before the stored position is ignored.
    async fn upsert_safe_threshold_state<'a>(
        &'a self,
        tx: OptTx<'a>,
        safe_address: Address,
        threshold: String,
        block: u64,
        tx_index: u64,
        log_index: u64,
    ) -> Result<()>;

    /// Returns the current owners of a Safe in ascending address order.
    async fn get_safe_owners<'a>(&'a self, tx: OptTx<'a>, safe_address: Address) -> Result<Vec<Address>>;

    /// Returns the current threshold of a Safe, if known.
    async fn get_safe_threshold<'a>(&'a self, tx: OptTx<'a>, safe_address: Address) -> Result<Option<String>>;

    /// Returns the block the owner snapshot of a Safe is pinned at, or `None` before the first snapshot.
    async fn get_safe_owners_synced_block<'a>(&'a self, tx: OptTx<'a>, safe_address: Address) -> Result<Option<u64>>;

    /// Stores an RPC snapshot of owners and threshold taken at `snapshot_block`.
    ///
    /// A no-op when the Safe already has a snapshot at or above `snapshot_block`, so a stale snapshot
    /// cannot overwrite a newer one.
    ///
    /// The owner set of the Safe becomes exactly `owners`, except for rows written by live events
    /// above `snapshot_block`, which are newer than the snapshot and kept. Owners missing from the
    /// snapshot are marked as removed. `owners_synced_block` is advanced to `snapshot_block`.
    async fn apply_safe_owner_snapshot<'a>(
        &'a self,
        tx: OptTx<'a>,
        safe_address: Address,
        owners: Vec<Address>,
        threshold: String,
        snapshot_block: u64,
    ) -> Result<()>;

    /// Returns the Safes without an owner snapshot at or above `snapshot_block`.
    ///
    /// This makes the snapshot refresh resumable: Safes already refreshed are not returned.
    async fn get_safes_pending_owner_snapshot<'a>(&'a self, tx: OptTx<'a>, snapshot_block: u64)
    -> Result<Vec<Address>>;
}

#[async_trait]
impl BlokliDbSafeHistoryOperations for BlokliDb {
    #[allow(clippy::too_many_arguments)]
    async fn upsert_safe_owner_state<'a>(
        &'a self,
        tx: OptTx<'a>,
        safe_address: Address,
        owner_address: Address,
        is_current_owner: bool,
        block: u64,
        tx_index: u64,
        log_index: u64,
    ) -> Result<()> {
        let tx = self.nest_transaction(tx).await?;
        let safe = require_safe_contract(tx.as_ref(), safe_address).await?;
        let position = (block.cast_signed(), tx_index.cast_signed(), log_index.cast_signed());
        write_owner_row(
            tx.as_ref(),
            safe.id,
            owner_address,
            is_current_owner,
            position,
            stored_position_before("hopr_safe_owner_state", position),
        )
        .await?;
        tx.commit().await?;
        Ok(())
    }

    async fn upsert_safe_threshold_state<'a>(
        &'a self,
        tx: OptTx<'a>,
        safe_address: Address,
        threshold: String,
        block: u64,
        tx_index: u64,
        log_index: u64,
    ) -> Result<()> {
        let tx = self.nest_transaction(tx).await?;
        let safe = require_safe_contract(tx.as_ref(), safe_address).await?;
        let position = (block.cast_signed(), tx_index.cast_signed(), log_index.cast_signed());
        write_threshold_row(
            tx.as_ref(),
            safe.id,
            threshold,
            position,
            stored_position_before("hopr_safe_threshold_state", position),
        )
        .await?;
        tx.commit().await?;
        Ok(())
    }

    async fn get_safe_owners<'a>(&'a self, tx: OptTx<'a>, safe_address: Address) -> Result<Vec<Address>> {
        let nested = self.nest_transaction(tx).await?;
        let Some(safe) = get_safe_contract(nested.as_ref(), safe_address).await? else {
            nested.commit().await?;
            return Ok(Vec::new());
        };

        let mut owner_bytes = HoprSafeOwnerState::find()
            .filter(hopr_safe_owner_state::Column::HoprSafeContractId.eq(safe.id))
            .filter(hopr_safe_owner_state::Column::IsCurrentOwner.eq(true))
            .all(nested.as_ref())
            .await?
            .into_iter()
            .map(|state| state.owner_address)
            .collect::<Vec<_>>();

        owner_bytes.sort();
        let owners = owner_bytes
            .into_iter()
            .map(|owner| {
                Address::try_from(owner.as_slice())
                    .map_err(|e| DbSqlError::Construction(format!("invalid safe owner address length: {e}")))
            })
            .collect::<Result<Vec<_>>>()?;

        nested.commit().await?;
        Ok(owners)
    }

    async fn get_safe_threshold<'a>(&'a self, tx: OptTx<'a>, safe_address: Address) -> Result<Option<String>> {
        let nested = self.nest_transaction(tx).await?;
        let Some(safe) = get_safe_contract(nested.as_ref(), safe_address).await? else {
            nested.commit().await?;
            return Ok(None);
        };
        let threshold = HoprSafeThresholdState::find()
            .filter(hopr_safe_threshold_state::Column::HoprSafeContractId.eq(safe.id))
            .one(nested.as_ref())
            .await?
            .map(|state| state.threshold);
        nested.commit().await?;
        Ok(threshold)
    }

    async fn get_safe_owners_synced_block<'a>(&'a self, tx: OptTx<'a>, safe_address: Address) -> Result<Option<u64>> {
        let nested = self.nest_transaction(tx).await?;
        let Some(safe) = get_safe_contract(nested.as_ref(), safe_address).await? else {
            nested.commit().await?;
            return Ok(None);
        };
        let synced = owners_synced_block_of(nested.as_ref(), safe.id).await?;
        nested.commit().await?;
        Ok(synced.map(i64::cast_unsigned))
    }

    async fn apply_safe_owner_snapshot<'a>(
        &'a self,
        tx: OptTx<'a>,
        safe_address: Address,
        owners: Vec<Address>,
        threshold: String,
        snapshot_block: u64,
    ) -> Result<()> {
        let snapshot_block = block_to_i64(snapshot_block, "snapshot block")?;
        let position = (snapshot_block, 0, 0);

        let tx = self.nest_transaction(tx).await?;
        let safe = require_safe_contract(tx.as_ref(), safe_address).await?;

        // An older snapshot must never land after a newer one: it would revive owners removed since.
        if owners_synced_block_of(tx.as_ref(), safe.id)
            .await?
            .is_some_and(|synced| synced >= snapshot_block)
        {
            tx.commit().await?;
            return Ok(());
        }

        let snapshot_owners = owners
            .iter()
            .map(|owner| owner.as_ref().to_vec())
            .collect::<HashSet<_>>();
        for owner in &owners {
            write_owner_row(
                tx.as_ref(),
                safe.id,
                *owner,
                true,
                position,
                stored_block_at_or_before("hopr_safe_owner_state", snapshot_block),
            )
            .await?;
        }

        // Owners the snapshot does not contain are removed, unless a newer live event wrote them.
        let stale = HoprSafeOwnerState::find()
            .filter(hopr_safe_owner_state::Column::HoprSafeContractId.eq(safe.id))
            .filter(hopr_safe_owner_state::Column::IsCurrentOwner.eq(true))
            .filter(hopr_safe_owner_state::Column::PublishedBlock.lte(snapshot_block))
            .all(tx.as_ref())
            .await?
            .into_iter()
            .filter(|state| !snapshot_owners.contains(&state.owner_address));
        for state in stale {
            let owner = Address::try_from(state.owner_address.as_slice())
                .map_err(|e| DbSqlError::Construction(format!("invalid safe owner address length: {e}")))?;
            write_owner_row(
                tx.as_ref(),
                safe.id,
                owner,
                false,
                position,
                stored_block_at_or_before("hopr_safe_owner_state", snapshot_block),
            )
            .await?;
        }

        write_threshold_row(
            tx.as_ref(),
            safe.id,
            threshold,
            position,
            stored_block_at_or_before("hopr_safe_threshold_state", snapshot_block),
        )
        .await?;

        HoprSafeContractState::update_many()
            .col_expr(
                hopr_safe_contract_state::Column::OwnersSyncedBlock,
                Expr::value(snapshot_block),
            )
            .filter(hopr_safe_contract_state::Column::HoprSafeContractId.eq(safe.id))
            .filter(
                Condition::any()
                    .add(hopr_safe_contract_state::Column::OwnersSyncedBlock.is_null())
                    .add(hopr_safe_contract_state::Column::OwnersSyncedBlock.lt(snapshot_block)),
            )
            .exec(tx.as_ref())
            .await?;

        tx.commit().await?;
        Ok(())
    }

    async fn get_safes_pending_owner_snapshot<'a>(
        &'a self,
        tx: OptTx<'a>,
        snapshot_block: u64,
    ) -> Result<Vec<Address>> {
        let snapshot_block = block_to_i64(snapshot_block, "snapshot block")?;
        let nested = self.nest_transaction(tx).await?;

        let synced = Query::select()
            .column(hopr_safe_contract_state::Column::HoprSafeContractId)
            .from(HoprSafeContractState)
            .and_where(hopr_safe_contract_state::Column::OwnersSyncedBlock.gte(snapshot_block))
            .to_owned();
        // A Safe identity without any state row is incomplete (no module address or chain key, and not
        // served by the current views). There is no row to hold `owners_synced_block`, so it could never
        // leave the pending list: it is not listed until its first state row exists.
        let with_state = Query::select()
            .column(hopr_safe_contract_state::Column::HoprSafeContractId)
            .from(HoprSafeContractState)
            .to_owned();
        let pending = HoprSafeContract::find()
            .filter(hopr_safe_contract::Column::Id.in_subquery(with_state))
            .filter(hopr_safe_contract::Column::Id.not_in_subquery(synced))
            .order_by_asc(hopr_safe_contract::Column::Id)
            .all(nested.as_ref())
            .await?
            .into_iter()
            .map(|contract| {
                Address::try_from(contract.address.as_slice())
                    .map_err(|e| DbSqlError::Construction(format!("invalid safe address length: {e}")))
            })
            .collect::<Result<Vec<_>>>()?;

        nested.commit().await?;
        Ok(pending)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{db::BlokliDb, safe_contracts::BlokliDbSafeContractOperations};

    fn random_address() -> Address {
        Address::from(hopr_types::crypto_random::random_bytes())
    }

    async fn setup() -> anyhow::Result<(BlokliDb, Address)> {
        let db = BlokliDb::new_in_memory().await?;
        let safe_address = random_address();
        db.create_safe_contract(None, safe_address, random_address(), random_address(), 100, 0, 0)
            .await?;
        Ok((db, safe_address))
    }

    #[tokio::test]
    async fn test_get_safe_owners_tracks_current_membership() -> anyhow::Result<()> {
        let (db, safe_address) = setup().await?;
        let owner_one: Address = "1111111111111111111111111111111111111111".parse()?;
        let owner_two: Address = "2222222222222222222222222222222222222222".parse()?;

        db.upsert_safe_owner_state(None, safe_address, owner_one, true, 101, 0, 0)
            .await?;
        // Replaying the same event is a no-op.
        db.upsert_safe_owner_state(None, safe_address, owner_one, true, 101, 0, 0)
            .await?;
        db.upsert_safe_owner_state(None, safe_address, owner_two, true, 102, 0, 0)
            .await?;
        db.upsert_safe_owner_state(None, safe_address, owner_one, false, 103, 0, 0)
            .await?;

        assert_eq!(db.get_safe_owners(None, safe_address).await?, vec![owner_two]);

        db.upsert_safe_owner_state(None, safe_address, owner_one, true, 104, 0, 0)
            .await?;
        assert_eq!(
            db.get_safe_owners(None, safe_address).await?,
            vec![owner_one, owner_two]
        );

        Ok(())
    }

    #[tokio::test]
    async fn test_owner_state_keeps_a_single_row_per_owner() -> anyhow::Result<()> {
        let (db, safe_address) = setup().await?;
        let owner = random_address();

        for block in 101..106 {
            db.upsert_safe_owner_state(None, safe_address, owner, block % 2 == 1, block, 0, 0)
                .await?;
        }

        let rows = HoprSafeOwnerState::find().all(db.conn(crate::TargetDb::Index)).await?;
        assert_eq!(rows.len(), 1, "history must not accumulate");
        assert_eq!(rows[0].published_block, 105);

        Ok(())
    }

    #[tokio::test]
    async fn test_replayed_added_owner_does_not_revive_a_removed_owner() -> anyhow::Result<()> {
        let (db, safe_address) = setup().await?;
        let owner = random_address();

        db.upsert_safe_owner_state(None, safe_address, owner, false, 120, 1, 2)
            .await?;
        // An older AddedOwner (and one at the same position) arrives after the removal.
        db.upsert_safe_owner_state(None, safe_address, owner, true, 110, 0, 0)
            .await?;
        db.upsert_safe_owner_state(None, safe_address, owner, true, 120, 1, 2)
            .await?;

        assert!(db.get_safe_owners(None, safe_address).await?.is_empty());

        // A genuinely newer re-add is applied.
        db.upsert_safe_owner_state(None, safe_address, owner, true, 120, 1, 3)
            .await?;
        assert_eq!(db.get_safe_owners(None, safe_address).await?, vec![owner]);

        Ok(())
    }

    #[tokio::test]
    async fn test_threshold_state_is_position_guarded() -> anyhow::Result<()> {
        let (db, safe_address) = setup().await?;

        db.upsert_safe_threshold_state(None, safe_address, "2".into(), 110, 0, 0)
            .await?;
        db.upsert_safe_threshold_state(None, safe_address, "1".into(), 105, 0, 0)
            .await?;
        assert_eq!(db.get_safe_threshold(None, safe_address).await?.as_deref(), Some("2"));

        db.upsert_safe_threshold_state(None, safe_address, "3".into(), 111, 0, 0)
            .await?;
        assert_eq!(db.get_safe_threshold(None, safe_address).await?.as_deref(), Some("3"));

        Ok(())
    }

    #[tokio::test]
    async fn test_snapshot_replaces_owner_set_and_records_synced_block() -> anyhow::Result<()> {
        let (db, safe_address) = setup().await?;
        let (a, b, c) = (random_address(), random_address(), random_address());

        assert_eq!(db.get_safe_owners_synced_block(None, safe_address).await?, None);
        assert_eq!(
            db.get_safes_pending_owner_snapshot(None, 200).await?,
            vec![safe_address]
        );

        db.upsert_safe_owner_state(None, safe_address, a, true, 101, 0, 0)
            .await?;
        db.upsert_safe_owner_state(None, safe_address, b, true, 102, 0, 0)
            .await?;

        db.apply_safe_owner_snapshot(None, safe_address, vec![b, c], "2".into(), 200)
            .await?;

        let mut expected = vec![b, c];
        expected.sort();
        assert_eq!(db.get_safe_owners(None, safe_address).await?, expected);
        assert_eq!(db.get_safe_threshold(None, safe_address).await?.as_deref(), Some("2"));
        assert_eq!(db.get_safe_owners_synced_block(None, safe_address).await?, Some(200));
        assert!(db.get_safes_pending_owner_snapshot(None, 200).await?.is_empty());
        assert_eq!(
            db.get_safes_pending_owner_snapshot(None, 201).await?,
            vec![safe_address]
        );

        // The removed owner is kept as a tombstone: replaying its AddedOwner has no effect.
        db.upsert_safe_owner_state(None, safe_address, a, true, 101, 0, 0)
            .await?;
        assert!(!db.get_safe_owners(None, safe_address).await?.contains(&a));

        Ok(())
    }

    #[tokio::test]
    async fn test_snapshot_keeps_rows_written_above_the_snapshot_block() -> anyhow::Result<()> {
        let (db, safe_address) = setup().await?;
        let (a, b) = (random_address(), random_address());

        // A live event above the snapshot block landed before the snapshot was stored.
        db.upsert_safe_owner_state(None, safe_address, b, true, 205, 0, 0)
            .await?;
        db.upsert_safe_threshold_state(None, safe_address, "3".into(), 206, 0, 0)
            .await?;

        db.apply_safe_owner_snapshot(None, safe_address, vec![a], "1".into(), 200)
            .await?;

        let mut expected = vec![a, b];
        expected.sort();
        assert_eq!(db.get_safe_owners(None, safe_address).await?, expected);
        assert_eq!(db.get_safe_threshold(None, safe_address).await?.as_deref(), Some("3"));

        Ok(())
    }

    #[tokio::test]
    async fn test_owners_synced_block_survives_a_new_state_row() -> anyhow::Result<()> {
        let (db, safe_address) = setup().await?;

        db.apply_safe_owner_snapshot(None, safe_address, vec![random_address()], "1".into(), 150)
            .await?;
        db.update_safe_module_address(None, safe_address, random_address(), 160, 0, 0)
            .await?;

        assert_eq!(db.get_safe_owners_synced_block(None, safe_address).await?, Some(150));

        Ok(())
    }
    #[tokio::test]
    async fn test_older_snapshot_after_a_newer_one_is_a_no_op() -> anyhow::Result<()> {
        let (db, safe_address) = setup().await?;
        let (kept, removed_later) = (random_address(), random_address());

        db.apply_safe_owner_snapshot(None, safe_address, vec![kept], "2".into(), 210)
            .await?;
        // A read pinned earlier, when `removed_later` was still an owner, arrives afterwards.
        db.apply_safe_owner_snapshot(None, safe_address, vec![kept, removed_later], "1".into(), 200)
            .await?;
        // Applying at the same block again changes nothing either.
        db.apply_safe_owner_snapshot(None, safe_address, vec![], "9".into(), 210)
            .await?;

        assert_eq!(db.get_safe_owners(None, safe_address).await?, vec![kept]);
        assert_eq!(db.get_safe_threshold(None, safe_address).await?.as_deref(), Some("2"));
        assert_eq!(db.get_safe_owners_synced_block(None, safe_address).await?, Some(210));

        Ok(())
    }
    #[tokio::test]
    async fn test_safe_without_a_state_row_is_not_listed_as_pending() -> anyhow::Result<()> {
        let (db, safe_address) = setup().await?;
        let incomplete = random_address();
        HoprSafeContract::insert(hopr_safe_contract::ActiveModel {
            address: Set(incomplete.as_ref().to_vec()),
            ..Default::default()
        })
        .exec(db.conn(crate::TargetDb::Index))
        .await?;

        assert_eq!(
            db.get_safes_pending_owner_snapshot(None, 200).await?,
            vec![safe_address]
        );

        Ok(())
    }
}
