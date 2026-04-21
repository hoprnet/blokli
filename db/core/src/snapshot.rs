use std::{
    fs::{self, File},
    io::{BufRead, BufReader, BufWriter, Write},
    path::Path,
};

use blokli_db_entity::{
    log, log_status, log_topic_info,
    prelude::{Log, LogStatus, LogTopicInfo},
};
use chrono::NaiveDateTime;
use sea_orm::{ConnectionTrait, DatabaseBackend, EntityTrait, PaginatorTrait, QueryOrder, Set, Statement};

use crate::{
    BlokliDbGeneralModelOperations, OpenTransaction, TargetDb,
    db::BlokliDb,
    errors::{DbSqlError, Result},
};

pub const SNAPSHOT_SQL_FILE: &str = "hopr_logs.sql";
const SNAPSHOT_PAGE_SIZE: u64 = 10_000;
const IMPORT_BATCH_SIZE: usize = 1_000;
const TIMESTAMP_FORMAT: &str = "%Y-%m-%d %H:%M:%S%.f";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogsSnapshotInfo {
    pub log_count: u64,
    pub log_status_count: u64,
    pub log_topic_info_count: u64,
    pub latest_block: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SnapshotLogRow {
    id: i64,
    tx_index: i64,
    log_index: i64,
    block_number: i64,
    block_hash: Vec<u8>,
    tx_hash: Vec<u8>,
    address: Vec<u8>,
    topics: Vec<u8>,
    data: Vec<u8>,
    removed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SnapshotLogStatusRow {
    id: i64,
    log_id: i64,
    tx_index: i64,
    log_index: i64,
    block_number: i64,
    processed: bool,
    processed_at: Option<NaiveDateTime>,
    checksum: Option<Vec<u8>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SnapshotLogTopicInfoRow {
    id: i64,
    address: Vec<u8>,
    topic: Vec<u8>,
}

#[derive(Debug, Default)]
struct ParsedSnapshot {
    logs: Vec<SnapshotLogRow>,
    statuses: Vec<SnapshotLogStatusRow>,
    topics: Vec<SnapshotLogTopicInfoRow>,
    saw_log_copy: bool,
    saw_log_status_copy: bool,
    saw_log_topic_info_copy: bool,
}

impl ParsedSnapshot {
    fn info(&self) -> LogsSnapshotInfo {
        LogsSnapshotInfo {
            log_count: self.logs.len() as u64,
            log_status_count: self.statuses.len() as u64,
            log_topic_info_count: self.topics.len() as u64,
            latest_block: self.logs.last().map(|row| row.block_number.cast_unsigned()),
        }
    }

    fn ensure_complete(&self) -> Result<()> {
        if !self.saw_log_copy || !self.saw_log_status_copy || !self.saw_log_topic_info_copy {
            return Err(DbSqlError::Construction(
                "hopr_logs.sql must contain COPY sections for log, log_status, and log_topic_info".to_string(),
            ));
        }

        Ok(())
    }
}

fn io_error(context: &str, error: std::io::Error) -> DbSqlError {
    DbSqlError::Construction(format!("{context}: {error}"))
}

fn decode_copy_bytes(value: &str) -> Result<Vec<u8>> {
    let hex = value
        .strip_prefix("\\x")
        .ok_or_else(|| DbSqlError::Construction(format!("expected PostgreSQL bytea hex value, got '{value}'")))?;
    hex::decode(hex).map_err(|error| DbSqlError::Construction(format!("invalid bytea hex value '{value}': {error}")))
}

fn encode_copy_bytes(value: &[u8]) -> String {
    format!("\\x{}", hex::encode(value))
}

fn parse_bool(value: &str) -> Result<bool> {
    match value {
        "t" => Ok(true),
        "f" => Ok(false),
        _ => Err(DbSqlError::Construction(format!(
            "invalid boolean value '{value}' in snapshot"
        ))),
    }
}

fn parse_nullable_timestamp(value: &str) -> Result<Option<NaiveDateTime>> {
    if value == "\\N" {
        return Ok(None);
    }

    NaiveDateTime::parse_from_str(value, TIMESTAMP_FORMAT)
        .map(Some)
        .map_err(|error| DbSqlError::Construction(format!("invalid timestamp '{value}' in snapshot: {error}")))
}

fn parse_nullable_bytes(value: &str) -> Result<Option<Vec<u8>>> {
    if value == "\\N" {
        return Ok(None);
    }

    decode_copy_bytes(value).map(Some)
}

fn split_copy_row(line: &str) -> Vec<&str> {
    line.split('\t').collect()
}

fn parse_log_row(line: &str) -> Result<SnapshotLogRow> {
    let fields = split_copy_row(line);
    if fields.len() != 10 {
        return Err(DbSqlError::Construction(format!(
            "log COPY row must contain 10 fields, got {}",
            fields.len()
        )));
    }

    Ok(SnapshotLogRow {
        id: fields[0]
            .parse()
            .map_err(|error| DbSqlError::Construction(format!("invalid log.id '{}': {error}", fields[0])))?,
        tx_index: fields[1]
            .parse()
            .map_err(|error| DbSqlError::Construction(format!("invalid log.tx_index '{}': {error}", fields[1])))?,
        log_index: fields[2]
            .parse()
            .map_err(|error| DbSqlError::Construction(format!("invalid log.log_index '{}': {error}", fields[2])))?,
        block_number: fields[3]
            .parse()
            .map_err(|error| DbSqlError::Construction(format!("invalid log.block_number '{}': {error}", fields[3])))?,
        block_hash: decode_copy_bytes(fields[4])?,
        tx_hash: decode_copy_bytes(fields[5])?,
        address: decode_copy_bytes(fields[6])?,
        topics: decode_copy_bytes(fields[7])?,
        data: decode_copy_bytes(fields[8])?,
        removed: parse_bool(fields[9])?,
    })
}

fn parse_log_status_row(line: &str) -> Result<SnapshotLogStatusRow> {
    let fields = split_copy_row(line);
    if fields.len() != 8 {
        return Err(DbSqlError::Construction(format!(
            "log_status COPY row must contain 8 fields, got {}",
            fields.len()
        )));
    }

    Ok(SnapshotLogStatusRow {
        id: fields[0]
            .parse()
            .map_err(|error| DbSqlError::Construction(format!("invalid log_status.id '{}': {error}", fields[0])))?,
        log_id: fields[1]
            .parse()
            .map_err(|error| DbSqlError::Construction(format!("invalid log_status.log_id '{}': {error}", fields[1])))?,
        tx_index: fields[2].parse().map_err(|error| {
            DbSqlError::Construction(format!("invalid log_status.tx_index '{}': {error}", fields[2]))
        })?,
        log_index: fields[3].parse().map_err(|error| {
            DbSqlError::Construction(format!("invalid log_status.log_index '{}': {error}", fields[3]))
        })?,
        block_number: fields[4].parse().map_err(|error| {
            DbSqlError::Construction(format!("invalid log_status.block_number '{}': {error}", fields[4]))
        })?,
        processed: parse_bool(fields[5])?,
        processed_at: parse_nullable_timestamp(fields[6])?,
        checksum: parse_nullable_bytes(fields[7])?,
    })
}

fn parse_log_topic_info_row(line: &str) -> Result<SnapshotLogTopicInfoRow> {
    let fields = split_copy_row(line);
    if fields.len() != 3 {
        return Err(DbSqlError::Construction(format!(
            "log_topic_info COPY row must contain 3 fields, got {}",
            fields.len()
        )));
    }

    Ok(SnapshotLogTopicInfoRow {
        id: fields[0]
            .parse()
            .map_err(|error| DbSqlError::Construction(format!("invalid log_topic_info.id '{}': {error}", fields[0])))?,
        address: decode_copy_bytes(fields[1])?,
        topic: decode_copy_bytes(fields[2])?,
    })
}

pub fn inspect_logs_snapshot_sql(sql_path: &Path) -> Result<LogsSnapshotInfo> {
    parse_logs_snapshot_sql(sql_path).map(|snapshot| snapshot.info())
}

fn parse_logs_snapshot_sql(sql_path: &Path) -> Result<ParsedSnapshot> {
    let file = File::open(sql_path).map_err(|error| {
        DbSqlError::Construction(format!(
            "failed to open snapshot SQL file {}: {error}",
            sql_path.display()
        ))
    })?;
    let reader = BufReader::new(file);

    #[derive(Copy, Clone)]
    enum Section {
        None,
        Log,
        LogStatus,
        LogTopicInfo,
    }

    let mut parsed = ParsedSnapshot::default();
    let mut section = Section::None;

    for line in reader.lines() {
        let line = line.map_err(|error| {
            DbSqlError::Construction(format!(
                "failed to read snapshot SQL file {}: {error}",
                sql_path.display()
            ))
        })?;
        let trimmed = line.trim();

        if trimmed.is_empty() || trimmed.starts_with("--") || trimmed.starts_with("SET ") {
            continue;
        }

        if trimmed.starts_with("COPY ") {
            section = if trimmed.starts_with("COPY log ") || trimmed.starts_with("COPY public.log ") {
                parsed.saw_log_copy = true;
                Section::Log
            } else if trimmed.starts_with("COPY log_status ") || trimmed.starts_with("COPY public.log_status ") {
                parsed.saw_log_status_copy = true;
                Section::LogStatus
            } else if trimmed.starts_with("COPY log_topic_info ") || trimmed.starts_with("COPY public.log_topic_info ")
            {
                parsed.saw_log_topic_info_copy = true;
                Section::LogTopicInfo
            } else {
                Section::None
            };
            continue;
        }

        if trimmed == "\\." {
            section = Section::None;
            continue;
        }

        match section {
            Section::None => {}
            Section::Log => parsed.logs.push(parse_log_row(&line)?),
            Section::LogStatus => parsed.statuses.push(parse_log_status_row(&line)?),
            Section::LogTopicInfo => parsed.topics.push(parse_log_topic_info_row(&line)?),
        }
    }

    parsed.ensure_complete()?;
    parsed
        .logs
        .sort_by_key(|row| (row.block_number, row.tx_index, row.log_index));

    Ok(parsed)
}

pub async fn export_logs_snapshot_to_dir(db: &BlokliDb, target_dir: &Path) -> Result<LogsSnapshotInfo> {
    fs::create_dir_all(target_dir).map_err(|error| {
        DbSqlError::Construction(format!(
            "failed to create snapshot export directory {}: {error}",
            target_dir.display()
        ))
    })?;

    let conn = db.conn(TargetDb::Logs);
    let sql_path = target_dir.join(SNAPSHOT_SQL_FILE);
    let file = File::create(&sql_path)
        .map_err(|error| DbSqlError::Construction(format!("failed to create {}: {error}", sql_path.display())))?;
    let mut writer = BufWriter::new(file);

    writer
        .write_all(b"-- Blokli logs snapshot\n\n")
        .map_err(|error| io_error("failed to write snapshot header", error))?;

    let log_count = Log::find().count(conn).await?;
    let log_status_count = LogStatus::find().count(conn).await?;
    let log_topic_info_count = LogTopicInfo::find().count(conn).await?;

    write_log_copy_section(&mut writer, conn).await?;
    write_log_status_copy_section(&mut writer, conn).await?;
    write_log_topic_info_copy_section(&mut writer, conn).await?;

    writer
        .flush()
        .map_err(|error| io_error("failed to flush snapshot SQL file", error))?;

    let latest_log = Log::find()
        .order_by_desc(log::Column::BlockNumber)
        .order_by_desc(log::Column::TxIndex)
        .order_by_desc(log::Column::LogIndex)
        .one(conn)
        .await?;

    Ok(LogsSnapshotInfo {
        log_count,
        log_status_count,
        log_topic_info_count,
        latest_block: latest_log.map(|row| row.block_number.cast_unsigned()),
    })
}

async fn write_log_copy_section(writer: &mut BufWriter<File>, conn: &sea_orm::DatabaseConnection) -> Result<()> {
    writer
        .write_all(
            b"COPY log (id, tx_index, log_index, block_number, block_hash, transaction_hash, address, topics, data, removed) FROM stdin;\n",
        )
        .map_err(|error| io_error("failed to write log COPY header", error))?;

    let paginator = Log::find()
        .order_by_asc(log::Column::Id)
        .paginate(conn, SNAPSHOT_PAGE_SIZE);
    let pages = paginator.num_pages().await?;
    for page in 0..pages {
        for row in paginator.fetch_page(page).await? {
            writeln!(
                writer,
                "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
                row.id,
                row.tx_index,
                row.log_index,
                row.block_number,
                encode_copy_bytes(&row.block_hash),
                encode_copy_bytes(&row.transaction_hash),
                encode_copy_bytes(&row.address),
                encode_copy_bytes(&row.topics),
                encode_copy_bytes(&row.data),
                if row.removed { "t" } else { "f" }
            )
            .map_err(|error| io_error("failed to write log COPY row", error))?;
        }
    }

    writer
        .write_all(b"\\.\n\n")
        .map_err(|error| io_error("failed to terminate log COPY section", error))?;
    Ok(())
}

async fn write_log_status_copy_section(writer: &mut BufWriter<File>, conn: &sea_orm::DatabaseConnection) -> Result<()> {
    writer
        .write_all(
            b"COPY log_status (id, log_id, tx_index, log_index, block_number, processed, processed_at, checksum) FROM stdin;\n",
        )
        .map_err(|error| io_error("failed to write log_status COPY header", error))?;

    let paginator = LogStatus::find()
        .order_by_asc(log_status::Column::Id)
        .paginate(conn, SNAPSHOT_PAGE_SIZE);
    let pages = paginator.num_pages().await?;
    for page in 0..pages {
        for row in paginator.fetch_page(page).await? {
            let processed_at = row
                .processed_at
                .map(|value| value.format(TIMESTAMP_FORMAT).to_string())
                .unwrap_or_else(|| "\\N".to_string());
            let checksum = row
                .checksum
                .as_ref()
                .map(|value| encode_copy_bytes(value))
                .unwrap_or_else(|| "\\N".to_string());
            writeln!(
                writer,
                "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
                row.id,
                row.log_id,
                row.tx_index,
                row.log_index,
                row.block_number,
                if row.processed { "t" } else { "f" },
                processed_at,
                checksum
            )
            .map_err(|error| io_error("failed to write log_status COPY row", error))?;
        }
    }

    writer
        .write_all(b"\\.\n\n")
        .map_err(|error| io_error("failed to terminate log_status COPY section", error))?;
    Ok(())
}

async fn write_log_topic_info_copy_section(
    writer: &mut BufWriter<File>,
    conn: &sea_orm::DatabaseConnection,
) -> Result<()> {
    writer
        .write_all(b"COPY log_topic_info (id, address, topic) FROM stdin;\n")
        .map_err(|error| io_error("failed to write log_topic_info COPY header", error))?;

    let paginator = LogTopicInfo::find()
        .order_by_asc(log_topic_info::Column::Id)
        .paginate(conn, SNAPSHOT_PAGE_SIZE);
    let pages = paginator.num_pages().await?;
    for page in 0..pages {
        for row in paginator.fetch_page(page).await? {
            writeln!(
                writer,
                "{}\t{}\t{}",
                row.id,
                encode_copy_bytes(&row.address),
                encode_copy_bytes(&row.topic)
            )
            .map_err(|error| io_error("failed to write log_topic_info COPY row", error))?;
        }
    }

    writer
        .write_all(b"\\.\n")
        .map_err(|error| io_error("failed to terminate log_topic_info COPY section", error))?;
    Ok(())
}

pub async fn import_logs_snapshot_from_dir(db: &BlokliDb, snapshot_dir: &Path) -> Result<LogsSnapshotInfo> {
    let sql_path = snapshot_dir.join(SNAPSHOT_SQL_FILE);
    if !sql_path.exists() {
        return Err(DbSqlError::Construction(format!(
            "snapshot SQL file not found: {}",
            sql_path.display()
        )));
    }

    let parsed = parse_logs_snapshot_sql(&sql_path)?;
    let info = parsed.info();

    db.nest_transaction_in_db(None, TargetDb::Logs)
        .await?
        .perform(|tx: &OpenTransaction| {
            Box::pin(async move {
                LogStatus::delete_many().exec(tx.as_ref()).await?;
                Log::delete_many().exec(tx.as_ref()).await?;
                LogTopicInfo::delete_many().exec(tx.as_ref()).await?;

                insert_logs(tx.as_ref(), &parsed.logs).await?;
                insert_log_statuses(tx.as_ref(), &parsed.statuses).await?;
                insert_log_topic_infos(tx.as_ref(), &parsed.topics).await?;
                reset_sequences_if_needed(tx.as_ref()).await?;

                Ok::<(), DbSqlError>(())
            })
        })
        .await?;

    Ok(info)
}

async fn insert_logs(tx: &sea_orm::DatabaseTransaction, rows: &[SnapshotLogRow]) -> Result<()> {
    for chunk in rows.chunks(IMPORT_BATCH_SIZE) {
        let models = chunk
            .iter()
            .map(|row| log::ActiveModel {
                id: Set(row.id),
                tx_index: Set(row.tx_index),
                log_index: Set(row.log_index),
                block_number: Set(row.block_number),
                block_hash: Set(row.block_hash.clone()),
                transaction_hash: Set(row.tx_hash.clone()),
                address: Set(row.address.clone()),
                topics: Set(row.topics.clone()),
                data: Set(row.data.clone()),
                removed: Set(row.removed),
            })
            .collect::<Vec<_>>();
        Log::insert_many(models).exec(tx).await?;
    }

    Ok(())
}

async fn insert_log_statuses(tx: &sea_orm::DatabaseTransaction, rows: &[SnapshotLogStatusRow]) -> Result<()> {
    for chunk in rows.chunks(IMPORT_BATCH_SIZE) {
        let models = chunk
            .iter()
            .map(|row| log_status::ActiveModel {
                id: Set(row.id),
                log_id: Set(row.log_id),
                tx_index: Set(row.tx_index),
                log_index: Set(row.log_index),
                block_number: Set(row.block_number),
                processed: Set(row.processed),
                processed_at: Set(row.processed_at),
                checksum: Set(row.checksum.clone()),
            })
            .collect::<Vec<_>>();
        LogStatus::insert_many(models).exec(tx).await?;
    }

    Ok(())
}

async fn insert_log_topic_infos(tx: &sea_orm::DatabaseTransaction, rows: &[SnapshotLogTopicInfoRow]) -> Result<()> {
    for chunk in rows.chunks(IMPORT_BATCH_SIZE) {
        let models = chunk
            .iter()
            .map(|row| log_topic_info::ActiveModel {
                id: Set(row.id),
                address: Set(row.address.clone()),
                topic: Set(row.topic.clone()),
            })
            .collect::<Vec<_>>();
        LogTopicInfo::insert_many(models).exec(tx).await?;
    }

    Ok(())
}

async fn reset_sequences_if_needed(tx: &sea_orm::DatabaseTransaction) -> Result<()> {
    if tx.get_database_backend() != DatabaseBackend::Postgres {
        return Ok(());
    }

    for table in ["log", "log_status", "log_topic_info"] {
        tx.execute_raw(Statement::from_string(
            DatabaseBackend::Postgres,
            format!(
                "SELECT setval(pg_get_serial_sequence('{table}', 'id'), COALESCE((SELECT MAX(id) FROM {table}), 1), \
                 COALESCE((SELECT MAX(id) FROM {table}), 0) > 0)"
            ),
        ))
        .await?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use hopr_types::{
        crypto::types::Hash,
        primitive::prelude::{DateTime, SerializableLog},
    };
    use tempfile::TempDir;

    use super::*;
    use crate::{api::logs::BlokliDbLogOperations, db::BlokliDb};

    fn sample_log(block_number: u64, tx_index: u64, log_index: u64) -> SerializableLog {
        SerializableLog {
            block_number,
            tx_index,
            log_index,
            block_hash: [block_number as u8; 32].into(),
            tx_hash: [tx_index as u8; 32].into(),
            address: [log_index as u8; 20].into(),
            topics: vec![Hash::create(&[b"topic"]).into()],
            data: vec![9, 8, 7].into(),
            removed: false,
            processed: Some(true),
            processed_at: Some(DateTime::from_timestamp_millis(1_700_000_000_000).expect("valid timestamp")),
            checksum: Some("0x1111111111111111111111111111111111111111111111111111111111111111".to_string()),
        }
    }

    #[tokio::test]
    async fn test_snapshot_round_trip() -> Result<()> {
        let db = BlokliDb::new_in_memory().await?;
        db.ensure_logs_origin(vec![(sample_log(10, 1, 0).address, Hash::create(&[b"topic"]))])
            .await?;
        db.store_log(sample_log(10, 1, 0)).await?;
        db.store_log(sample_log(11, 1, 1)).await?;
        db.update_logs_checksums().await?;

        let export_dir = TempDir::new().expect("tempdir");
        let exported = export_logs_snapshot_to_dir(&db, export_dir.path()).await?;

        let imported_db = BlokliDb::new_in_memory().await?;
        let imported = import_logs_snapshot_from_dir(&imported_db, export_dir.path()).await?;

        assert_eq!(exported, imported);
        assert_eq!(imported_db.get_logs_count(None, None).await?, 2);
        assert_eq!(
            imported_db.get_logs_block_numbers(None, None, Some(true)).await?,
            vec![10, 11]
        );

        Ok(())
    }
}
