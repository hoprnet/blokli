use std::collections::{HashMap, HashSet};

use async_trait::async_trait;
use blokli_db_entity::{
    errors::DbEntityError,
    log, log_status, log_topic_info,
    prelude::{Log, LogStatus, LogTopicInfo},
};
use hopr_types::{
    crypto::prelude::Hash,
    primitive::prelude::{Address, DateTime, SerializableLog, ToHex, Utc},
};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, Condition, ConnectionTrait, DbErr, EntityTrait, FromQueryResult, IntoActiveModel,
    PaginatorTrait, QueryFilter, QueryOrder, QuerySelect,
    entity::Set,
    query::QueryTrait,
    sea_query::{Expr, OnConflict, Value},
};
use tracing::{error, trace};

use crate::{
    BlokliDbGeneralModelOperations, TargetDb,
    api::{
        errors::{DbError, Result},
        logs::BlokliDbLogOperations,
    },
    db::BlokliDb,
    errors::DbSqlError,
    numeric::{block_range_to_i64, i64_to_u64, log_position_to_i64},
    snapshot::{LOG_INSERT_COLUMNS, LOG_STATUS_INSERT_COLUMNS, import_batch_size},
};

/// Number of bound values a single log position contributes to a statement.
const LOG_POSITION_COLUMNS: usize = 3;

/// Builds a condition matching exactly the given `(block_number, tx_index, log_index)` positions.
fn log_positions_condition(positions: &[(i64, i64, i64)]) -> Condition {
    positions
        .iter()
        .fold(Condition::any(), |condition, (block_number, tx_index, log_index)| {
            condition.add(
                Condition::all()
                    .add(log_status::Column::BlockNumber.eq(*block_number))
                    .add(log_status::Column::TxIndex.eq(*tx_index))
                    .add(log_status::Column::LogIndex.eq(*log_index)),
            )
        })
}

/// Same as [`log_positions_condition`], for the `log` table.
fn log_table_positions_condition(positions: &[(i64, i64, i64)]) -> Condition {
    positions
        .iter()
        .fold(Condition::any(), |condition, (block_number, tx_index, log_index)| {
            condition.add(
                Condition::all()
                    .add(log::Column::BlockNumber.eq(*block_number))
                    .add(log::Column::TxIndex.eq(*tx_index))
                    .add(log::Column::LogIndex.eq(*log_index)),
            )
        })
}

#[derive(FromQueryResult)]
struct BlockNumber {
    block_number: i64,
}

/// Identifier of a stored log together with the position it was stored at.
#[derive(FromQueryResult)]
struct StoredLogPosition {
    id: i64,
    block_number: i64,
    tx_index: i64,
    log_index: i64,
}

#[async_trait]
impl BlokliDbLogOperations for BlokliDb {
    async fn store_log<'a>(&'a self, log: SerializableLog) -> Result<()> {
        match self.store_logs([log].to_vec()).await {
            Ok(results) => {
                if let Some(result) = results.into_iter().next() {
                    result
                } else {
                    panic!("when inserting a log into the db, the result should be a single item")
                }
            }
            Err(e) => Err(e),
        }
    }

    async fn store_logs(&self, logs: Vec<SerializableLog>) -> Result<Vec<Result<()>>> {
        let log_count = logs.len();
        if logs.is_empty() {
            return Ok(Vec::new());
        }

        // Build both ActiveModels up front so a conversion failure cannot leave an orphaned log row
        // without a matching log_status.
        let mut log_models = Vec::with_capacity(log_count);
        let mut status_models = Vec::with_capacity(log_count);
        let mut seen_positions = HashSet::with_capacity(log_count);

        for log in logs {
            let position = log_position_to_i64(log.block_number, log.tx_index, log.log_index).map_err(DbError::from)?;
            let log_model = log::ActiveModel::try_from(log.clone())
                .map_err(DbSqlError::from)
                .map_err(DbError::from)?;
            let status_model = log_status::ActiveModel::try_from(log)
                .map_err(DbSqlError::from)
                .map_err(DbError::from)?;

            // A position repeated inside one call would be collapsed by the database anyway;
            // dropping it here keeps the log_status rows unambiguous.
            if !seen_positions.insert(position) {
                continue;
            }
            log_models.push(log_model);
            status_models.push((position, status_model));
        }

        self.nest_transaction_in_db(None, TargetDb::Logs)
            .await?
            .perform(|tx| {
                Box::pin(async move {
                    let backend = tx.as_ref().get_database_backend();

                    // Insert the batch with as few statements as the backend's bind-parameter
                    // limit allows, leaving already stored logs untouched.
                    for chunk in log_models.chunks(import_batch_size(backend, LOG_INSERT_COLUMNS)) {
                        match Log::insert_many(chunk.to_vec())
                            .on_conflict(
                                OnConflict::columns([
                                    log::Column::LogIndex,
                                    log::Column::TxIndex,
                                    log::Column::BlockNumber,
                                ])
                                .do_nothing()
                                .to_owned(),
                            )
                            .exec_without_returning(tx.as_ref())
                            .await
                        {
                            Ok(_) | Err(DbErr::RecordNotInserted) => {}
                            Err(e) => {
                                error!(error = ?e, "failed to insert logs into db");
                                return Err(DbError::General(e.to_string()));
                            }
                        }
                    }

                    // Read the identifiers back for exactly the positions of this batch. This
                    // covers both the rows just inserted and the ones that were already stored, so
                    // the log_status rows below always point at the right log.
                    let positions = status_models.iter().map(|(position, _)| *position).collect::<Vec<_>>();
                    let mut log_ids = HashMap::with_capacity(positions.len());

                    for chunk in positions.chunks(import_batch_size(backend, LOG_POSITION_COLUMNS)) {
                        let stored = Log::find()
                            .select_only()
                            .columns([
                                log::Column::Id,
                                log::Column::BlockNumber,
                                log::Column::TxIndex,
                                log::Column::LogIndex,
                            ])
                            .filter(log_table_positions_condition(chunk))
                            .into_model::<StoredLogPosition>()
                            .all(tx.as_ref())
                            .await
                            .map_err(|e| {
                                error!(error = ?e, "failed to read back stored log identifiers");
                                DbError::General(e.to_string())
                            })?;

                        log_ids.extend(
                            stored
                                .into_iter()
                                .map(|row| ((row.block_number, row.tx_index, row.log_index), row.id)),
                        );
                    }

                    let mut status_models_with_ids = Vec::with_capacity(status_models.len());
                    for (position, mut status_model) in status_models {
                        let Some(log_id) = log_ids.get(&position).copied() else {
                            let (block_number, tx_index, log_index) = position;
                            error!(block_number, tx_index, log_index, "log not found after insert");
                            return Err(DbError::General(format!(
                                "Log not found: block {block_number}, tx {tx_index}, log {log_index}"
                            )));
                        };
                        status_model.log_id = Set(log_id);
                        status_models_with_ids.push(status_model);
                    }

                    // Statuses of logs already present must not be reset, hence the same
                    // do-nothing conflict handling as before.
                    for chunk in status_models_with_ids.chunks(import_batch_size(backend, LOG_STATUS_INSERT_COLUMNS)) {
                        match LogStatus::insert_many(chunk.to_vec())
                            .on_conflict(
                                OnConflict::columns([
                                    log_status::Column::LogIndex,
                                    log_status::Column::TxIndex,
                                    log_status::Column::BlockNumber,
                                ])
                                .do_nothing()
                                .to_owned(),
                            )
                            .exec_without_returning(tx.as_ref())
                            .await
                        {
                            Ok(_) | Err(DbErr::RecordNotInserted) => {}
                            Err(e) => {
                                error!(error = ?e, "failed to insert log statuses into db");
                                return Err(DbError::General(e.to_string()));
                            }
                        }
                    }

                    Ok(())
                })
            })
            .await?;

        Ok((0..log_count).map(|_| Ok(())).collect())
    }

    async fn get_log(&self, block_number: u64, tx_index: u64, log_index: u64) -> Result<SerializableLog> {
        let (block_number, tx_index, log_index) =
            log_position_to_i64(block_number, tx_index, log_index).map_err(DbError::from)?;
        let query = Log::find()
            .filter(log::Column::BlockNumber.eq(block_number))
            .filter(log::Column::TxIndex.eq(tx_index))
            .filter(log::Column::LogIndex.eq(log_index))
            .find_also_related(LogStatus);

        match query.all(self.conn(TargetDb::Logs)).await {
            Ok(mut res) => {
                if let Some((log, log_status)) = res.pop() {
                    if let Some(status) = log_status {
                        create_log(log, status).map_err(DbError::from)
                    } else {
                        Err(DbError::MissingLogStatus)
                    }
                } else {
                    Err(DbError::MissingLog)
                }
            }
            Err(e) => Err(DbError::from(DbSqlError::from(e))),
        }
    }

    async fn get_logs<'a>(
        &'a self,
        block_number: Option<u64>,
        block_offset: Option<u64>,
    ) -> Result<Vec<SerializableLog>> {
        let (min_block_number, max_block_number) =
            block_range_to_i64(block_number, block_offset).map_err(DbError::from)?;

        let query = Log::find()
            .find_also_related(LogStatus)
            .filter(log::Column::BlockNumber.gte(min_block_number))
            .apply_if(max_block_number, |q, v| q.filter(log::Column::BlockNumber.lt(v)))
            .order_by_asc(log::Column::BlockNumber)
            .order_by_asc(log::Column::TxIndex)
            .order_by_asc(log::Column::LogIndex);

        match query.all(self.conn(TargetDb::Logs)).await {
            Ok(logs) => logs
                .into_iter()
                .map(|(log, status)| {
                    if let Some(status) = status {
                        create_log(log, status).map_err(DbError::from)
                    } else {
                        error!(log = ?log, "missing log status for log in db");
                        Err(DbError::MissingLogStatus)
                    }
                })
                .collect::<Result<Vec<_>>>(),
            Err(e) => {
                error!(error = ?e, "failed to get logs from db");
                Err(DbError::from(DbSqlError::from(e)))
            }
        }
    }

    async fn get_logs_count(&self, block_number: Option<u64>, block_offset: Option<u64>) -> Result<u64> {
        let (min_block_number, max_block_number) =
            block_range_to_i64(block_number, block_offset).map_err(DbError::from)?;

        Log::find()
            .select_only()
            .column(log::Column::BlockNumber)
            .column(log::Column::TxIndex)
            .column(log::Column::LogIndex)
            .filter(log::Column::BlockNumber.gte(min_block_number))
            .apply_if(max_block_number, |q, v| q.filter(log::Column::BlockNumber.lt(v)))
            .count(self.conn(TargetDb::Logs))
            .await
            .map_err(|e| DbSqlError::from(e).into())
    }

    async fn get_logs_block_numbers<'a>(
        &'a self,
        block_number: Option<u64>,
        block_offset: Option<u64>,
        processed: Option<bool>,
    ) -> Result<Vec<u64>> {
        let (min_block_number, max_block_number) =
            block_range_to_i64(block_number, block_offset).map_err(DbError::from)?;

        LogStatus::find()
            .select_only()
            .column(log_status::Column::BlockNumber)
            .distinct()
            .filter(log_status::Column::BlockNumber.gte(min_block_number))
            .apply_if(max_block_number, |q, v| q.filter(log_status::Column::BlockNumber.lt(v)))
            .apply_if(processed, |q, v| q.filter(log_status::Column::Processed.eq(v)))
            .order_by_asc(log_status::Column::BlockNumber)
            .into_model::<BlockNumber>()
            .all(self.conn(TargetDb::Logs))
            .await
            .map_err(|e| {
                error!(error = ?e, "failed to get logs block numbers from db");
                DbError::from(DbSqlError::from(e))
            })?
            .into_iter()
            .map(|b| i64_to_u64(b.block_number, "block_number").map_err(DbError::from))
            .collect()
    }

    async fn set_logs_processed(&self, block_number: Option<u64>, block_offset: Option<u64>) -> Result<()> {
        let (min_block_number, max_block_number) =
            block_range_to_i64(block_number, block_offset).map_err(DbError::from)?;
        let now = Utc::now();

        let query = LogStatus::update_many()
            .col_expr(log_status::Column::Processed, Expr::value(Value::Bool(Some(true))))
            .col_expr(
                log_status::Column::ProcessedAt,
                Expr::value(Value::ChronoDateTimeUtc(Some(now))),
            )
            .filter(log_status::Column::BlockNumber.gte(min_block_number))
            .apply_if(max_block_number, |q, v| q.filter(log_status::Column::BlockNumber.lt(v)));

        match query.exec(self.conn(TargetDb::Logs)).await {
            Ok(_) => Ok(()),
            Err(e) => Err(DbError::from(DbSqlError::from(e))),
        }
    }

    async fn set_log_processed<'a>(&'a self, log: SerializableLog) -> Result<()> {
        let now = Utc::now();
        let (block_number, tx_index, log_index) =
            log_position_to_i64(log.block_number, log.tx_index, log.log_index).map_err(DbError::from)?;

        let query = LogStatus::update_many()
            .col_expr(log_status::Column::Processed, Expr::value(Value::Bool(Some(true))))
            .col_expr(
                log_status::Column::ProcessedAt,
                Expr::value(Value::ChronoDateTimeUtc(Some(now))),
            )
            .filter(log_status::Column::BlockNumber.eq(block_number))
            .filter(log_status::Column::TxIndex.eq(tx_index))
            .filter(log_status::Column::LogIndex.eq(log_index));

        match query.exec(self.conn(TargetDb::Logs)).await {
            Ok(_) => Ok(()),
            Err(e) => {
                error!("Failed to update log status in db");
                Err(DbError::from(DbSqlError::from(e)))
            }
        }
    }

    async fn set_log_batch_processed(&self, logs: Vec<SerializableLog>) -> Result<()> {
        let mut seen_positions = HashSet::with_capacity(logs.len());
        let mut positions = Vec::with_capacity(logs.len());

        for log in logs {
            let position = log_position_to_i64(log.block_number, log.tx_index, log.log_index).map_err(DbError::from)?;
            // A repeated position would only grow the condition and force extra statements.
            if seen_positions.insert(position) {
                positions.push(position);
            }
        }

        if positions.is_empty() {
            return Ok(());
        }
        let now = Utc::now();

        self.nest_transaction_in_db(None, TargetDb::Logs)
            .await?
            .perform(|tx| {
                Box::pin(async move {
                    // One statement per chunk instead of one per log. The chunking keeps the
                    // generated condition within the bind-parameter limits of both backends.
                    let chunk_size = import_batch_size(tx.as_ref().get_database_backend(), LOG_POSITION_COLUMNS);

                    for chunk in positions.chunks(chunk_size) {
                        LogStatus::update_many()
                            .col_expr(log_status::Column::Processed, Expr::value(Value::Bool(Some(true))))
                            .col_expr(
                                log_status::Column::ProcessedAt,
                                Expr::value(Value::ChronoDateTimeUtc(Some(now))),
                            )
                            .filter(log_positions_condition(chunk))
                            .exec(tx.as_ref())
                            .await
                            .map_err(DbSqlError::from)
                            .map_err(DbError::from)?;
                    }

                    Ok(())
                })
            })
            .await
    }

    async fn set_logs_unprocessed(&self, block_number: Option<u64>, block_offset: Option<u64>) -> Result<()> {
        let (min_block_number, max_block_number) =
            block_range_to_i64(block_number, block_offset).map_err(DbError::from)?;

        let query = LogStatus::update_many()
            .col_expr(log_status::Column::Processed, Expr::value(Value::Bool(Some(false))))
            .col_expr(
                log_status::Column::ProcessedAt,
                Expr::value(Value::ChronoDateTimeUtc(None)),
            )
            .filter(log_status::Column::BlockNumber.gte(min_block_number))
            .apply_if(max_block_number, |q, v| q.filter(log_status::Column::BlockNumber.lt(v)));

        match query.exec(self.conn(TargetDb::Logs)).await {
            Ok(_) => Ok(()),
            Err(e) => Err(DbError::from(DbSqlError::from(e))),
        }
    }

    async fn get_last_checksummed_log(&self) -> Result<Option<SerializableLog>> {
        let query = LogStatus::find()
            .filter(log_status::Column::Checksum.is_not_null())
            .order_by_desc(log_status::Column::BlockNumber)
            .order_by_desc(log_status::Column::TxIndex)
            .order_by_desc(log_status::Column::LogIndex)
            .find_also_related(Log);

        match query.one(self.conn(TargetDb::Logs)).await {
            Ok(Some((status, Some(log)))) => {
                if let Ok(slog) = create_log(log, status) {
                    Ok(Some(slog))
                } else {
                    Ok(None)
                }
            }
            Ok(_) => Ok(None),
            Err(e) => Err(DbError::from(DbSqlError::from(e))),
        }
    }

    async fn update_logs_checksums(&self) -> Result<Hash> {
        self.nest_transaction_in_db(None, TargetDb::Logs)
            .await?
            .perform(|tx| {
                Box::pin(async move {
                    let mut last_checksum = LogStatus::find()
                        .filter(log_status::Column::Checksum.is_not_null())
                        .order_by_desc(log_status::Column::BlockNumber)
                        .order_by_desc(log_status::Column::TxIndex)
                        .order_by_desc(log_status::Column::LogIndex)
                        .one(tx.as_ref())
                        .await
                        .map_err(|e| DbError::from(DbSqlError::from(e)))?
                        .and_then(|m| m.checksum)
                        .and_then(|c| Hash::try_from(c.as_slice()).ok())
                        .unwrap_or_default();

                    let query = LogStatus::find()
                        .filter(log_status::Column::Checksum.is_null())
                        .order_by_asc(log_status::Column::BlockNumber)
                        .order_by_asc(log_status::Column::TxIndex)
                        .order_by_asc(log_status::Column::LogIndex)
                        .find_also_related(Log);

                    match query.all(tx.as_ref()).await {
                        Ok(entries) => {
                            let mut entries = entries.into_iter();
                            while let Some((status, Some(log_entry))) = entries.next() {
                                let slog = create_log(log_entry.clone(), status.clone())?;
                                // we compute the hash of a single log as a combination of the block
                                // hash, TX hash, and the log index
                                let log_hash = Hash::create(&[
                                    log_entry.block_hash.as_slice(),
                                    log_entry.transaction_hash.as_slice(),
                                    &i64_to_u64(log_entry.log_index, "log_index")?.to_be_bytes(),
                                ]);

                                let next_checksum = Hash::create(&[last_checksum.as_ref(), log_hash.as_ref()]);

                                let mut updated_status = status.into_active_model();
                                updated_status.checksum = Set(Some(next_checksum.as_ref().to_vec()));

                                match updated_status.update(tx.as_ref()).await {
                                    Ok(_) => {
                                        last_checksum = next_checksum;
                                        trace!(log = %slog, checksum = %next_checksum, "Generated log checksum");
                                    }
                                    Err(error) => {
                                        error!(%error, "Failed to update log status checksum in db");
                                        break;
                                    }
                                }
                            }
                            Ok(last_checksum)
                        }
                        Err(e) => Err(DbError::from(DbSqlError::from(e))),
                    }
                })
            })
            .await
    }

    async fn ensure_logs_origin(&self, contract_address_topics: Vec<(Address, Hash)>) -> Result<()> {
        if contract_address_topics.is_empty() {
            return Err(DbError::LogicalError(
                "contract address topics must not be empty".into(),
            ));
        }

        self.nest_transaction_in_db(None, TargetDb::Logs)
            .await?
            .perform(|tx| {
                Box::pin(async move {
                    // keep selected columns to a minimum to reduce copy overhead in db
                    let log_count = Log::find()
                        .select_only()
                        .column(log::Column::BlockNumber)
                        .column(log::Column::TxIndex)
                        .column(log::Column::LogIndex)
                        .count(tx.as_ref())
                        .await
                        .map_err(|e| DbError::from(DbSqlError::from(e)))?;
                    let log_topic_count = LogTopicInfo::find()
                        .count(tx.as_ref())
                        .await
                        .map_err(|e| DbError::from(DbSqlError::from(e)))?;

                    if log_count == 0 && log_topic_count == 0 {
                        // Prime the DB with the values
                        LogTopicInfo::insert_many(contract_address_topics.into_iter().map(|(addr, topic)| {
                            log_topic_info::ActiveModel {
                                address: Set(addr.as_ref().to_vec()),
                                topic: Set(topic.as_ref().to_vec()),
                                ..Default::default()
                            }
                        }))
                        .exec_without_returning(tx.as_ref())
                        .await
                        .map_err(|e| DbError::from(DbSqlError::from(e)))?;
                    } else {
                        // Check that all contract addresses and topics are in the DB
                        for (addr, topic) in contract_address_topics {
                            let log_topic_count = LogTopicInfo::find()
                                .filter(log_topic_info::Column::Address.eq(addr.as_ref().to_vec()))
                                .filter(log_topic_info::Column::Topic.eq(topic.as_ref().to_vec()))
                                .count(tx.as_ref())
                                .await
                                .map_err(|e| DbError::from(DbSqlError::from(e)))?;
                            if log_topic_count == 0 {
                                tracing::error!(
                                    %addr,
                                    %topic,
                                    "Missing address/topic info in log topic info table"
                                );
                                return Err(DbError::InconsistentLogs);
                            }
                            if log_topic_count != 1 {
                                tracing::error!(
                                    %addr,
                                    %topic,
                                    "More than one address/topic info in log topic info table"
                                );
                                return Err(DbError::InconsistentLogs);
                            }
                        }
                    }
                    Ok(())
                })
            })
            .await
    }
}

fn create_log(raw_log: log::Model, status: log_status::Model) -> crate::errors::Result<SerializableLog> {
    let log = SerializableLog::try_from(raw_log).map_err(DbSqlError::from)?;

    let checksum = if let Some(c) = status.checksum {
        let h: std::result::Result<[u8; 32], _> = c.try_into();

        if let Ok(hash) = h {
            Some(Hash::from(hash).to_hex())
        } else {
            return Err(DbSqlError::from(DbEntityError::Conversion(
                "Invalid log checksum".into(),
            )));
        }
    } else {
        None
    };

    let log = if let Some(raw_ts) = status.processed_at {
        let ts = DateTime::<Utc>::from_naive_utc_and_offset(raw_ts, Utc);
        SerializableLog {
            processed: Some(status.processed),
            processed_at: Some(ts),
            checksum,
            ..log
        }
    } else {
        SerializableLog {
            processed: Some(status.processed),
            processed_at: None,
            checksum,
            ..log
        }
    };

    Ok(log)
}

#[cfg(test)]
mod tests {
    use hopr_types::crypto::prelude::Hash;

    use super::*;

    fn test_log(block_number: u64, tx_index: u64, log_index: u64) -> SerializableLog {
        SerializableLog {
            address: Address::new(b"my address 123456789"),
            topics: [Hash::create(&[b"my topic"]).into()].into(),
            data: vec![block_number as u8, tx_index as u8, log_index as u8],
            block_hash: Hash::create(&[b"my block hash"]).into(),
            tx_hash: Hash::create(&[b"my tx hash"]).into(),
            block_number,
            tx_index,
            log_index,
            removed: false,
            ..Default::default()
        }
    }

    #[tokio::test]
    async fn test_store_single_log() {
        let db = BlokliDb::new_in_memory().await.unwrap();

        let log = SerializableLog {
            address: Address::new(b"my address 123456789"),
            topics: [Hash::create(&[b"my topic"]).into()].into(),
            data: [1, 2, 3, 4].into(),
            tx_index: 1u64,
            block_number: 1u64,
            block_hash: Hash::create(&[b"my block hash"]).into(),
            tx_hash: Hash::create(&[b"my tx hash"]).into(),
            log_index: 1u64,
            removed: false,
            processed: Some(false),
            ..Default::default()
        };

        db.store_log(log.clone()).await.unwrap();

        let logs = db.get_logs(None, None).await.unwrap();

        assert_eq!(logs.len(), 1);
        assert_eq!(logs[0], log);
    }

    #[tokio::test]
    async fn test_store_multiple_logs() {
        let db = BlokliDb::new_in_memory().await.unwrap();

        let log_1 = SerializableLog {
            address: Address::new(b"my address 123456789"),
            topics: [Hash::create(&[b"my topic"]).into()].into(),
            data: [1, 2, 3, 4].into(),
            tx_index: 1u64,
            block_number: 1u64,
            block_hash: Hash::create(&[b"my block hash"]).into(),
            tx_hash: Hash::create(&[b"my tx hash"]).into(),
            log_index: 1u64,
            removed: false,
            processed: Some(false),
            ..Default::default()
        };

        let log_2 = SerializableLog {
            address: Address::new(b"my address 223456789"),
            topics: [Hash::create(&[b"my topic 2"]).into()].into(),
            data: [1, 2, 3, 4, 5].into(),
            tx_index: 2u64,
            block_number: 2u64,
            block_hash: Hash::create(&[b"my block hash 2"]).into(),
            tx_hash: Hash::create(&[b"my tx hash 2"]).into(),
            log_index: 2u64,
            removed: false,
            processed: Some(true),
            ..Default::default()
        };

        db.store_log(log_1.clone()).await.unwrap();
        db.store_log(log_2.clone()).await.unwrap();

        let logs = db.get_logs(None, None).await.unwrap();

        assert_eq!(logs.len(), 2);
        assert_eq!(logs[0], log_1);
        assert_eq!(logs[1], log_2);

        let log_2_retrieved = db
            .get_log(log_2.block_number, log_2.tx_index, log_2.log_index)
            .await
            .unwrap();

        assert_eq!(log_2, log_2_retrieved);
    }

    #[tokio::test]
    async fn test_store_duplicate_log() {
        let db = BlokliDb::new_in_memory().await.unwrap();

        let log = SerializableLog {
            address: Address::new(b"my address 123456789"),
            topics: [Hash::create(&[b"my topic"]).into()].into(),
            data: [1, 2, 3, 4].into(),
            tx_index: 1u64,
            block_number: 1u64,
            block_hash: Hash::create(&[b"my block hash"]).into(),
            tx_hash: Hash::create(&[b"my tx hash"]).into(),
            log_index: 1u64,
            removed: false,
            ..Default::default()
        };

        db.store_log(log.clone()).await.unwrap();

        // Idempotent: storing the same log again should succeed
        db.store_log(log.clone()).await.unwrap();

        let logs = db.get_logs(None, None).await.unwrap();

        assert_eq!(logs.len(), 1);
    }

    #[tokio::test]
    async fn test_store_logs_batch_mixes_new_and_existing_without_resetting_status() {
        let db = BlokliDb::new_in_memory().await.unwrap();

        let existing = test_log(1, 1, 1);
        let fresh = test_log(1, 1, 2);

        db.store_log(existing.clone()).await.unwrap();
        db.set_log_processed(existing.clone()).await.unwrap();

        // The batch mixes an already stored (and processed) log with a new one: the new log must
        // be inserted with its own status, and the existing status must survive untouched.
        let results = db.store_logs(vec![existing.clone(), fresh.clone()]).await.unwrap();
        assert_eq!(results.len(), 2);
        assert!(results.into_iter().all(|result| result.is_ok()));

        let logs = db.get_logs(None, None).await.unwrap();
        assert_eq!(logs.len(), 2);

        let existing_db = db
            .get_log(existing.block_number, existing.tx_index, existing.log_index)
            .await
            .unwrap();
        assert_eq!(existing_db.processed, Some(true));
        assert!(existing_db.processed_at.is_some());
        assert_eq!(existing_db.data, existing.data);

        let fresh_db = db
            .get_log(fresh.block_number, fresh.tx_index, fresh.log_index)
            .await
            .unwrap();
        assert_eq!(fresh_db.processed, Some(false));
        assert_eq!(fresh_db.processed_at, None);
    }

    #[tokio::test]
    async fn test_store_logs_collapses_repeated_positions() {
        let db = BlokliDb::new_in_memory().await.unwrap();

        let log = test_log(1, 1, 1);

        let results = db.store_logs(vec![log.clone(), log.clone()]).await.unwrap();
        assert_eq!(results.len(), 2, "one result is reported per input log");
        assert!(results.into_iter().all(|result| result.is_ok()));

        let logs = db.get_logs(None, None).await.unwrap();
        assert_eq!(logs.len(), 1);
    }

    #[tokio::test]
    async fn test_set_log_batch_processed_marks_only_the_given_logs() {
        let db = BlokliDb::new_in_memory().await.unwrap();

        let all_logs = vec![
            test_log(1, 0, 0),
            test_log(1, 0, 1),
            test_log(2, 0, 0),
            test_log(2, 1, 0),
        ];
        db.store_logs(all_logs.clone()).await.unwrap();

        // Two logs, from two different blocks, must be marked without touching their neighbours.
        let marked = vec![all_logs[0].clone(), all_logs[3].clone()];
        db.set_log_batch_processed(marked).await.unwrap();

        let expected = [true, false, false, true];
        for (log, expected_processed) in all_logs.iter().zip(expected) {
            let stored = db.get_log(log.block_number, log.tx_index, log.log_index).await.unwrap();
            assert_eq!(
                stored.processed,
                Some(expected_processed),
                "unexpected processed flag for block {} tx {} log {}",
                log.block_number,
                log.tx_index,
                log.log_index
            );
            assert_eq!(stored.processed_at.is_some(), expected_processed);
        }
    }

    #[tokio::test]
    async fn test_set_log_processed() {
        let db = BlokliDb::new_in_memory().await.unwrap();

        let log = SerializableLog {
            address: Address::new(b"my address 123456789"),
            topics: [Hash::create(&[b"my topic"]).into()].into(),
            data: [1, 2, 3, 4].into(),
            tx_index: 1u64,
            block_number: 1u64,
            block_hash: Hash::create(&[b"my block hash"]).into(),
            tx_hash: Hash::create(&[b"my tx hash"]).into(),
            log_index: 1u64,
            removed: false,
            ..Default::default()
        };

        db.store_log(log.clone()).await.unwrap();

        let log_db = db.get_log(log.block_number, log.tx_index, log.log_index).await.unwrap();

        assert_eq!(log_db.processed, Some(false));
        assert_eq!(log_db.processed_at, None);

        db.set_log_processed(log.clone()).await.unwrap();

        let log_db_updated = db.get_log(log.block_number, log.tx_index, log.log_index).await.unwrap();

        assert_eq!(log_db_updated.processed, Some(true));
        assert!(log_db_updated.processed_at.is_some());
    }

    #[tokio::test]
    async fn test_list_logs_ordered() {
        let db = BlokliDb::new_in_memory().await.unwrap();

        let logs_per_tx = 3;
        let tx_per_block = 3;
        let blocks = 10;
        let start_block = 32183412;
        let base_log = SerializableLog {
            address: Address::new(b"my address 123456789"),
            topics: [Hash::create(&[b"my topic"]).into()].into(),
            data: [1, 2, 3, 4].into(),
            tx_index: 0,
            block_number: 0,
            block_hash: Hash::create(&[b"my block hash"]).into(),
            tx_hash: Hash::create(&[b"my tx hash"]).into(),
            log_index: 0,
            removed: false,
            ..Default::default()
        };

        for block_offset in 0..blocks {
            for tx_index in 0..tx_per_block {
                for log_index in 0..logs_per_tx {
                    let log = SerializableLog {
                        tx_index,
                        block_number: start_block + block_offset,
                        log_index,
                        ..base_log.clone()
                    };
                    db.store_log(log).await.unwrap()
                }
            }
        }

        let block_fetch_interval = 3;
        let mut next_block = start_block;

        while next_block <= start_block + blocks {
            let ordered_logs = db.get_logs(Some(next_block), Some(block_fetch_interval)).await.unwrap();

            assert!(!ordered_logs.is_empty());

            ordered_logs.iter().reduce(|prev_log, curr_log| {
                assert!(prev_log.block_number >= next_block);
                assert!(prev_log.block_number <= (next_block + block_fetch_interval));
                assert!(curr_log.block_number >= next_block);
                assert!(curr_log.block_number <= (next_block + block_fetch_interval));
                if prev_log.block_number == curr_log.block_number {
                    if prev_log.tx_index == curr_log.tx_index {
                        assert!(prev_log.log_index < curr_log.log_index);
                    } else {
                        assert!(prev_log.tx_index < curr_log.tx_index);
                    }
                } else {
                    assert!(prev_log.block_number < curr_log.block_number);
                }
                curr_log
            });
            next_block += block_fetch_interval;
        }
    }

    #[tokio::test]
    async fn test_get_nonexistent_log() {
        let db = BlokliDb::new_in_memory().await.unwrap();

        let result = db.get_log(999, 999, 999).await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_get_logs_with_block_offset() {
        let db = BlokliDb::new_in_memory().await.unwrap();

        let log_1 = SerializableLog {
            address: Address::new(b"my address 123456789"),
            topics: [Hash::create(&[b"topic1"]).into()].into(),
            data: [1, 2, 3, 4].into(),
            tx_index: 1,
            block_number: 1,
            block_hash: Hash::create(&[b"block_hash1"]).into(),
            tx_hash: Hash::create(&[b"tx_hash1"]).into(),
            log_index: 1,
            removed: false,
            processed: Some(false),
            ..Default::default()
        };

        let log_2 = SerializableLog {
            address: Address::new(b"my address 223456789"),
            topics: [Hash::create(&[b"topic2"]).into()].into(),
            data: [1, 2, 3, 4].into(),
            tx_index: 2,
            block_number: 2,
            block_hash: Hash::create(&[b"block_hash2"]).into(),
            tx_hash: Hash::create(&[b"tx_hash2"]).into(),
            log_index: 2,
            removed: false,
            processed: Some(false),
            ..Default::default()
        };

        db.store_logs(vec![log_1.clone(), log_2.clone()])
            .await
            .unwrap()
            .into_iter()
            .for_each(|r| assert!(r.is_ok()));

        let logs = db.get_logs(Some(1), Some(0)).await.unwrap();

        assert_eq!(logs.len(), 1);
        assert_eq!(logs[0], log_1);
    }

    #[tokio::test]
    async fn test_set_logs_unprocessed() {
        let db = BlokliDb::new_in_memory().await.unwrap();

        let log = SerializableLog {
            address: Address::new(b"my address 123456789"),
            topics: [Hash::create(&[b"topic"]).into()].into(),
            data: [1, 2, 3, 4].into(),
            tx_index: 1,
            block_number: 1,
            block_hash: Hash::create(&[b"block_hash"]).into(),
            tx_hash: Hash::create(&[b"tx_hash"]).into(),
            log_index: 1,
            removed: false,
            processed: Some(true),
            processed_at: Some(Utc::now()),
            ..Default::default()
        };

        db.store_log(log.clone()).await.unwrap();

        db.set_logs_unprocessed(Some(1), Some(0)).await.unwrap();

        let log_db = db.get_log(log.block_number, log.tx_index, log.log_index).await.unwrap();

        assert_eq!(log_db.processed, Some(false));
        assert!(log_db.processed_at.is_none());
    }

    #[tokio::test]
    async fn test_get_logs_block_numbers() {
        let db = BlokliDb::new_in_memory().await.unwrap();

        let log_1 = SerializableLog {
            address: Address::new(b"my address 123456789"),
            topics: [Hash::create(&[b"topic1"]).into()].into(),
            data: [1, 2, 3, 4].into(),
            tx_index: 1,
            block_number: 1,
            block_hash: Hash::create(&[b"block_hash1"]).into(),
            tx_hash: Hash::create(&[b"tx_hash1"]).into(),
            log_index: 1,
            removed: false,
            processed: Some(true),
            ..Default::default()
        };

        let log_2 = SerializableLog {
            address: Address::new(b"my address 223456789"),
            topics: [Hash::create(&[b"topic2"]).into()].into(),
            data: [1, 2, 3, 4].into(),
            tx_index: 2,
            block_number: 2,
            block_hash: Hash::create(&[b"block_hash2"]).into(),
            tx_hash: Hash::create(&[b"tx_hash2"]).into(),
            log_index: 2,
            removed: false,
            processed: Some(false),
            ..Default::default()
        };

        let log_3 = SerializableLog {
            address: Address::new(b"my address 323456789"),
            topics: [Hash::create(&[b"topic3"]).into()].into(),
            data: [1, 2, 3, 4].into(),
            tx_index: 3,
            block_number: 3,
            block_hash: Hash::create(&[b"block_hash3"]).into(),
            tx_hash: Hash::create(&[b"tx_hash3"]).into(),
            log_index: 3,
            removed: false,
            processed: Some(false),
            ..Default::default()
        };

        db.store_logs(vec![log_1.clone(), log_2.clone(), log_3.clone()])
            .await
            .unwrap()
            .into_iter()
            .for_each(|r| assert!(r.is_ok()));

        let block_numbers_all = db.get_logs_block_numbers(None, None, None).await.unwrap();
        assert_eq!(block_numbers_all.len(), 3);
        assert_eq!(block_numbers_all, [1, 2, 3]);

        let block_numbers_first_only = db.get_logs_block_numbers(Some(1), Some(0), None).await.unwrap();
        assert_eq!(block_numbers_first_only.len(), 1);
        assert_eq!(block_numbers_first_only[0], 1);

        let block_numbers_last_only = db.get_logs_block_numbers(Some(3), Some(0), None).await.unwrap();
        assert_eq!(block_numbers_last_only.len(), 1);
        assert_eq!(block_numbers_last_only[0], 3);

        let block_numbers_processed = db.get_logs_block_numbers(None, None, Some(true)).await.unwrap();
        assert_eq!(block_numbers_processed.len(), 1);
        assert_eq!(block_numbers_processed[0], 1);

        let block_numbers_unprocessed_second = db.get_logs_block_numbers(Some(2), Some(0), Some(false)).await.unwrap();
        assert_eq!(block_numbers_unprocessed_second.len(), 1);
        assert_eq!(block_numbers_unprocessed_second[0], 2);
    }

    #[tokio::test]
    async fn test_update_logs_checksums() {
        let db = BlokliDb::new_in_memory().await.unwrap();

        // insert first log and update checksum
        let log_1 = SerializableLog {
            address: Address::new(b"my address 123456789"),
            topics: [Hash::create(&[b"topic"]).into()].into(),
            data: [1, 2, 3, 4].into(),
            tx_index: 1,
            block_number: 1,
            block_hash: Hash::create(&[b"block_hash"]).into(),
            tx_hash: Hash::create(&[b"tx_hash"]).into(),
            log_index: 1,
            removed: false,
            ..Default::default()
        };

        db.store_log(log_1.clone()).await.unwrap();

        assert!(db.get_last_checksummed_log().await.unwrap().is_none());

        db.update_logs_checksums().await.unwrap();

        let updated_log_1 = db.get_last_checksummed_log().await.unwrap().unwrap();
        assert!(updated_log_1.checksum.is_some());

        // insert two more logs and update checksums
        let log_2 = SerializableLog {
            block_number: 2,
            ..log_1.clone()
        };
        let log_3 = SerializableLog {
            block_number: 3,
            ..log_1.clone()
        };

        db.store_logs(vec![log_2.clone(), log_3.clone()])
            .await
            .unwrap()
            .into_iter()
            .for_each(|r| assert!(r.is_ok()));

        // ensure the first log is still the last updated
        assert_eq!(
            updated_log_1.clone().checksum.unwrap(),
            db.get_last_checksummed_log().await.unwrap().unwrap().checksum.unwrap()
        );

        db.update_logs_checksums().await.unwrap();

        let updated_log_3 = db.get_last_checksummed_log().await.unwrap().unwrap();

        db.get_logs(None, None).await.unwrap().into_iter().for_each(|log| {
            assert!(log.checksum.is_some());
        });

        // ensure the first log is not the last updated anymore
        assert_ne!(
            updated_log_1.clone().checksum.unwrap(),
            updated_log_3.clone().checksum.unwrap(),
        );
        assert_ne!(updated_log_1, updated_log_3);
    }

    /// After the `1.3.0` -> `1.4.0` schema bump clears the logs database, `ensure_logs_origin`
    /// must accept the widened contract set and prime the `log_topic_info` table with it, rather
    /// than reporting the service registry topics as inconsistent.
    ///
    /// The topic list is built explicitly here: the authoritative one lives in
    /// `chain/indexer/src/constants.rs::topics::service_registry()` (section 1.2), and `blokli-db`
    /// cannot depend on the indexer crate.
    #[tokio::test]
    async fn test_ensure_logs_origin_primes_service_registry_topics_on_cleared_db() -> anyhow::Result<()> {
        let db = BlokliDb::new_in_memory().await?;

        let channels = Address::new(b"channels contract 12");
        let registry = Address::new(b"service registry 123");

        let pre_upgrade_origin = vec![(channels, Hash::create(&[b"ChannelOpened"]))];

        // The ten service registry events the indexer subscribes to.
        let registry_topics: Vec<Hash> = [
            b"Registered".as_slice(),
            b"Updated".as_slice(),
            b"Deregistered".as_slice(),
            b"ServiceTypeRegistered".as_slice(),
            b"TypeOwnershipTransferred".as_slice(),
            b"RequirementUpdated".as_slice(),
            b"SelfRegistrationBurnUpdated".as_slice(),
            b"SelfUpdateBurnUpdated".as_slice(),
            b"TypeRegistrationFeeUpdated".as_slice(),
            b"NodeSafeRegistryUpdated".as_slice(),
        ]
        .into_iter()
        .map(|name| Hash::create(&[name]))
        .collect();

        let post_upgrade_origin: Vec<(Address, Hash)> = pre_upgrade_origin
            .iter()
            .copied()
            .chain(registry_topics.iter().map(|topic| (registry, *topic)))
            .collect();

        // A database primed before the upgrade rejects the widened set.
        db.ensure_logs_origin(pre_upgrade_origin.clone()).await?;
        assert!(matches!(
            db.ensure_logs_origin(post_upgrade_origin.clone()).await,
            Err(DbError::InconsistentLogs)
        ));

        // The minor schema bump clears the logs tables, which is what unblocks the new set.
        LogTopicInfo::delete_many().exec(db.conn(TargetDb::Logs)).await?;
        LogStatus::delete_many().exec(db.conn(TargetDb::Logs)).await?;
        Log::delete_many().exec(db.conn(TargetDb::Logs)).await?;

        db.ensure_logs_origin(post_upgrade_origin.clone()).await?;

        let primed = LogTopicInfo::find().count(db.conn(TargetDb::Logs)).await?;
        assert_eq!(primed, post_upgrade_origin.len() as u64);

        // A second call over the same set is a no-op rather than an error.
        db.ensure_logs_origin(post_upgrade_origin).await?;

        Ok(())
    }

    #[tokio::test]
    async fn test_should_not_allow_inconsistent_logs_in_the_db() -> anyhow::Result<()> {
        let db = BlokliDb::new_in_memory().await?;
        let addr_1 = Address::new(b"my address 123456789");
        let addr_2 = Address::new(b"my 2nd address 12345");
        let topic_1 = Hash::create(&[b"my topic 1"]);
        let topic_2 = Hash::create(&[b"my topic 2"]);

        db.ensure_logs_origin(vec![(addr_1, topic_1)]).await?;

        db.ensure_logs_origin(vec![(addr_1, topic_2)])
            .await
            .expect_err("expected error due to inconsistent logs in the db");

        db.ensure_logs_origin(vec![(addr_2, topic_1)])
            .await
            .expect_err("expected error due to inconsistent logs in the db");

        db.ensure_logs_origin(vec![(addr_1, topic_1)]).await?;

        Ok(())
    }
}
