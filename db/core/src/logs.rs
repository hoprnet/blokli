use std::collections::HashSet;

use async_trait::async_trait;
use blokli_db_entity::{
    errors::DbEntityError,
    log, log_status, log_topic_info,
    prelude::{Log, LogStatus, LogTopicInfo},
};
use futures::{StreamExt, stream};
use hopr_types::{
    crypto::prelude::Hash,
    primitive::prelude::{Address, DateTime, SerializableLog, ToHex, Utc},
};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, DatabaseBackend, DbErr, EntityTrait, FromQueryResult,
    PaginatorTrait, QueryFilter, QueryOrder, QuerySelect,
    entity::{Set, Unchanged},
    query::QueryTrait,
    sea_query::{CaseStatement, Expr, ExprTrait, OnConflict, Value},
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
    numeric::{block_range_to_i64, i64_to_u64, log_position_to_i64, u64_to_i64},
};

#[derive(FromQueryResult)]
struct BlockNumber {
    block_number: i64,
}

#[derive(FromQueryResult)]
struct LogIdentity {
    tx_index: i64,
    log_index: i64,
    block_hash: Vec<u8>,
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
        self.nest_transaction_in_db(None, TargetDb::Logs)
            .await?
            .perform(|tx| {
                Box::pin(async move {
                    let results = stream::iter(logs).then(|log| async {
                        let log_id = log.to_string();
                        let (block_number, tx_index, log_index) =
                            log_position_to_i64(log.block_number, log.tx_index, log.log_index)
                                .map_err(DbError::from)?;

                        let block_hash = log.block_hash.to_vec();

                        // Build both ActiveModels up front so a conversion failure
                        // cannot leave an orphaned log row without a matching log_status.
                        let log_model = log::ActiveModel::try_from(log.clone())
                            .map_err(DbSqlError::from)
                            .map_err(DbError::from)?;
                        let replacement_model = log_model.clone();
                        let mut status_model = log_status::ActiveModel::try_from(log)
                            .map_err(DbSqlError::from)
                            .map_err(DbError::from)?;
                        let log_result = Log::insert(log_model)
                            .on_conflict(
                                OnConflict::columns([
                                    log::Column::LogIndex,
                                    log::Column::TxIndex,
                                    log::Column::BlockNumber,
                                ])
                                .do_nothing()
                                .to_owned(),
                            )
                            .exec(tx.as_ref())
                            .await;

                        match log_result {
                            Ok(insert_result) => {
                                // Log was inserted successfully, now insert LogStatus with the log_id
                                status_model.log_id = Set(insert_result.last_insert_id);

                                match LogStatus::insert(status_model)
                                    .on_conflict(
                                        OnConflict::columns([
                                            log_status::Column::LogIndex,
                                            log_status::Column::TxIndex,
                                            log_status::Column::BlockNumber,
                                        ])
                                        .do_nothing()
                                        .to_owned(),
                                    )
                                    .exec(tx.as_ref())
                                    .await
                                {
                                    Ok(_) => Ok(()),
                                    Err(DbErr::RecordNotInserted) => {
                                        // LogStatus already exists - idempotent success
                                        trace!(log_id, "log status already in the DB");
                                        Ok(())
                                    }
                                    Err(e) => {
                                        error!(%log_id, error = ?e, "failed to insert log status into db");
                                        Err(DbError::General(e.to_string()))
                                    }
                                }
                            }
                            Err(DbErr::RecordNotInserted) => {
                                // Log already exists, need to find it to get its ID for LogStatus
                                match Log::find()
                                    .filter(log::Column::BlockNumber.eq(block_number))
                                    .filter(log::Column::TxIndex.eq(tx_index))
                                    .filter(log::Column::LogIndex.eq(log_index))
                                    .one(tx.as_ref())
                                    .await
                                {
                                    Ok(Some(existing_log)) => {
                                        if existing_log.block_hash != block_hash {
                                            let mut replacement = replacement_model;
                                            replacement.id = Unchanged(existing_log.id);

                                            if let Err(e) = replacement.update(tx.as_ref()).await {
                                                error!(%log_id, error = ?e, "failed to replace reorganised log in db");
                                                return Err(DbError::General(e.to_string()));
                                            }

                                            if let Err(e) = LogStatus::update_many()
                                                .col_expr(
                                                    log_status::Column::Processed,
                                                    Expr::value(Value::Bool(Some(false))),
                                                )
                                                .col_expr(
                                                    log_status::Column::ProcessedAt,
                                                    Expr::value(Value::ChronoDateTimeUtc(None)),
                                                )
                                                .col_expr(
                                                    log_status::Column::Checksum,
                                                    Expr::value(Value::Bytes(None)),
                                                )
                                                .filter(log_status::Column::BlockNumber.eq(block_number))
                                                .filter(log_status::Column::TxIndex.eq(tx_index))
                                                .filter(log_status::Column::LogIndex.eq(log_index))
                                                .exec(tx.as_ref())
                                                .await
                                            {
                                                error!(%log_id, error = ?e, "failed to reset status of reorganised log");
                                                return Err(DbError::General(e.to_string()));
                                            }

                                            if let Err(e) = LogStatus::update_many()
                                                .col_expr(
                                                    log_status::Column::Checksum,
                                                    Expr::value(Value::Bytes(None)),
                                                )
                                                .filter(log_status::Column::BlockNumber.gte(block_number))
                                                .exec(tx.as_ref())
                                                .await
                                            {
                                                error!(%log_id, error = ?e, "failed to invalidate checksums after a reorganised log");
                                                return Err(DbError::General(e.to_string()));
                                            }

                                            trace!(log_id, "replaced reorganised log in the DB");
                                        }

                                        // Found existing log, reuse the pre-built status model with its ID
                                        status_model.log_id = Set(existing_log.id);

                                        match LogStatus::insert(status_model)
                                            .on_conflict(
                                                OnConflict::columns([
                                                    log_status::Column::LogIndex,
                                                    log_status::Column::TxIndex,
                                                    log_status::Column::BlockNumber,
                                                ])
                                                .do_nothing()
                                                .to_owned(),
                                            )
                                            .exec(tx.as_ref())
                                            .await
                                        {
                                            Ok(_) | Err(DbErr::RecordNotInserted) => {
                                                // Both Log and LogStatus exist - idempotent success
                                                trace!(log_id, "log already in the DB");
                                                Ok(())
                                            }
                                            Err(e) => {
                                                error!(%log_id, error = ?e, "failed to insert log status into db");
                                                Err(DbError::General(e.to_string()))
                                            }
                                        }
                                    }
                                    Ok(None) => {
                                        error!(%log_id, "Log not found after RecordNotInserted error");
                                        Err(DbError::General(format!("Log not found: {log_id}")))
                                    }
                                    Err(e) => {
                                        error!(%log_id, error = ?e, "failed to find existing log");
                                        Err(DbError::General(e.to_string()))
                                    }
                                }
                            }
                            Err(e) => {
                                error!(error = ?e, "failed to insert log into db");
                                Err(DbError::General(e.to_string()))
                            }
                        }
                    });
                    Ok(results.collect::<Vec<_>>().await)
                })
            })
            .await
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

    async fn get_processed_log_identities(&self, block_number: u64) -> Result<HashSet<(u64, u64, [u8; 32])>> {
        let block_number = u64_to_i64(block_number, "block_number").map_err(DbError::from)?;

        let conn = self.conn(TargetDb::Logs);

        // Read the two tables separately, each filtered by block number, and pair them up in
        // memory. Joining them on `log_status.log_id` instead reads correctly but is not indexed -
        // Postgres does not index the referencing side of a foreign key - so the planner drives
        // from `log_status` and walks every processed row in the database to answer a question
        // about one block. That is linear in the total log count, which is invisible on a fresh
        // database and throttles indexing once the table has grown. Both queries here are bounded
        // by the block: `idx_log_composite` covers the first and `idx_unprocessed_log_status` the
        // second. The position triple is `log_status`'s own unique key, so keying on it rather than
        // on `log_id` selects exactly the same rows.
        let processed: HashSet<(i64, i64)> = LogStatus::find()
            .select_only()
            .column(log_status::Column::TxIndex)
            .column(log_status::Column::LogIndex)
            .filter(log_status::Column::BlockNumber.eq(block_number))
            .filter(log_status::Column::Processed.eq(true))
            .into_tuple()
            .all(conn)
            .await
            .map_err(|e| {
                error!(error = ?e, "failed to get processed log positions from db");
                DbError::from(DbSqlError::from(e))
            })?
            .into_iter()
            .collect();

        if processed.is_empty() {
            return Ok(HashSet::new());
        }

        // The block hash comes from the log row rather than its status: a reorganisation can put a
        // different log at the same position, and only the hash tells the two apart.
        Ok(Log::find()
            .select_only()
            .column(log::Column::TxIndex)
            .column(log::Column::LogIndex)
            .column(log::Column::BlockHash)
            .filter(log::Column::BlockNumber.eq(block_number))
            .into_model::<LogIdentity>()
            .all(conn)
            .await
            .map_err(|e| {
                error!(error = ?e, "failed to get processed log identities from db");
                DbError::from(DbSqlError::from(e))
            })?
            .into_iter()
            .filter(|identity| processed.contains(&(identity.tx_index, identity.log_index)))
            .filter_map(|identity| {
                let block_hash: [u8; 32] = identity.block_hash.try_into().ok()?;
                Some((
                    i64_to_u64(identity.tx_index, "tx_index").ok()?,
                    i64_to_u64(identity.log_index, "log_index").ok()?,
                    block_hash,
                ))
            })
            .collect())
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
        let backend = self.conn(TargetDb::Logs).get_database_backend();
        self.update_logs_checksums_in_chunks(
            LOG_CHECKSUM_CHUNK_SIZE,
            rows_within_bind_limit(backend, LOG_CHECKSUM_BINDS_PER_ROW),
        )
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

impl BlokliDb {
    /// Checksums the logs that have none yet, `chunk_size` logs per transaction.
    ///
    /// Chunks are read in log order with a keyset cursor and committed one by one, so the
    /// checksummed logs always form a contiguous prefix of the chain. Each chunk is written with
    /// one `UPDATE` per `rows_per_update` logs. If a chunk fails, it is rolled back, the chunks
    /// before it stay committed and the error is returned; the next call resumes after them.
    async fn update_logs_checksums_in_chunks(&self, chunk_size: u64, rows_per_update: usize) -> Result<Hash> {
        // read inside the first chunk's transaction, so a pass with nothing to do costs one transaction
        let mut last_checksum: Option<Hash> = None;
        let mut cursor: Option<(i64, i64, i64)> = None;

        loop {
            let (chunk_checksum, chunk_cursor, chunk_len) = self
                .nest_transaction_in_db(None, TargetDb::Logs)
                .await?
                .perform(|tx| {
                    Box::pin(async move {
                        let seed = match last_checksum {
                            Some(checksum) => checksum,
                            None => last_log_checksum(tx.as_ref()).await?,
                        };
                        checksum_logs_chunk(tx.as_ref(), seed, cursor, chunk_size, rows_per_update).await
                    })
                })
                .await?;

            match chunk_cursor {
                Some(position) if chunk_len >= chunk_size => {
                    last_checksum = Some(chunk_checksum);
                    cursor = Some(position);
                }
                _ => return Ok(chunk_checksum),
            }
        }
    }
}

/// The checksum of the last checksummed log, or the zero hash if no log has one yet.
async fn last_log_checksum<C: ConnectionTrait>(conn: &C) -> Result<Hash> {
    Ok(LogStatus::find()
        .filter(log_status::Column::Checksum.is_not_null())
        .order_by_desc(log_status::Column::BlockNumber)
        .order_by_desc(log_status::Column::TxIndex)
        .order_by_desc(log_status::Column::LogIndex)
        .one(conn)
        .await
        .map_err(|e| DbError::from(DbSqlError::from(e)))?
        .and_then(|m| m.checksum)
        .and_then(|c| Hash::try_from(c.as_slice()).ok())
        .unwrap_or_default())
}

/// Logs read and checksummed per transaction by [`BlokliDbLogOperations::update_logs_checksums`].
const LOG_CHECKSUM_CHUNK_SIZE: u64 = 10_000;
/// Bind parameters per log in the checksum `UPDATE`: the id and checksum in the `CASE` arm and
/// the id in the `IN` list.
const LOG_CHECKSUM_BINDS_PER_ROW: usize = 3;
/// Default `SQLITE_MAX_VARIABLE_NUMBER` for SQLite builds before 3.32.
const SQLITE_MAX_BIND_PARAMETERS: usize = 999;
/// PostgreSQL wire protocol limit on bind parameters per statement.
const POSTGRES_MAX_BIND_PARAMETERS: usize = 65_535;

/// Number of rows that fit in one statement with `binds_per_row` bind parameters each.
fn rows_within_bind_limit(backend: DatabaseBackend, binds_per_row: usize) -> usize {
    let limit = if backend == DatabaseBackend::Sqlite {
        SQLITE_MAX_BIND_PARAMETERS
    } else {
        POSTGRES_MAX_BIND_PARAMETERS
    };
    (limit / binds_per_row.max(1)).max(1)
}

/// The columns of an unchecksummed log that the checksum and the keyset cursor need.
#[derive(FromQueryResult)]
struct UnchecksummedLog {
    id: i64,
    block_number: i64,
    tx_index: i64,
    log_index: i64,
    block_hash: Vec<u8>,
    transaction_hash: Vec<u8>,
    hashed_log_index: i64,
}

/// Checksums the next `chunk_size` unchecksummed logs after `cursor`, chained onto
/// `last_checksum`.
///
/// Returns the checksum of the last log in the chunk (`last_checksum` if the chunk is empty), the
/// position of that log and the number of logs in the chunk.
async fn checksum_logs_chunk<C: ConnectionTrait>(
    conn: &C,
    mut last_checksum: Hash,
    cursor: Option<(i64, i64, i64)>,
    chunk_size: u64,
    rows_per_update: usize,
) -> Result<(Hash, Option<(i64, i64, i64)>, u64)> {
    let position = || {
        Expr::tuple([
            Expr::col((LogStatus, log_status::Column::BlockNumber)),
            Expr::col((LogStatus, log_status::Column::TxIndex)),
            Expr::col((LogStatus, log_status::Column::LogIndex)),
        ])
    };

    let entries = LogStatus::find()
        .select_only()
        .column(log_status::Column::Id)
        .column(log_status::Column::BlockNumber)
        .column(log_status::Column::TxIndex)
        .column(log_status::Column::LogIndex)
        .column_as(log::Column::BlockHash, "block_hash")
        .column_as(log::Column::TransactionHash, "transaction_hash")
        .column_as(log::Column::LogIndex, "hashed_log_index")
        .inner_join(Log)
        .filter(log_status::Column::Checksum.is_null())
        .apply_if(cursor, |q, (block_number, tx_index, log_index)| {
            q.filter(position().gt(Expr::tuple([
                Expr::value(block_number),
                Expr::value(tx_index),
                Expr::value(log_index),
            ])))
        })
        .order_by_asc(log_status::Column::BlockNumber)
        .order_by_asc(log_status::Column::TxIndex)
        .order_by_asc(log_status::Column::LogIndex)
        .limit(chunk_size)
        .into_model::<UnchecksummedLog>()
        .all(conn)
        .await
        .map_err(|e| DbError::from(DbSqlError::from(e)))?;

    let chunk_len = entries.len() as u64;
    let chunk_cursor = entries.last().map(|e| (e.block_number, e.tx_index, e.log_index));

    let mut checksums = Vec::with_capacity(entries.len());
    for entry in entries {
        if Hash::try_from(entry.block_hash.as_slice()).is_err()
            || Hash::try_from(entry.transaction_hash.as_slice()).is_err()
        {
            return Err(DbSqlError::from(DbEntityError::Conversion(format!(
                "invalid block or transaction hash length in log at {}/{}/{}",
                entry.block_number, entry.tx_index, entry.log_index
            )))
            .into());
        }

        // we compute the hash of a single log as a combination of the block
        // hash, TX hash, and the log index
        let log_hash = Hash::create(&[
            entry.block_hash.as_slice(),
            entry.transaction_hash.as_slice(),
            &i64_to_u64(entry.hashed_log_index, "log_index")?.to_be_bytes(),
        ]);
        last_checksum = Hash::create(&[last_checksum.as_ref(), log_hash.as_ref()]);
        trace!(
            block_number = entry.block_number,
            tx_index = entry.tx_index,
            log_index = entry.log_index,
            checksum = %last_checksum,
            "Generated log checksum"
        );
        checksums.push((entry.id, last_checksum));
    }

    for batch in checksums.chunks(rows_per_update.max(1)) {
        let checksum_by_id = batch.iter().fold(CaseStatement::new(), |case, (id, checksum)| {
            case.case(log_status::Column::Id.eq(*id), Expr::value(checksum.as_ref().to_vec()))
        });

        let result = LogStatus::update_many()
            .col_expr(log_status::Column::Checksum, checksum_by_id.into())
            .filter(log_status::Column::Id.is_in(batch.iter().map(|(id, _)| *id)))
            .exec(conn)
            .await
            .map_err(|error| {
                error!(%error, "Failed to update log status checksums in db");
                DbError::from(DbSqlError::from(error))
            })?;

        if result.rows_affected != batch.len() as u64 {
            error!(
                expected = batch.len(),
                updated = result.rows_affected,
                "Log status checksum update touched an unexpected number of rows"
            );
            return Err(DbError::LogicalError(format!(
                "log status checksum update touched {} rows instead of {}",
                result.rows_affected,
                batch.len()
            )));
        }
    }

    Ok((last_checksum, chunk_cursor, chunk_len))
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
    async fn test_replacing_a_checksummed_log_rebuilds_the_chain_suffix() -> anyhow::Result<()> {
        let db = BlokliDb::new_in_memory().await?;

        let first = SerializableLog {
            address: Address::new(b"my address 123456789"),
            topics: [Hash::create(&[b"my topic"]).into()].into(),
            data: [1, 2, 3, 4].into(),
            tx_index: 0u64,
            block_number: 1u64,
            block_hash: Hash::create(&[b"orphaned block"]).into(),
            tx_hash: Hash::create(&[b"first tx"]).into(),
            log_index: 0u64,
            ..Default::default()
        };
        let second = SerializableLog {
            block_number: 2u64,
            block_hash: Hash::create(&[b"second block"]).into(),
            tx_hash: Hash::create(&[b"second tx"]).into(),
            ..first.clone()
        };

        db.store_logs(vec![first.clone(), second.clone()]).await?;
        db.set_log_processed(first.clone()).await?;
        db.set_log_processed(second.clone()).await?;
        db.update_logs_checksums().await?;

        let second_before = db.get_log(2, 0, 0).await?.checksum;
        assert!(second_before.is_some());

        // A reorganisation replaces the log in block 1. Every checksum from block 1 onward was
        // derived from it, so the whole suffix has to be rebuilt. Clearing only block 1 would make
        // the refill seed from block 2 and hash block 1 on top of it.
        let replacement = SerializableLog {
            block_hash: Hash::create(&[b"canonical block"]).into(),
            data: [5, 6, 7].into(),
            ..first.clone()
        };
        db.store_log(replacement.clone()).await?;
        db.set_log_processed(replacement.clone()).await?;
        db.update_logs_checksums().await?;

        // Recompute the chain by hand and require the stored one to match it exactly.
        let expected_first = Hash::create(&[
            Hash::default().as_ref(),
            Hash::create(&[
                replacement.block_hash.as_ref(),
                replacement.tx_hash.as_ref(),
                &replacement.log_index.to_be_bytes(),
            ])
            .as_ref(),
        ]);
        let expected_second = Hash::create(&[
            expected_first.as_ref(),
            Hash::create(&[
                second.block_hash.as_ref(),
                second.tx_hash.as_ref(),
                &second.log_index.to_be_bytes(),
            ])
            .as_ref(),
        ]);

        assert_eq!(db.get_log(1, 0, 0).await?.checksum, Some(expected_first.to_hex()));
        assert_eq!(db.get_log(2, 0, 0).await?.checksum, Some(expected_second.to_hex()));

        // The later log's checksum must have moved: it depended on the log that was replaced.
        assert_ne!(db.get_log(2, 0, 0).await?.checksum, second_before);

        Ok(())
    }

    #[tokio::test]
    async fn test_store_log_replaces_a_reorganised_position() -> anyhow::Result<()> {
        let db = BlokliDb::new_in_memory().await?;

        let orphaned = SerializableLog {
            address: Address::new(b"my address 123456789"),
            topics: [Hash::create(&[b"my topic"]).into()].into(),
            data: [1, 2, 3, 4].into(),
            tx_index: 1u64,
            block_number: 1u64,
            block_hash: Hash::create(&[b"orphaned block"]).into(),
            tx_hash: Hash::create(&[b"my tx hash"]).into(),
            log_index: 1u64,
            ..Default::default()
        };

        db.store_log(orphaned.clone()).await?;
        db.set_log_processed(orphaned.clone()).await?;

        // A reorganisation puts a different log at the same position. Keeping the orphaned row
        // would leave the processed identity pointing at a hash that no canonical log ever carries,
        // so the canonical log would be dispatched again on every later re-delivery.
        let canonical = SerializableLog {
            block_hash: Hash::create(&[b"canonical block"]).into(),
            data: [5, 6, 7].into(),
            ..orphaned.clone()
        };

        db.store_log(canonical.clone()).await?;

        let stored = db
            .get_log(canonical.block_number, canonical.tx_index, canonical.log_index)
            .await?;
        assert_eq!(stored.block_hash, canonical.block_hash);
        assert_eq!(stored.data, canonical.data);
        assert_eq!(stored.processed, Some(false));

        let identities = db.get_processed_log_identities(canonical.block_number).await?;
        assert!(identities.is_empty());

        db.set_log_processed(canonical.clone()).await?;

        let identities = db.get_processed_log_identities(canonical.block_number).await?;
        assert_eq!(
            identities,
            HashSet::from([(canonical.tx_index, canonical.log_index, canonical.block_hash)])
        );

        Ok(())
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

    /// Checksums of [`checksum_fixture_logs`] in log order, as computed by the single-transaction,
    /// row-by-row checksum pass that preceded the chunked one (captured at commit 8b1b26e).
    const GOLDEN_CHECKSUMS: [&str; 24] = [
        "0xec66b85d55801b050d82d3ad126ffcc61c3bcc75800e4cb86616e403b7d85239",
        "0x59482a5b2c78117ee97c3a2ffa17a94d0023344894b948b1a1f03c356d3bdc6c",
        "0x422e3490a52418aeb14509c6058b43aca90eccec3224623c17f27e3d81e4dda4",
        "0x3069e57371727adebc3515f18bdb2dc4760779b70dc3ef260c256f330efb383c",
        "0x6a1b1488152f2ddcd4eb29eb0e484e07e68fed5289e0e17256d1de253ef489c5",
        "0xb9d752f5e10bc74b0ed19d9ddf1918e04040e647ab1dcf9bace1427f8178d12b",
        "0x3366fc1a6f3d91fe12e982a0b2159ba297649f369b94573f2fdbb05e62fb8b13",
        "0x40ca647e86de3c8aca3fbbc8321e1169e1f63b396b5c5867670bd4dde5a7498d",
        "0xbe4010b1e5f56bceba77294bbf8e55ceda8e4bd1e83e2271c85f3c8ca2033d3f",
        "0x580c928c6396dad5d23a7a845d549ab212ee084b4a26dc0846ddda411ef1c7e6",
        "0xc22b6d967e15077fcaa5db8840ebb811749d3c20d5d5dd14c2348ee199e19367",
        "0x520de1006e05a1973dd28457f118d3d43c6c5b2e8d9044912fbea48c05497765",
        "0x45674dd573988a086ebdd83798fe0848966c75ec271991ecf24860edc85c8daa",
        "0x4416ef3b8bae89546919edde84ca09f63098241cd4483b63f4de2ee451f86ca5",
        "0x073f6e19ea9a14f5e2551ba55b237d9279f6dab7cfe9486393c198ba0279bed5",
        "0xd62bcb53a78a030a9e225ef1c3c5274a348c1d753b6a7d9baf278c66c09d0a53",
        "0xc1c97f3158624baa4a2fe01556976540d40f00d4156aa394f189723d3c202b29",
        "0xcf562dcec0889be58171827d3bab38a3dda40c4c901ab8521e74b50e5b64f7cc",
        "0x17bf2cfa04de716eb20d1061947425514c094f18e1fde438ad4fe0743628221b",
        "0x071952d8a729e5d98a7365ba9aec1b93ba850347e4f5f6fcfa010a4dc1809ed8",
        "0x1dc28a8e993a1efaaa00bc32b5266a7e33d6ad7688117e36f35ff1b16bd061bf",
        "0x97ac6a9eb0f39dd399b07109cc226a2703d805fd1ab626823ab52269baecb3f7",
        "0xe381043fa9c104bfd17d79923621d835ec76566923d16e52808fbee41101208a",
        "0x218b3308e3f574edbcdd38374e1c6ea09da48a222c0172f5b3db92c2ec53de76",
    ];

    /// 24 logs over 6 blocks, with a varying number of transactions per block and two logs per
    /// transaction, in log order.
    fn checksum_fixture_logs() -> Vec<SerializableLog> {
        let mut logs = Vec::new();
        for block in 1u8..=6 {
            for tx in 0..(block % 3 + 1) {
                for log in 0..2u8 {
                    let (block_number, tx_index) = (u64::from(block), u64::from(tx));
                    logs.push(SerializableLog {
                        address: Address::new(b"my address 123456789"),
                        topics: [Hash::create(&[b"topic"]).into()].into(),
                        data: [block, tx, log].into(),
                        tx_index,
                        block_number,
                        block_hash: Hash::create(&[b"block", &block_number.to_be_bytes()]).into(),
                        tx_hash: Hash::create(&[b"tx", &block_number.to_be_bytes(), &tx_index.to_be_bytes()]).into(),
                        log_index: tx_index * 2 + u64::from(log),
                        removed: false,
                        ..Default::default()
                    });
                }
            }
        }
        logs
    }

    async fn reset_logs(db: &BlokliDb) -> anyhow::Result<()> {
        LogStatus::delete_many().exec(db.conn(TargetDb::Logs)).await?;
        Log::delete_many().exec(db.conn(TargetDb::Logs)).await?;
        Ok(())
    }

    async fn store_all(db: &BlokliDb, logs: Vec<SerializableLog>) -> anyhow::Result<()> {
        for result in db.store_logs(logs).await? {
            result?;
        }
        Ok(())
    }

    /// Requires the first `prefix` fixture logs to carry their reference checksum and every later
    /// log to have none.
    async fn assert_golden_prefix(db: &BlokliDb, prefix: usize) -> anyhow::Result<()> {
        let position = |log: &SerializableLog| (log.block_number, log.tx_index, log.log_index);
        let stored: Vec<_> = db
            .get_logs(None, None)
            .await?
            .into_iter()
            .map(|log| (position(&log), log.checksum))
            .collect();
        let expected: Vec<_> = checksum_fixture_logs()
            .iter()
            .zip(GOLDEN_CHECKSUMS)
            .enumerate()
            .map(|(i, (log, checksum))| (position(log), (i < prefix).then(|| checksum.to_string())))
            .collect();
        assert_eq!(stored, expected);
        Ok(())
    }

    fn golden_head() -> anyhow::Result<Hash> {
        Ok(Hash::from_hex(GOLDEN_CHECKSUMS[GOLDEN_CHECKSUMS.len() - 1])?)
    }

    async fn check_chunked_checksums_match_reference(db: &BlokliDb) -> anyhow::Result<()> {
        // (read chunk size, rows per UPDATE): one chunk, chunk boundaries inside and between blocks,
        // several UPDATEs per chunk, and a chunk size that divides the log count exactly.
        for (chunk_size, rows_per_update) in [(10_000, 333), (24, 24), (12, 12), (5, 2), (7, 3), (1, 1)] {
            reset_logs(db).await?;
            let mut logs = checksum_fixture_logs();
            // insert out of log order, so the pass has to sort
            logs.reverse();
            store_all(db, logs).await?;

            let head = db.update_logs_checksums_in_chunks(chunk_size, rows_per_update).await?;
            assert_eq!(
                head,
                golden_head()?,
                "chunk_size {chunk_size}, rows_per_update {rows_per_update}"
            );
            assert_golden_prefix(db, GOLDEN_CHECKSUMS.len()).await?;

            // nothing left to checksum: the head is returned unchanged
            let head = db.update_logs_checksums_in_chunks(chunk_size, rows_per_update).await?;
            assert_eq!(head, golden_head()?);
        }

        // A pass seeded from an earlier one continues the same chain.
        reset_logs(db).await?;
        let mut logs = checksum_fixture_logs();
        let later = logs.split_off(10);
        store_all(db, logs).await?;
        assert_eq!(
            db.update_logs_checksums_in_chunks(4, 3).await?,
            Hash::from_hex(GOLDEN_CHECKSUMS[9])?
        );
        store_all(db, later).await?;
        assert_eq!(db.update_logs_checksums_in_chunks(4, 3).await?, golden_head()?);
        assert_golden_prefix(db, GOLDEN_CHECKSUMS.len()).await?;

        // The trait method gives the same result.
        reset_logs(db).await?;
        store_all(db, checksum_fixture_logs()).await?;
        assert_eq!(db.update_logs_checksums().await?, golden_head()?);
        assert_golden_prefix(db, GOLDEN_CHECKSUMS.len()).await?;

        Ok(())
    }

    /// `fail_from_block_4` must make every checksum update of a log in block 4 or later fail.
    async fn check_failed_chunk_keeps_committed_prefix(
        db: &BlokliDb,
        fail_from_block_4: &str,
        drop_failure: &str,
    ) -> anyhow::Result<()> {
        reset_logs(db).await?;
        store_all(db, checksum_fixture_logs()).await?;
        db.conn(TargetDb::Logs).execute_unprepared(fail_from_block_4).await?;

        // Chunks of 5: logs 0..5 and 5..10 commit. The third chunk holds the two logs of block 3,
        // whose UPDATE succeeds, then the logs of block 4, whose UPDATE fails.
        let result = db.update_logs_checksums_in_chunks(5, 2).await;
        assert!(result.is_err(), "a failed checksum update must be returned as an error");

        // The failed chunk is rolled back as a whole, block 3 included; the earlier chunks stay as
        // a valid prefix of the chain.
        assert_golden_prefix(db, 10).await?;
        let last = db
            .get_last_checksummed_log()
            .await?
            .ok_or_else(|| anyhow::anyhow!("expected a checksummed log"))?;
        assert_eq!((last.block_number, last.tx_index, last.log_index), (2, 2, 5));

        // The next pass resumes from the prefix and ends where an uninterrupted pass ends.
        db.conn(TargetDb::Logs).execute_unprepared(drop_failure).await?;
        assert_eq!(db.update_logs_checksums_in_chunks(5, 2).await?, golden_head()?);
        assert_golden_prefix(db, GOLDEN_CHECKSUMS.len()).await?;

        Ok(())
    }

    const SQLITE_FAIL_FROM_BLOCK_4: &str = "CREATE TRIGGER fail_checksum BEFORE UPDATE OF checksum ON log_status WHEN \
                                            NEW.block_number >= 4 BEGIN SELECT RAISE(ABORT, 'forced checksum \
                                            failure'); END";
    const SQLITE_DROP_FAILURE: &str = "DROP TRIGGER fail_checksum";

    const POSTGRES_FAIL_FROM_BLOCK_4: &str = "CREATE FUNCTION fail_checksum() RETURNS trigger AS $$ BEGIN RAISE \
                                              EXCEPTION 'forced checksum failure'; END; $$ LANGUAGE plpgsql; CREATE \
                                              TRIGGER fail_checksum BEFORE UPDATE OF checksum ON log_status FOR EACH \
                                              ROW WHEN (NEW.block_number >= 4) EXECUTE FUNCTION fail_checksum()";
    const POSTGRES_DROP_FAILURE: &str = "DROP TRIGGER fail_checksum ON log_status; DROP FUNCTION fail_checksum()";

    #[tokio::test]
    async fn test_chunked_log_checksums_match_reference() -> anyhow::Result<()> {
        let db = BlokliDb::new_in_memory().await?;
        check_chunked_checksums_match_reference(&db).await
    }

    #[tokio::test]
    async fn test_failed_checksum_chunk_returns_error_and_keeps_committed_prefix() -> anyhow::Result<()> {
        let db = BlokliDb::new_in_memory().await?;
        check_failed_chunk_keeps_committed_prefix(&db, SQLITE_FAIL_FROM_BLOCK_4, SQLITE_DROP_FAILURE).await
    }

    #[test]
    fn test_checksum_update_rows_stay_within_bind_limit() {
        assert_eq!(
            rows_within_bind_limit(DatabaseBackend::Sqlite, LOG_CHECKSUM_BINDS_PER_ROW),
            333
        );
        assert_eq!(
            rows_within_bind_limit(DatabaseBackend::Postgres, LOG_CHECKSUM_BINDS_PER_ROW),
            21_845
        );
        // A full read chunk takes a bounded number of statements, independent of the log count.
        assert_eq!(
            LOG_CHECKSUM_CHUNK_SIZE
                .div_ceil(rows_within_bind_limit(DatabaseBackend::Sqlite, LOG_CHECKSUM_BINDS_PER_ROW) as u64),
            31
        );
    }

    /// Runs the chunked checksum tests against PostgreSQL. Needs `BLOKLI_TEST_POSTGRES_URL` to point
    /// at a server whose user may create databases; a scratch database is created and dropped.
    #[tokio::test]
    #[ignore = "needs PostgreSQL, set BLOKLI_TEST_POSTGRES_URL and run with --ignored"]
    async fn test_chunked_log_checksums_on_postgres() -> anyhow::Result<()> {
        let admin_url: url::Url = std::env::var("BLOKLI_TEST_POSTGRES_URL")?.parse()?;
        let scratch_db = format!("blokli_log_checksums_{}", std::process::id());
        let mut scratch_url = admin_url.clone();
        scratch_url.set_path(&scratch_db);

        let admin = sea_orm::Database::connect(admin_url.as_str()).await?;
        admin
            .execute_unprepared(&format!("CREATE DATABASE \"{scratch_db}\""))
            .await?;

        let result = async {
            let db = BlokliDb::new(scratch_url.as_str(), None, crate::db::BlokliDbConfig::default()).await?;
            check_chunked_checksums_match_reference(&db).await?;
            check_failed_chunk_keeps_committed_prefix(&db, POSTGRES_FAIL_FROM_BLOCK_4, POSTGRES_DROP_FAILURE).await?;
            anyhow::Ok(())
        }
        .await;

        admin
            .execute_unprepared(&format!("DROP DATABASE IF EXISTS \"{scratch_db}\" WITH (FORCE)"))
            .await?;
        result
    }
}
