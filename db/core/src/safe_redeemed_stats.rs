use std::collections::{HashMap, HashSet};

use async_trait::async_trait;
pub use blokli_db_entity::conversions::redemptions_aggregation::AggregatedRedeemedStats;
use blokli_db_entity::{
    conversions::redemptions_aggregation::fetch_aggregated_redeemed_stats,
    hopr_safe_redeemed_stat_event, hopr_safe_redeemed_stats,
    prelude::{HoprSafeRedeemedStatEvent, HoprSafeRedeemedStats},
};
use hopr_types::primitive::{
    prelude::{Address, HoprBalance, ToHex},
    traits::IntoEndian,
};
use sea_orm::{ActiveModelTrait, ColumnTrait, ConnectionTrait, DbBackend, EntityTrait, QueryFilter, Set};
use sea_query::{Condition, OnConflict};

use crate::{
    BlokliDbGeneralModelOperations, OptTx, TargetDb,
    db::BlokliDb,
    errors::{DbSqlError, Result},
    with_opt_transaction,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SafeRedeemedStatsEntry {
    pub safe_address: Address,
    pub node_address: Address,
    pub redeemed_amount: HoprBalance,
    pub redemption_count: u64,
    pub last_redeemed_block: u64,
    pub last_redeemed_tx_index: u64,
    pub last_redeemed_log_index: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StagedSafeRedeemedStatMutation {
    pub safe_address: Address,
    pub node_address: Address,
    pub redeemed_amount: HoprBalance,
    pub block: u64,
    pub tx_index: u64,
    pub log_index: u64,
}

#[derive(Clone, Debug)]
struct PreparedSafeRedeemedStatMutation {
    safe_address: Vec<u8>,
    node_address: Vec<u8>,
    redeemed_amount: Vec<u8>,
    block: i64,
    tx_index: i64,
    log_index: i64,
}

#[derive(Clone, Debug)]
struct GroupedRedeemedDelta {
    redeemed_amount: HoprBalance,
    redemption_count: i64,
    last_redeemed_block: i64,
    last_redeemed_tx_index: i64,
    last_redeemed_log_index: i64,
}

type SafeRedeemedStatEventIdentity = (Vec<u8>, Vec<u8>, i64, i64, i64);
type SafeRedeemedStatPair = (Vec<u8>, Vec<u8>);

const SQLITE_SAFE_REDEEMED_STATS_BATCH_SIZE: usize = 100;
const DEFAULT_SAFE_REDEEMED_STATS_BATCH_SIZE: usize = 1_000;

fn safe_redeemed_stats_batch_size(backend: DbBackend) -> usize {
    match backend {
        DbBackend::Sqlite => SQLITE_SAFE_REDEEMED_STATS_BATCH_SIZE,
        _ => DEFAULT_SAFE_REDEEMED_STATS_BATCH_SIZE,
    }
}

fn model_to_entry(model: hopr_safe_redeemed_stats::Model) -> Result<SafeRedeemedStatsEntry> {
    let safe_address: Address = Address::try_from(model.safe_address.as_slice())?;
    let node_address: Address = Address::try_from(model.node_address.as_slice())?;
    let redeemed_amount: HoprBalance = HoprBalance::from_be_bytes(model.redeemed_amount.as_slice());

    Ok(SafeRedeemedStatsEntry {
        safe_address,
        node_address,
        redeemed_amount,
        redemption_count: u64::try_from(model.redemption_count).map_err(|_| DbSqlError::DecodingError)?,
        last_redeemed_block: u64::try_from(model.last_redeemed_block).map_err(|_| DbSqlError::DecodingError)?,
        last_redeemed_tx_index: u64::try_from(model.last_redeemed_tx_index).map_err(|_| DbSqlError::DecodingError)?,
        last_redeemed_log_index: u64::try_from(model.last_redeemed_log_index).map_err(|_| DbSqlError::DecodingError)?,
    })
}

fn safe_redeemed_stat_event_identity(mutation: &PreparedSafeRedeemedStatMutation) -> SafeRedeemedStatEventIdentity {
    (
        mutation.safe_address.clone(),
        mutation.node_address.clone(),
        mutation.block,
        mutation.tx_index,
        mutation.log_index,
    )
}

fn safe_redeemed_stat_pair(mutation: &PreparedSafeRedeemedStatMutation) -> SafeRedeemedStatPair {
    (mutation.safe_address.clone(), mutation.node_address.clone())
}

fn safe_redeemed_stat_pair_filter(pairs: impl IntoIterator<Item = SafeRedeemedStatPair>) -> Condition {
    pairs
        .into_iter()
        .fold(Condition::any(), |condition, (safe_address, node_address)| {
            condition.add(
                Condition::all()
                    .add(hopr_safe_redeemed_stats::Column::SafeAddress.eq(safe_address))
                    .add(hopr_safe_redeemed_stats::Column::NodeAddress.eq(node_address)),
            )
        })
}

fn safe_redeemed_stat_event_identity_filter(
    identities: impl IntoIterator<Item = SafeRedeemedStatEventIdentity>,
) -> Condition {
    identities.into_iter().fold(
        Condition::any(),
        |condition, (safe_address, node_address, block, tx_index, log_index)| {
            condition.add(
                Condition::all()
                    .add(hopr_safe_redeemed_stat_event::Column::SafeAddress.eq(safe_address))
                    .add(hopr_safe_redeemed_stat_event::Column::NodeAddress.eq(node_address))
                    .add(hopr_safe_redeemed_stat_event::Column::PublishedBlock.eq(block))
                    .add(hopr_safe_redeemed_stat_event::Column::PublishedTxIndex.eq(tx_index))
                    .add(hopr_safe_redeemed_stat_event::Column::PublishedLogIndex.eq(log_index)),
            )
        },
    )
}

fn is_later_position(current: (i64, i64, i64), candidate: (i64, i64, i64)) -> bool {
    candidate > current
}

fn prepare_staged_safe_redeemed_stat_mutations(
    mutations: &[StagedSafeRedeemedStatMutation],
) -> Result<Vec<PreparedSafeRedeemedStatMutation>> {
    let mut identities = HashSet::with_capacity(mutations.len());
    let mut prepared = Vec::with_capacity(mutations.len());

    for mutation in mutations {
        let block = i64::try_from(mutation.block)
            .map_err(|_| DbSqlError::LogicalError(format!("block {} exceeds i64", mutation.block)))?;
        let tx_index = i64::try_from(mutation.tx_index)
            .map_err(|_| DbSqlError::LogicalError(format!("tx index {} exceeds i64", mutation.tx_index)))?;
        let log_index = i64::try_from(mutation.log_index)
            .map_err(|_| DbSqlError::LogicalError(format!("log index {} exceeds i64", mutation.log_index)))?;

        let prepared_mutation = PreparedSafeRedeemedStatMutation {
            safe_address: mutation.safe_address.as_ref().to_vec(),
            node_address: mutation.node_address.as_ref().to_vec(),
            redeemed_amount: mutation.redeemed_amount.to_be_bytes().to_vec(),
            block,
            tx_index,
            log_index,
        };

        let identity = safe_redeemed_stat_event_identity(&prepared_mutation);
        if !identities.insert(identity) {
            return Err(DbSqlError::Construction(format!(
                "duplicate staged safe redeemed stat mutation for safe {} and node {} at {}:{}:{}",
                mutation.safe_address.to_hex(),
                mutation.node_address.to_hex(),
                mutation.block,
                mutation.tx_index,
                mutation.log_index
            )));
        }

        prepared.push(prepared_mutation);
    }

    Ok(prepared)
}

async fn insert_safe_redeemed_stat_event_anchors<C: ConnectionTrait>(
    conn: &C,
    mutations: &[PreparedSafeRedeemedStatMutation],
    batch_size: usize,
) -> Result<Vec<hopr_safe_redeemed_stat_event::Model>> {
    let mut inserted = Vec::new();

    for chunk in mutations.chunks(batch_size) {
        let inserted_chunk = HoprSafeRedeemedStatEvent::insert_many(chunk.iter().cloned().map(|mutation| {
            hopr_safe_redeemed_stat_event::ActiveModel {
                safe_address: Set(mutation.safe_address),
                node_address: Set(mutation.node_address),
                redeemed_amount: Set(mutation.redeemed_amount),
                published_block: Set(mutation.block),
                published_tx_index: Set(mutation.tx_index),
                published_log_index: Set(mutation.log_index),
                ..Default::default()
            }
        }))
        .on_conflict(
            OnConflict::columns([
                hopr_safe_redeemed_stat_event::Column::SafeAddress,
                hopr_safe_redeemed_stat_event::Column::NodeAddress,
                hopr_safe_redeemed_stat_event::Column::PublishedBlock,
                hopr_safe_redeemed_stat_event::Column::PublishedTxIndex,
                hopr_safe_redeemed_stat_event::Column::PublishedLogIndex,
            ])
            .do_nothing()
            .to_owned(),
        )
        .exec_with_returning(conn)
        .await?;

        inserted.extend(inserted_chunk);
    }

    Ok(inserted)
}

async fn load_safe_redeemed_stat_event_anchors<C: ConnectionTrait>(
    conn: &C,
    identities: &[SafeRedeemedStatEventIdentity],
    batch_size: usize,
) -> Result<Vec<hopr_safe_redeemed_stat_event::Model>> {
    let mut stored = Vec::new();

    for chunk in identities.chunks(batch_size) {
        let stored_chunk = HoprSafeRedeemedStatEvent::find()
            .filter(safe_redeemed_stat_event_identity_filter(chunk.iter().cloned()))
            .all(conn)
            .await?;
        stored.extend(stored_chunk);
    }

    Ok(stored)
}

async fn load_safe_redeemed_stats_models<C: ConnectionTrait>(
    conn: &C,
    pairs: &[SafeRedeemedStatPair],
    batch_size: usize,
) -> Result<Vec<hopr_safe_redeemed_stats::Model>> {
    let mut stored = Vec::new();

    for chunk in pairs.chunks(batch_size) {
        let stored_chunk = HoprSafeRedeemedStats::find()
            .filter(safe_redeemed_stat_pair_filter(chunk.iter().cloned()))
            .all(conn)
            .await?;
        stored.extend(stored_chunk);
    }

    Ok(stored)
}

fn group_inserted_safe_redeemed_stat_events(
    inserted_events: Vec<hopr_safe_redeemed_stat_event::Model>,
) -> Result<HashMap<SafeRedeemedStatPair, GroupedRedeemedDelta>> {
    let mut grouped: HashMap<SafeRedeemedStatPair, GroupedRedeemedDelta> = HashMap::new();

    for event in inserted_events {
        let pair = (event.safe_address, event.node_address);
        let redeemed_amount = HoprBalance::from_be_bytes(event.redeemed_amount.as_slice());
        let position = (
            event.published_block,
            event.published_tx_index,
            event.published_log_index,
        );

        if let Some(delta) = grouped.get_mut(&pair) {
            delta.redeemed_amount += redeemed_amount;
            delta.redemption_count = delta
                .redemption_count
                .checked_add(1)
                .ok_or_else(|| DbSqlError::LogicalError("safe redemption count overflow".to_string()))?;

            if is_later_position(
                (
                    delta.last_redeemed_block,
                    delta.last_redeemed_tx_index,
                    delta.last_redeemed_log_index,
                ),
                position,
            ) {
                delta.last_redeemed_block = position.0;
                delta.last_redeemed_tx_index = position.1;
                delta.last_redeemed_log_index = position.2;
            }
        } else {
            grouped.insert(
                pair,
                GroupedRedeemedDelta {
                    redeemed_amount,
                    redemption_count: 1,
                    last_redeemed_block: position.0,
                    last_redeemed_tx_index: position.1,
                    last_redeemed_log_index: position.2,
                },
            );
        }
    }

    Ok(grouped)
}

async fn persist_grouped_safe_redeemed_stats<C: ConnectionTrait>(
    conn: &C,
    grouped_deltas: HashMap<SafeRedeemedStatPair, GroupedRedeemedDelta>,
) -> Result<()> {
    if grouped_deltas.is_empty() {
        return Ok(());
    }

    let pairs = grouped_deltas.keys().cloned().collect::<Vec<_>>();
    let batch_size = safe_redeemed_stats_batch_size(conn.get_database_backend());
    let existing_models = load_safe_redeemed_stats_models(conn, &pairs, batch_size).await?;

    let existing_by_pair = existing_models
        .into_iter()
        .map(|model| ((model.safe_address.clone(), model.node_address.clone()), model))
        .collect::<HashMap<_, _>>();

    for (pair, delta) in grouped_deltas {
        match existing_by_pair.get(&pair).cloned() {
            Some(model) => {
                let current_amount = HoprBalance::from_be_bytes(model.redeemed_amount.as_slice());
                let current_position = (
                    model.last_redeemed_block,
                    model.last_redeemed_tx_index,
                    model.last_redeemed_log_index,
                );
                let delta_position = (
                    delta.last_redeemed_block,
                    delta.last_redeemed_tx_index,
                    delta.last_redeemed_log_index,
                );

                let (last_redeemed_block, last_redeemed_tx_index, last_redeemed_log_index) =
                    if is_later_position(current_position, delta_position) {
                        delta_position
                    } else {
                        current_position
                    };

                let redemption_count = model
                    .redemption_count
                    .checked_add(delta.redemption_count)
                    .ok_or_else(|| DbSqlError::LogicalError("safe redemption count overflow".to_string()))?;

                let mut active: hopr_safe_redeemed_stats::ActiveModel = model.into();
                active.redeemed_amount = Set((current_amount + delta.redeemed_amount).to_be_bytes().to_vec());
                active.redemption_count = Set(redemption_count);
                active.last_redeemed_block = Set(last_redeemed_block);
                active.last_redeemed_tx_index = Set(last_redeemed_tx_index);
                active.last_redeemed_log_index = Set(last_redeemed_log_index);
                active.update(conn).await?;
            }
            None => {
                hopr_safe_redeemed_stats::ActiveModel {
                    safe_address: Set(pair.0),
                    node_address: Set(pair.1),
                    redeemed_amount: Set(delta.redeemed_amount.to_be_bytes().to_vec()),
                    redemption_count: Set(delta.redemption_count),
                    last_redeemed_block: Set(delta.last_redeemed_block),
                    last_redeemed_tx_index: Set(delta.last_redeemed_tx_index),
                    last_redeemed_log_index: Set(delta.last_redeemed_log_index),
                    ..Default::default()
                }
                .insert(conn)
                .await?;
            }
        }
    }

    Ok(())
}

async fn load_safe_redeemed_stats_entries<C: ConnectionTrait>(
    conn: &C,
    pairs: Vec<SafeRedeemedStatPair>,
) -> Result<Vec<SafeRedeemedStatsEntry>> {
    if pairs.is_empty() {
        return Ok(Vec::new());
    }

    let batch_size = safe_redeemed_stats_batch_size(conn.get_database_backend());
    let models = load_safe_redeemed_stats_models(conn, &pairs, batch_size).await?;

    let models_by_pair = models
        .into_iter()
        .map(|model| ((model.safe_address.clone(), model.node_address.clone()), model))
        .collect::<HashMap<_, _>>();

    let mut entries = Vec::with_capacity(pairs.len());
    for pair in pairs {
        let model = models_by_pair.get(&pair).cloned().ok_or_else(|| {
            DbSqlError::EntityNotFound(format!(
                "safe redeemed stats not found for safe 0x{} and node 0x{}",
                hex::encode(&pair.0),
                hex::encode(&pair.1)
            ))
        })?;
        entries.push(model_to_entry(model)?);
    }

    entries.sort_by(|left, right| {
        left.safe_address
            .as_ref()
            .cmp(right.safe_address.as_ref())
            .then_with(|| left.node_address.as_ref().cmp(right.node_address.as_ref()))
    });

    Ok(entries)
}

async fn persist_staged_safe_ticket_redemptions_in<C: ConnectionTrait>(
    conn: &C,
    mutations: Vec<StagedSafeRedeemedStatMutation>,
) -> Result<Vec<SafeRedeemedStatsEntry>> {
    if mutations.is_empty() {
        return Ok(Vec::new());
    }

    let prepared_mutations = prepare_staged_safe_redeemed_stat_mutations(&mutations)?;
    let pairs = prepared_mutations
        .iter()
        .map(safe_redeemed_stat_pair)
        .collect::<HashSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    let batch_size = safe_redeemed_stats_batch_size(conn.get_database_backend());

    let inserted_events = insert_safe_redeemed_stat_event_anchors(conn, &prepared_mutations, batch_size).await?;
    let grouped_deltas = group_inserted_safe_redeemed_stat_events(inserted_events)?;
    persist_grouped_safe_redeemed_stats(conn, grouped_deltas).await?;

    let event_identities = prepared_mutations
        .iter()
        .map(safe_redeemed_stat_event_identity)
        .collect::<Vec<_>>();
    let stored_events = load_safe_redeemed_stat_event_anchors(conn, &event_identities, batch_size).await?;
    if stored_events.len() != prepared_mutations.len() {
        return Err(DbSqlError::EntityNotFound(
            "failed to reconcile safe redeemed stat event anchors after bulk insert".to_string(),
        ));
    }

    load_safe_redeemed_stats_entries(conn, pairs).await
}

async fn record_safe_ticket_redeemed_in<C: ConnectionTrait>(
    conn: &C,
    safe_address: Address,
    node_address: Address,
    redeemed_amount: HoprBalance,
    block: u32,
    tx_index: u32,
    log_index: u32,
) -> Result<SafeRedeemedStatsEntry> {
    let entries = persist_staged_safe_ticket_redemptions_in(
        conn,
        vec![StagedSafeRedeemedStatMutation {
            safe_address,
            node_address,
            redeemed_amount,
            block: u64::from(block),
            tx_index: u64::from(tx_index),
            log_index: u64::from(log_index),
        }],
    )
    .await?;

    entries.into_iter().next().ok_or_else(|| {
        DbSqlError::EntityNotFound(format!(
            "safe redeemed stats not found for safe {} and node {}",
            safe_address.to_hex(),
            node_address.to_hex()
        ))
    })
}

#[async_trait]
pub trait BlokliDbSafeRedeemedStatsOperations {
    #[allow(clippy::too_many_arguments)]
    async fn record_safe_ticket_redeemed<'a>(
        &'a self,
        tx: OptTx<'a>,
        safe_address: Address,
        destination_node_address: Address,
        redeemed_amount: HoprBalance,
        block: u32,
        tx_index: u32,
        log_index: u32,
    ) -> Result<SafeRedeemedStatsEntry>;

    async fn persist_staged_safe_ticket_redemptions<'a>(
        &'a self,
        tx: OptTx<'a>,
        mutations: Vec<StagedSafeRedeemedStatMutation>,
    ) -> Result<Vec<SafeRedeemedStatsEntry>>;

    async fn get_aggregated_redeemed_stats(
        &self,
        safe_address: Option<Address>,
        node_address: Option<Address>,
    ) -> Result<AggregatedRedeemedStats>;
}

#[async_trait]
impl BlokliDbSafeRedeemedStatsOperations for BlokliDb {
    async fn record_safe_ticket_redeemed<'a>(
        &'a self,
        tx: OptTx<'a>,
        safe_address: Address,
        node_address: Address,
        redeemed_amount: HoprBalance,
        block: u32,
        tx_index: u32,
        log_index: u32,
    ) -> Result<SafeRedeemedStatsEntry> {
        with_opt_transaction(self, tx, TargetDb::Index, |tx| {
            Box::pin(async move {
                record_safe_ticket_redeemed_in(
                    tx.as_ref(),
                    safe_address,
                    node_address,
                    redeemed_amount,
                    block,
                    tx_index,
                    log_index,
                )
                .await
            })
        })
        .await
    }

    async fn persist_staged_safe_ticket_redemptions<'a>(
        &'a self,
        tx: OptTx<'a>,
        mutations: Vec<StagedSafeRedeemedStatMutation>,
    ) -> Result<Vec<SafeRedeemedStatsEntry>> {
        with_opt_transaction(self, tx, TargetDb::Index, |tx| {
            Box::pin(async move { persist_staged_safe_ticket_redemptions_in(tx.as_ref(), mutations).await })
        })
        .await
    }

    async fn get_aggregated_redeemed_stats(
        &self,
        safe_address: Option<Address>,
        node_address: Option<Address>,
    ) -> Result<AggregatedRedeemedStats> {
        Ok(fetch_aggregated_redeemed_stats(self.conn(TargetDb::Index), safe_address, node_address).await?)
    }
}

#[cfg(test)]
mod tests {
    use rand::Rng;
    use sea_orm::{ColumnTrait, EntityTrait, PaginatorTrait, QueryFilter};

    use super::*;
    use crate::TargetDb;

    fn random_address() -> Address {
        let mut rng = rand::rng();
        let mut bytes = [0u8; 20];
        rng.fill_bytes(&mut bytes);
        Address::from(bytes)
    }

    async fn load_stats_entry(
        db: &BlokliDb,
        safe_address: Address,
        node_address: Address,
    ) -> anyhow::Result<Option<SafeRedeemedStatsEntry>> {
        Ok(HoprSafeRedeemedStats::find()
            .filter(hopr_safe_redeemed_stats::Column::SafeAddress.eq(safe_address.as_ref().to_vec()))
            .filter(hopr_safe_redeemed_stats::Column::NodeAddress.eq(node_address.as_ref().to_vec()))
            .one(db.conn(TargetDb::Index))
            .await?
            .map(model_to_entry)
            .transpose()?)
    }

    async fn redeemed_anchor_count(db: &BlokliDb) -> anyhow::Result<u64> {
        Ok(HoprSafeRedeemedStatEvent::find()
            .count(db.conn(TargetDb::Index))
            .await?)
    }

    #[tokio::test]
    async fn test_record_and_get_safe_redeemed_stats() -> anyhow::Result<()> {
        let db = BlokliDb::new_in_memory().await?;
        let safe_address = random_address();
        let node_address = random_address();

        let first = db
            .record_safe_ticket_redeemed(None, safe_address, node_address, HoprBalance::from(10_u64), 100, 1, 1)
            .await?;
        assert_eq!(first.redeemed_amount, HoprBalance::from(10_u64));
        assert_eq!(first.redemption_count, 1);
        assert_eq!(first.node_address, node_address);

        let second = db
            .record_safe_ticket_redeemed(None, safe_address, node_address, HoprBalance::from(5_u64), 110, 2, 3)
            .await?;
        assert_eq!(second.redeemed_amount, HoprBalance::from(15_u64));
        assert_eq!(second.redemption_count, 2);
        assert_eq!(second.node_address, node_address);
        assert_eq!(second.last_redeemed_block, 110);
        assert_eq!(second.last_redeemed_tx_index, 2);
        assert_eq!(second.last_redeemed_log_index, 3);
        assert_eq!(redeemed_anchor_count(&db).await?, 2);

        let loaded = load_stats_entry(&db, safe_address, node_address)
            .await?
            .expect("stats should exist");
        assert_eq!(loaded, second);

        Ok(())
    }

    #[tokio::test]
    async fn test_record_safe_ticket_redeemed_is_replay_safe() -> anyhow::Result<()> {
        let db = BlokliDb::new_in_memory().await?;
        let safe_address = random_address();
        let node_address = random_address();

        let first = db
            .record_safe_ticket_redeemed(None, safe_address, node_address, HoprBalance::from(10_u64), 100, 1, 1)
            .await?;
        let replayed = db
            .record_safe_ticket_redeemed(None, safe_address, node_address, HoprBalance::from(10_u64), 100, 1, 1)
            .await?;

        assert_eq!(first, replayed);
        assert_eq!(replayed.redeemed_amount, HoprBalance::from(10_u64));
        assert_eq!(replayed.redemption_count, 1);
        assert_eq!(redeemed_anchor_count(&db).await?, 1);

        Ok(())
    }

    #[tokio::test]
    async fn test_get_safe_redeemed_stats_missing() -> anyhow::Result<()> {
        let db = BlokliDb::new_in_memory().await?;
        let safe_address = random_address();
        let node_address = random_address();

        let loaded = load_stats_entry(&db, safe_address, node_address).await?;
        assert!(loaded.is_none());

        Ok(())
    }

    #[tokio::test]
    async fn test_record_is_partitioned_by_safe_and_node() -> anyhow::Result<()> {
        let db = BlokliDb::new_in_memory().await?;
        let safe_address = random_address();
        let first_node = random_address();
        let second_node = random_address();

        let first = db
            .record_safe_ticket_redeemed(None, safe_address, first_node, HoprBalance::from(10_u64), 100, 1, 1)
            .await?;
        assert_eq!(first.redeemed_amount, HoprBalance::from(10_u64));
        assert_eq!(first.redemption_count, 1);

        let second = db
            .record_safe_ticket_redeemed(None, safe_address, second_node, HoprBalance::from(3_u64), 101, 1, 2)
            .await?;
        assert_eq!(second.redeemed_amount, HoprBalance::from(3_u64));
        assert_eq!(second.redemption_count, 1);

        let first_updated = db
            .record_safe_ticket_redeemed(None, safe_address, first_node, HoprBalance::from(5_u64), 102, 2, 0)
            .await?;
        assert_eq!(first_updated.redeemed_amount, HoprBalance::from(15_u64));
        assert_eq!(first_updated.redemption_count, 2);

        let loaded_first = load_stats_entry(&db, safe_address, first_node)
            .await?
            .expect("first node stats should exist");
        let loaded_second = load_stats_entry(&db, safe_address, second_node)
            .await?
            .expect("second node stats should exist");

        assert_eq!(loaded_first.redeemed_amount, HoprBalance::from(15_u64));
        assert_eq!(loaded_first.redemption_count, 2);
        assert_eq!(loaded_second.redeemed_amount, HoprBalance::from(3_u64));
        assert_eq!(loaded_second.redemption_count, 1);
        assert_eq!(redeemed_anchor_count(&db).await?, 3);

        Ok(())
    }

    #[tokio::test]
    async fn test_persist_staged_safe_ticket_redemptions_groups_and_is_idempotent() -> anyhow::Result<()> {
        let db = BlokliDb::new_in_memory().await?;
        let safe_address = random_address();
        let first_node = random_address();
        let second_node = random_address();

        let staged = vec![
            StagedSafeRedeemedStatMutation {
                safe_address,
                node_address: first_node,
                redeemed_amount: HoprBalance::from(10_u64),
                block: 100,
                tx_index: 1,
                log_index: 1,
            },
            StagedSafeRedeemedStatMutation {
                safe_address,
                node_address: first_node,
                redeemed_amount: HoprBalance::from(5_u64),
                block: 110,
                tx_index: 2,
                log_index: 3,
            },
            StagedSafeRedeemedStatMutation {
                safe_address,
                node_address: second_node,
                redeemed_amount: HoprBalance::from(3_u64),
                block: 101,
                tx_index: 1,
                log_index: 2,
            },
        ];

        let first = db.persist_staged_safe_ticket_redemptions(None, staged.clone()).await?;
        let replayed = db.persist_staged_safe_ticket_redemptions(None, staged).await?;

        assert_eq!(first, replayed);
        assert_eq!(first.len(), 2);

        let first_node_entry = first
            .iter()
            .find(|entry| entry.node_address == first_node)
            .expect("first node entry should exist");
        assert_eq!(first_node_entry.redeemed_amount, HoprBalance::from(15_u64));
        assert_eq!(first_node_entry.redemption_count, 2);
        assert_eq!(first_node_entry.last_redeemed_block, 110);
        assert_eq!(first_node_entry.last_redeemed_tx_index, 2);
        assert_eq!(first_node_entry.last_redeemed_log_index, 3);

        let second_node_entry = first
            .iter()
            .find(|entry| entry.node_address == second_node)
            .expect("second node entry should exist");
        assert_eq!(second_node_entry.redeemed_amount, HoprBalance::from(3_u64));
        assert_eq!(second_node_entry.redemption_count, 1);

        assert_eq!(redeemed_anchor_count(&db).await?, 3);

        Ok(())
    }

    #[tokio::test]
    async fn test_persist_staged_safe_ticket_redemptions_reconciles_large_sqlite_batches() -> anyhow::Result<()> {
        let db = BlokliDb::new_in_memory().await?;
        let safe_address = random_address();

        let staged = (0_u64..250)
            .map(|index| {
                let mut node_address = [0u8; 20];
                node_address[12..20].copy_from_slice(&index.to_be_bytes());

                StagedSafeRedeemedStatMutation {
                    safe_address,
                    node_address: Address::from(node_address),
                    redeemed_amount: HoprBalance::from(index + 1),
                    block: 100 + index,
                    tx_index: 1,
                    log_index: 0,
                }
            })
            .collect::<Vec<_>>();

        let persisted = db.persist_staged_safe_ticket_redemptions(None, staged).await?;

        assert_eq!(persisted.len(), 250);
        assert_eq!(redeemed_anchor_count(&db).await?, 250);

        Ok(())
    }

    #[tokio::test]
    async fn test_persist_staged_safe_ticket_redemptions_updates_large_existing_sqlite_batches() -> anyhow::Result<()> {
        let db = BlokliDb::new_in_memory().await?;
        let safe_address = random_address();

        let first_batch = (0_u64..600)
            .map(|index| {
                let mut node_address = [0u8; 20];
                node_address[12..20].copy_from_slice(&index.to_be_bytes());

                StagedSafeRedeemedStatMutation {
                    safe_address,
                    node_address: Address::from(node_address),
                    redeemed_amount: HoprBalance::from(1_u64),
                    block: 100 + index,
                    tx_index: 1,
                    log_index: 0,
                }
            })
            .collect::<Vec<_>>();

        let second_batch = (0_u64..600)
            .map(|index| {
                let mut node_address = [0u8; 20];
                node_address[12..20].copy_from_slice(&index.to_be_bytes());

                StagedSafeRedeemedStatMutation {
                    safe_address,
                    node_address: Address::from(node_address),
                    redeemed_amount: HoprBalance::from(2_u64),
                    block: 1_100 + index,
                    tx_index: 2,
                    log_index: 0,
                }
            })
            .collect::<Vec<_>>();

        let first = db.persist_staged_safe_ticket_redemptions(None, first_batch).await?;
        let second = db.persist_staged_safe_ticket_redemptions(None, second_batch).await?;

        assert_eq!(first.len(), 600);
        assert_eq!(second.len(), 600);
        assert!(
            second
                .iter()
                .all(|entry| entry.redeemed_amount == HoprBalance::from(3_u64) && entry.redemption_count == 2)
        );
        assert_eq!(redeemed_anchor_count(&db).await?, 1_200);

        Ok(())
    }
}
