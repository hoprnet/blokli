use std::{
    fs::{self, File},
    io::{BufRead, BufReader, BufWriter, Write},
    path::Path,
};

use blokli_db_entity::{
    log, log_status, log_topic_info,
    prelude::{Log, LogStatus, LogTopicInfo},
};
use chrono::{DateTime, FixedOffset, NaiveDateTime};
use sea_orm::{
    ColumnTrait, ConnectionTrait, DatabaseBackend, EntityTrait, PaginatorTrait, QueryFilter, QueryOrder, QuerySelect,
    Set, Statement,
};

use crate::{
    BlokliDbGeneralModelOperations, OpenTransaction, TargetDb,
    db::BlokliDb,
    errors::{DbSqlError, Result},
};

pub const SNAPSHOT_SQL_FILE: &str = "hopr_logs.sql";
const SNAPSHOT_PAGE_SIZE: u64 = 10_000;
const DEFAULT_IMPORT_BATCH_SIZE: usize = 1_000;
const SQLITE_MAX_VARIABLE_NUMBER: usize = 999;
const LOG_INSERT_COLUMNS: usize = 10;
const LOG_STATUS_INSERT_COLUMNS: usize = 8;
const LOG_TOPIC_INFO_INSERT_COLUMNS: usize = 3;
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
struct SnapshotProgress {
    log_count: u64,
    log_status_count: u64,
    log_topic_info_count: u64,
    latest_block: Option<u64>,
    saw_log_copy: bool,
    saw_log_status_copy: bool,
    saw_log_topic_info_copy: bool,
}

impl SnapshotProgress {
    fn info(&self) -> LogsSnapshotInfo {
        LogsSnapshotInfo {
            log_count: self.log_count,
            log_status_count: self.log_status_count,
            log_topic_info_count: self.log_topic_info_count,
            latest_block: self.latest_block,
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

    fn record_log(&mut self, row: &SnapshotLogRow) {
        self.log_count += 1;
        self.latest_block = u64::try_from(row.block_number).ok().map(|block_number| {
            self.latest_block
                .map_or(block_number, |current| current.max(block_number))
        });
    }

    fn record_log_status(&mut self) {
        self.log_status_count += 1;
    }

    fn record_log_topic_info(&mut self) {
        self.log_topic_info_count += 1;
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

fn validate_copy_bytes(value: &str) -> Result<()> {
    let hex = value
        .strip_prefix("\\x")
        .ok_or_else(|| DbSqlError::Construction(format!("expected PostgreSQL bytea hex value, got '{value}'")))?;
    if hex.len() % 2 != 0 || !hex.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(DbSqlError::Construction(format!(
            "invalid bytea hex value '{value}' in snapshot"
        )));
    }

    Ok(())
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

    if let Ok(parsed) = DateTime::<FixedOffset>::parse_from_str(value, "%Y-%m-%d %H:%M:%S%.f%:z") {
        return Ok(Some(parsed.naive_utc()));
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

fn validate_nonnegative_snapshot_integer(value: &str, field_name: &str) -> Result<()> {
    value
        .parse::<u64>()
        .map_err(|error| DbSqlError::Construction(format!("invalid {field_name} '{value}': {error}")))?;
    Ok(())
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
    validate_logs_snapshot_sql(sql_path)
}

pub fn validate_logs_snapshot_sql(sql_path: &Path) -> Result<LogsSnapshotInfo> {
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

    let mut section = Section::None;
    let mut saw_log_copy = false;
    let mut saw_log_status_copy = false;
    let mut saw_log_topic_info_copy = false;
    let mut log_count = 0_u64;
    let mut log_status_count = 0_u64;
    let mut log_topic_info_count = 0_u64;
    let mut latest_block: Option<u64> = None;

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
                saw_log_copy = true;
                Section::Log
            } else if trimmed.starts_with("COPY log_status ") || trimmed.starts_with("COPY public.log_status ") {
                saw_log_status_copy = true;
                Section::LogStatus
            } else if trimmed.starts_with("COPY log_topic_info ") || trimmed.starts_with("COPY public.log_topic_info ")
            {
                saw_log_topic_info_copy = true;
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
            Section::Log => {
                let fields = split_copy_row(&line);
                if fields.len() != 10 {
                    return Err(DbSqlError::Construction(format!(
                        "log COPY row must contain 10 fields, got {}",
                        fields.len()
                    )));
                }
                validate_nonnegative_snapshot_integer(fields[0], "log.id")?;
                validate_nonnegative_snapshot_integer(fields[1], "log.tx_index")?;
                validate_nonnegative_snapshot_integer(fields[2], "log.log_index")?;
                let block_number = fields[3].parse::<u64>().map_err(|error| {
                    DbSqlError::Construction(format!("invalid log.block_number '{}': {error}", fields[3]))
                })?;
                for value in [&fields[4], &fields[5], &fields[6], &fields[7], &fields[8]] {
                    validate_copy_bytes(value)?;
                }
                parse_bool(fields[9])?;
                latest_block = Some(latest_block.map_or(block_number, |current| current.max(block_number)));
                log_count += 1;
            }
            Section::LogStatus => {
                let fields = split_copy_row(&line);
                if fields.len() != 8 {
                    return Err(DbSqlError::Construction(format!(
                        "log_status COPY row must contain 8 fields, got {}",
                        fields.len()
                    )));
                }
                for value in fields.iter().take(5) {
                    value.parse::<i64>().map_err(|error| {
                        DbSqlError::Construction(format!("invalid log_status field '{}': {error}", value))
                    })?;
                }
                parse_bool(fields[5])?;
                parse_nullable_timestamp(fields[6])?;
                parse_nullable_bytes(fields[7])?;
                log_status_count += 1;
            }
            Section::LogTopicInfo => {
                let fields = split_copy_row(&line);
                if fields.len() != 3 {
                    return Err(DbSqlError::Construction(format!(
                        "log_topic_info COPY row must contain 3 fields, got {}",
                        fields.len()
                    )));
                }
                fields[0].parse::<i64>().map_err(|error| {
                    DbSqlError::Construction(format!("invalid log_topic_info.id '{}': {error}", fields[0]))
                })?;
                validate_copy_bytes(fields[1])?;
                validate_copy_bytes(fields[2])?;
                log_topic_info_count += 1;
            }
        }
    }

    if !matches!(section, Section::None) {
        return Err(DbSqlError::Construction(format!(
            "snapshot SQL file {} ended before terminating a COPY section",
            sql_path.display()
        )));
    }

    if !saw_log_copy || !saw_log_status_copy || !saw_log_topic_info_copy {
        return Err(DbSqlError::Construction(
            "hopr_logs.sql must contain COPY sections for log, log_status, and log_topic_info".to_string(),
        ));
    }

    Ok(LogsSnapshotInfo {
        log_count,
        log_status_count,
        log_topic_info_count,
        latest_block,
    })
}

fn import_batch_size(backend: DatabaseBackend, nr_of_columns: usize) -> usize {
    if backend == DatabaseBackend::Sqlite {
        (SQLITE_MAX_VARIABLE_NUMBER / nr_of_columns).max(1)
    } else {
        DEFAULT_IMPORT_BATCH_SIZE
    }
}

async fn import_logs_snapshot_sql(tx: &sea_orm::DatabaseTransaction, sql_path: &Path) -> Result<LogsSnapshotInfo> {
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

    let mut progress = SnapshotProgress::default();
    let mut section = Section::None;
    let mut logs = Vec::new();
    let mut statuses = Vec::new();
    let mut topics = Vec::new();
    let log_batch_size = import_batch_size(tx.get_database_backend(), LOG_INSERT_COLUMNS);
    let log_status_batch_size = import_batch_size(tx.get_database_backend(), LOG_STATUS_INSERT_COLUMNS);
    let log_topic_info_batch_size = import_batch_size(tx.get_database_backend(), LOG_TOPIC_INFO_INSERT_COLUMNS);

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
                progress.saw_log_copy = true;
                Section::Log
            } else if trimmed.starts_with("COPY log_status ") || trimmed.starts_with("COPY public.log_status ") {
                progress.saw_log_status_copy = true;
                Section::LogStatus
            } else if trimmed.starts_with("COPY log_topic_info ") || trimmed.starts_with("COPY public.log_topic_info ")
            {
                progress.saw_log_topic_info_copy = true;
                Section::LogTopicInfo
            } else {
                Section::None
            };
            continue;
        }

        if trimmed == "\\." {
            match section {
                Section::None => {}
                Section::Log => {
                    if !logs.is_empty() {
                        insert_logs(tx, &logs).await?;
                        logs.clear();
                    }
                }
                Section::LogStatus => {
                    if !statuses.is_empty() {
                        insert_log_statuses(tx, &statuses).await?;
                        statuses.clear();
                    }
                }
                Section::LogTopicInfo => {
                    if !topics.is_empty() {
                        insert_log_topic_infos(tx, &topics).await?;
                        topics.clear();
                    }
                }
            }
            section = Section::None;
            continue;
        }

        match section {
            Section::None => {}
            Section::Log => {
                let row = parse_log_row(&line)?;
                progress.record_log(&row);
                logs.push(row);
                if logs.len() >= log_batch_size {
                    insert_logs(tx, &logs).await?;
                    logs.clear();
                }
            }
            Section::LogStatus => {
                let row = parse_log_status_row(&line)?;
                progress.record_log_status();
                statuses.push(row);
                if statuses.len() >= log_status_batch_size {
                    insert_log_statuses(tx, &statuses).await?;
                    statuses.clear();
                }
            }
            Section::LogTopicInfo => {
                let row = parse_log_topic_info_row(&line)?;
                progress.record_log_topic_info();
                topics.push(row);
                if topics.len() >= log_topic_info_batch_size {
                    insert_log_topic_infos(tx, &topics).await?;
                    topics.clear();
                }
            }
        }
    }

    if !matches!(section, Section::None) {
        return Err(DbSqlError::Construction(format!(
            "snapshot SQL file {} ended before terminating a COPY section",
            sql_path.display()
        )));
    }

    if !logs.is_empty() {
        insert_logs(tx, &logs).await?;
    }
    if !statuses.is_empty() {
        insert_log_statuses(tx, &statuses).await?;
    }
    if !topics.is_empty() {
        insert_log_topic_infos(tx, &topics).await?;
    }

    progress.ensure_complete()?;
    Ok(progress.info())
}

pub async fn export_logs_snapshot_to_dir(db: &BlokliDb, target_dir: &Path) -> Result<LogsSnapshotInfo> {
    fs::create_dir_all(target_dir).map_err(|error| {
        DbSqlError::Construction(format!(
            "failed to create snapshot export directory {}: {error}",
            target_dir.display()
        ))
    })?;

    let tx = db.begin_transaction_in_db(TargetDb::Logs).await?;
    let conn = tx.as_ref();
    if conn.get_database_backend() == DatabaseBackend::Postgres {
        conn.execute_raw(Statement::from_string(
            DatabaseBackend::Postgres,
            "SET TRANSACTION ISOLATION LEVEL REPEATABLE READ READ ONLY".to_string(),
        ))
        .await?;
    }

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

    let info = LogsSnapshotInfo {
        log_count,
        log_status_count,
        log_topic_info_count,
        latest_block: latest_log.and_then(|row| u64::try_from(row.block_number).ok()),
    };

    tx.commit().await?;

    Ok(info)
}

async fn write_log_copy_section<C>(writer: &mut BufWriter<File>, conn: &C) -> Result<()>
where
    C: ConnectionTrait,
{
    writer
        .write_all(
            b"COPY log (id, tx_index, log_index, block_number, block_hash, transaction_hash, address, topics, data, removed) FROM stdin;\n",
        )
        .map_err(|error| io_error("failed to write log COPY header", error))?;

    let mut last_id = None;
    loop {
        let mut query = Log::find().order_by_asc(log::Column::Id).limit(SNAPSHOT_PAGE_SIZE);
        if let Some(id) = last_id {
            query = query.filter(log::Column::Id.gt(id));
        }
        let rows = query.all(conn).await?;
        if rows.is_empty() {
            break;
        }

        last_id = rows.last().map(|row| row.id);
        for row in rows {
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

async fn write_log_status_copy_section<C>(writer: &mut BufWriter<File>, conn: &C) -> Result<()>
where
    C: ConnectionTrait,
{
    writer
        .write_all(
            b"COPY log_status (id, log_id, tx_index, log_index, block_number, processed, processed_at, checksum) FROM stdin;\n",
        )
        .map_err(|error| io_error("failed to write log_status COPY header", error))?;

    let mut last_id = None;
    loop {
        let mut query = LogStatus::find()
            .order_by_asc(log_status::Column::Id)
            .limit(SNAPSHOT_PAGE_SIZE);
        if let Some(id) = last_id {
            query = query.filter(log_status::Column::Id.gt(id));
        }
        let rows = query.all(conn).await?;
        if rows.is_empty() {
            break;
        }

        last_id = rows.last().map(|row| row.id);
        for row in rows {
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

async fn write_log_topic_info_copy_section<C>(writer: &mut BufWriter<File>, conn: &C) -> Result<()>
where
    C: ConnectionTrait,
{
    writer
        .write_all(b"COPY log_topic_info (id, address, topic) FROM stdin;\n")
        .map_err(|error| io_error("failed to write log_topic_info COPY header", error))?;

    let mut last_id = None;
    loop {
        let mut query = LogTopicInfo::find()
            .order_by_asc(log_topic_info::Column::Id)
            .limit(SNAPSHOT_PAGE_SIZE);
        if let Some(id) = last_id {
            query = query.filter(log_topic_info::Column::Id.gt(id));
        }
        let rows = query.all(conn).await?;
        if rows.is_empty() {
            break;
        }

        last_id = rows.last().map(|row| row.id);
        for row in rows {
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

    db.nest_transaction_in_db(None, TargetDb::Logs)
        .await?
        .perform(|tx: &OpenTransaction| {
            Box::pin(async move {
                clear_logs_tables(tx.as_ref()).await?;
                let info = import_logs_snapshot_sql(tx.as_ref(), &sql_path).await?;
                reset_sequences_if_needed(tx.as_ref()).await?;
                Ok::<LogsSnapshotInfo, DbSqlError>(info)
            })
        })
        .await
}

async fn insert_logs(tx: &sea_orm::DatabaseTransaction, rows: &[SnapshotLogRow]) -> Result<()> {
    let batch_size = import_batch_size(tx.get_database_backend(), LOG_INSERT_COLUMNS);
    for chunk in rows.chunks(batch_size) {
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
    let batch_size = import_batch_size(tx.get_database_backend(), LOG_STATUS_INSERT_COLUMNS);
    for chunk in rows.chunks(batch_size) {
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
    let batch_size = import_batch_size(tx.get_database_backend(), LOG_TOPIC_INFO_INSERT_COLUMNS);
    for chunk in rows.chunks(batch_size) {
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

async fn clear_logs_tables(tx: &sea_orm::DatabaseTransaction) -> Result<()> {
    if tx.get_database_backend() == DatabaseBackend::Postgres {
        tx.execute_raw(Statement::from_string(
            DatabaseBackend::Postgres,
            "TRUNCATE TABLE log_status, log, log_topic_info RESTART IDENTITY CASCADE".to_string(),
        ))
        .await?;
        return Ok(());
    }

    LogStatus::delete_many().exec(tx).await?;
    Log::delete_many().exec(tx).await?;
    LogTopicInfo::delete_many().exec(tx).await?;
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
    use std::fs;

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

    #[tokio::test]
    async fn test_snapshot_round_trip_large_sqlite_import() -> Result<()> {
        let db = BlokliDb::new_in_memory().await?;
        let sample_origin = (sample_log(10, 1, 0).address, Hash::create(&[b"topic"]));
        db.ensure_logs_origin(vec![sample_origin]).await?;
        for index in 0..150_u64 {
            let mut log = sample_log(10 + index, 1, index);
            log.address = sample_origin.0;
            db.store_log(log).await?;
        }
        db.update_logs_checksums().await?;

        let export_dir = TempDir::new().expect("tempdir");
        let exported = export_logs_snapshot_to_dir(&db, export_dir.path()).await?;

        let imported_db = BlokliDb::new_in_memory().await?;
        let imported = import_logs_snapshot_from_dir(&imported_db, export_dir.path()).await?;

        assert_eq!(exported, imported);
        assert_eq!(imported_db.get_logs_count(None, None).await?, 150);

        Ok(())
    }

    #[test]
    fn test_validate_logs_snapshot_sql_rejects_truncated_copy_section() {
        let temp_dir = TempDir::new().expect("tempdir");
        let sql_path = temp_dir.path().join(SNAPSHOT_SQL_FILE);
        fs::write(
            &sql_path,
            "COPY log (id, tx_index, log_index, block_number, block_hash, transaction_hash, address, topics, data, \
             removed) FROM \
             stdin;\n1\t1\t1\t1\t\\x0000000000000000000000000000000000000000000000000000000000000000\t\\\
             x0000000000000000000000000000000000000000000000000000000000000001\t\\\
             x0000000000000000000000000000000000000001\t\\x010203\t\\x0405\tf\n",
        )
        .expect("write truncated snapshot");

        let error = validate_logs_snapshot_sql(&sql_path).expect_err("truncated snapshot should fail validation");
        assert!(error.to_string().contains("ended before terminating a COPY section"));
    }

    #[test]
    fn test_validate_logs_snapshot_sql_rejects_negative_log_identifiers() {
        let temp_dir = TempDir::new().expect("tempdir");
        let sql_path = temp_dir.path().join(SNAPSHOT_SQL_FILE);
        fs::write(
            &sql_path,
            "COPY log (id, tx_index, log_index, block_number, block_hash, transaction_hash, address, topics, data, \
             removed) FROM \
             stdin;\n-1\t1\t1\t1\t\\x0000000000000000000000000000000000000000000000000000000000000000\t\\\
             x0000000000000000000000000000000000000000000000000000000000000001\t\\\
             x0000000000000000000000000000000000000001\t\\x010203\t\\x0405\tf\n\\.\nCOPY log_status (id, log_id, \
             tx_index, log_index, block_number, processed, processed_at, checksum) FROM \
             stdin;\n1\t1\t1\t1\t1\tt\t2026-01-01 \
             00:00:00.000000\t\\x1111111111111111111111111111111111111111111111111111111111111111\n\\.\nCOPY \
             log_topic_info (id, address, topic) FROM \
             stdin;\n1\t\\x0000000000000000000000000000000000000001\t\\\
             xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\n\\.\n",
        )
        .expect("write invalid snapshot");

        let error = validate_logs_snapshot_sql(&sql_path).expect_err("negative log.id should fail validation");
        assert!(error.to_string().contains("invalid log.id '-1'"));
    }

    #[tokio::test]
    async fn test_import_logs_snapshot_from_dir_rejects_truncated_copy_section() -> Result<()> {
        let temp_dir = TempDir::new().expect("tempdir");
        let sql_path = temp_dir.path().join(SNAPSHOT_SQL_FILE);
        fs::write(
            &sql_path,
            "COPY log (id, tx_index, log_index, block_number, block_hash, transaction_hash, address, topics, data, \
             removed) FROM \
             stdin;\n1\t1\t1\t1\t\\x0000000000000000000000000000000000000000000000000000000000000000\t\\\
             x0000000000000000000000000000000000000000000000000000000000000001\t\\\
             x0000000000000000000000000000000000000001\t\\x010203\t\\x0405\tf\nCOPY log_status (id, log_id, tx_index, \
             log_index, block_number, processed, processed_at, checksum) FROM stdin;\n1\t1\t1\t1\t1\tt\t2026-01-01 \
             00:00:00.000000\t\\x1111111111111111111111111111111111111111111111111111111111111111\n\\.\nCOPY \
             log_topic_info (id, address, topic) FROM \
             stdin;\n1\t\\x0000000000000000000000000000000000000001\t\\\
             xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\n\\.\n",
        )
        .expect("write truncated snapshot");

        let db = BlokliDb::new_in_memory().await?;
        let error = import_logs_snapshot_from_dir(&db, temp_dir.path())
            .await
            .expect_err("truncated snapshot should fail import");
        assert!(error.to_string().contains("ended before terminating a COPY section"));
        assert_eq!(db.get_logs_count(None, None).await?, 0);

        Ok(())
    }
}
