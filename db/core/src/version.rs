use blokli_db_entity::prelude::{
    Account, AccountState, Announcement, ChainInfo, Channel, ChannelState, HoprBalance, HoprNodeSafeRegistration,
    HoprSafeContract, HoprSafeContractState, HoprSafeRedeemedStats, Log, LogStatus, LogTopicInfo, NativeBalance,
};
use sea_orm::{ConnectionTrait, DatabaseConnection, EntityTrait, Statement};
use tracing::{info, warn};

use crate::errors::{DbSqlError, Result};

/// Current schema version in `"MAJOR.MINOR"` format. This single constant is
/// the only value a developer needs to update when changing the schema.
///
/// **MAJOR** component — bump when the migration stack is structurally rewritten
/// or existing data is fundamentally incompatible with new code. Any increase
/// triggers a **full database wipe** (including `seaql_migrations`) so that all
/// migrations re-run from scratch on the next startup. This check runs
/// **before** migrations.
///
/// **MINOR** component — bump for schema changes that require existing data to be
/// discarded and re-indexed within the current major epoch. Only application
/// data is cleared; `seaql_migrations` is left intact and new migrations run
/// incrementally. This check runs **after** migrations.
///
/// Version history:
/// - `"1.1"`: Initial schema (consolidated from prior migration stack)
pub const SCHEMA_VERSION: &str = "1.1";

/// The singleton ID used for the schema_version table.
const SCHEMA_VERSION_TABLE_ID: i64 = 1;

/// Parse a `"MAJOR.MINOR"` version string into `(major, minor)`.
///
/// Returns `None` for any unexpected format (e.g. old bare integers like `"12"`).
fn parse_version(s: &str) -> Option<(i64, i64)> {
    let (major_str, minor_str) = s.split_once('.')?;
    let major = major_str.trim().parse().ok()?;
    let minor = minor_str.trim().parse().ok()?;
    Some((major, minor))
}

/// Return the current code version parsed from [`SCHEMA_VERSION`] as `(major, minor)`.
///
/// # Panics
///
/// Panics if [`SCHEMA_VERSION`] is not in `"MAJOR.MINOR"` format — this is a
/// developer error caught at startup.
fn code_version() -> (i64, i64) {
    parse_version(SCHEMA_VERSION)
        .unwrap_or_else(|| panic!("SCHEMA_VERSION constant '{SCHEMA_VERSION}' is not in 'MAJOR.MINOR' format"))
}

/// Attempt to read the stored version from the database.
///
/// Returns `None` when the table or row does not exist (fresh install).
/// Any value that cannot be parsed as `"MAJOR.MINOR"` (e.g. a bare integer from
/// an old schema) is treated as `(0, 0)`, which will trigger a full wipe.
async fn try_get_stored_version(db: &DatabaseConnection) -> Result<Option<(i64, i64)>> {
    let stmt = Statement::from_string(
        db.get_database_backend(),
        format!("SELECT version FROM schema_version WHERE id = {SCHEMA_VERSION_TABLE_ID}"),
    );

    let row = match db.query_one_raw(stmt).await {
        Err(_) => return Ok(None),   // Table does not exist → fresh install
        Ok(None) => return Ok(None), // Table exists but no row yet
        Ok(Some(r)) => r,
    };

    let version_str: String = row.try_get("", "version").unwrap_or_default();
    // Unparseable values (e.g. legacy bare integers like "12") fall back to (0, 0)
    // so they always trigger a full wipe on the next startup.
    Ok(Some(parse_version(&version_str).unwrap_or((0, 0))))
}

/// Read the stored version (expects the table to exist; called after migrations).
///
/// # Errors
///
/// Returns an error if the table is empty or the query fails.
async fn get_stored_version(db: &DatabaseConnection) -> Result<(i64, i64)> {
    try_get_stored_version(db)
        .await?
        .ok_or_else(|| DbSqlError::Construction("schema_version table is empty".to_string()))
}

/// Persist a version string to the database.
///
/// The string is stored as-is in the `version` column.
///
/// # Errors
///
/// Returns an error if the update fails.
pub(crate) async fn set_schema_version(db: &DatabaseConnection, version: &str) -> Result<()> {
    db.execute_unprepared(&format!(
        "UPDATE schema_version SET version = '{version}', updated_at = CURRENT_TIMESTAMP WHERE id = \
         {SCHEMA_VERSION_TABLE_ID}"
    ))
    .await?;
    Ok(())
}

/// Check the stored **major** version and perform a full database wipe if it changed.
///
/// Must be called **before** running migrations. If the stored major version
/// is lower than the major component of [`SCHEMA_VERSION`], the entire database
/// is wiped — including `seaql_migrations` and the `schema_version` row — so
/// that all migrations execute from scratch on the next startup.
///
/// An unparseable stored value (e.g. a bare integer from an old schema) is treated
/// as version `(0, 0)`, which always triggers the wipe.
///
/// Returns `Ok(true)` if a full wipe was performed, `Ok(false)` if no action was
/// needed (fresh install or major version already matches).
///
/// # Errors
///
/// Returns an error if the stored major version is *higher* than the code major
/// version (database is from a newer release) or if database operations fail.
pub async fn check_major_version_and_reset(
    db: &DatabaseConnection,
    logs_db: Option<&DatabaseConnection>,
) -> Result<bool> {
    let (stored_major, _) = match try_get_stored_version(db).await? {
        None => return Ok(false), // Fresh install; nothing to wipe.
        Some(v) => v,
    };

    let (code_major, _) = code_version();

    if stored_major < code_major {
        warn!(
            stored_major,
            code_major, "Major schema version changed; wiping all data and migration history for a full re-sync"
        );

        // Clear seaql_migrations on all connections so migrations run from scratch.
        let _ = db.execute_unprepared("DELETE FROM seaql_migrations").await;
        if let Some(logs) = logs_db {
            let _ = logs.execute_unprepared("DELETE FROM seaql_migrations").await;
        }

        // Remove the schema_version row so m001_initial_schema can re-insert it
        // with the correct "MAJOR.MINOR" text value without hitting a PK conflict.
        let _ = db
            .execute_unprepared(&format!(
                "DELETE FROM schema_version WHERE id = {SCHEMA_VERSION_TABLE_ID}"
            ))
            .await;

        // Wipe all application data.
        clear_all_data(db, logs_db).await?;

        info!("Full database wipe complete; migrations will run fresh");
        Ok(true)
    } else if stored_major > code_major {
        Err(DbSqlError::Construction(format!(
            "Database major version {stored_major} is newer than code major version {code_major}. Please upgrade the \
             code."
        )))
    } else {
        Ok(false) // Major version matches; no full wipe needed.
    }
}

/// Check the stored **schema** version and clear data if it changed.
///
/// Must be called **after** running migrations. Compares the minor component of
/// the stored version against the minor component of [`SCHEMA_VERSION`].
/// If the stored minor is lower, all application data is cleared for a re-sync
/// and the stored version is updated to [`SCHEMA_VERSION`]. `seaql_migrations`
/// is not touched.
///
/// # Arguments
///
/// * `db` - Database connection for the index database
/// * `logs_db` - Optional database connection for the logs database (SQLite only)
///
/// # Returns
///
/// Returns `Ok(true)` if data was reset, `Ok(false)` if no action was needed.
///
/// # Errors
///
/// Returns an error if the stored minor version is higher than the code minor
/// version (database is from a newer release) or if database operations fail.
pub async fn check_and_reset_if_needed(db: &DatabaseConnection, logs_db: Option<&DatabaseConnection>) -> Result<bool> {
    let (_, stored_minor) = get_stored_version(db).await?;
    let (_, code_minor) = code_version();

    if stored_minor < code_minor {
        // Version upgrade: clear all data and update version
        warn!(
            stored_minor,
            code_minor, "Schema version increased; clearing data for re-sync"
        );

        clear_all_data(db, logs_db).await?;
        set_schema_version(db, SCHEMA_VERSION).await?;

        info!(new_version = SCHEMA_VERSION, "Schema version updated and data cleared");

        Ok(true)
    } else if stored_minor > code_minor {
        // Version downgrade: reject (code is too old for this database)
        Err(DbSqlError::Construction(format!(
            "Database schema version {stored_minor} is newer than code version {code_minor}. Please upgrade the code \
             to match the database schema."
        )))
    } else {
        // Same version: no action needed
        Ok(false)
    }
}

/// Clear all data from both index and logs databases.
///
/// This function deletes all application data while preserving the schema structure.
/// For the SQLite dual-database setup, it clears both the index and logs databases.
///
/// # Errors
///
/// Returns an error if any database operations fail.
async fn clear_all_data(db: &DatabaseConnection, logs_db: Option<&DatabaseConnection>) -> Result<()> {
    // Clear index database data
    clear_index_data(db).await?;

    // Clear logs database data (if separate)
    if let Some(logs_conn) = logs_db {
        clear_logs_data(logs_conn).await?;
    } else {
        // PostgreSQL: logs are in the same database
        clear_logs_data(db).await?;
    }

    Ok(())
}

/// Clear all data from the index database.
///
/// Deletes rows in dependency order (children before parents) to respect foreign key constraints.
///
/// # Errors
///
/// Returns an error if any database operations fail.
async fn clear_index_data(db: &DatabaseConnection) -> Result<()> {
    ChannelState::delete_many().exec(db).await?;
    Channel::delete_many().exec(db).await?;

    AccountState::delete_many().exec(db).await?;
    Announcement::delete_many().exec(db).await?;
    Account::delete_many().exec(db).await?;

    HoprBalance::delete_many().exec(db).await?;
    NativeBalance::delete_many().exec(db).await?;

    HoprSafeContractState::delete_many().exec(db).await?;
    HoprSafeRedeemedStats::delete_many().exec(db).await?;
    HoprSafeContract::delete_many().exec(db).await?;

    HoprNodeSafeRegistration::delete_many().exec(db).await?;

    ChainInfo::delete_many().exec(db).await?;

    Ok(())
}

/// Clear all data from the logs database.
///
/// # Errors
///
/// Returns an error if any database operations fail.
async fn clear_logs_data(db: &DatabaseConnection) -> Result<()> {
    LogTopicInfo::delete_many().exec(db).await?;
    LogStatus::delete_many().exec(db).await?;
    Log::delete_many().exec(db).await?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use blokli_db_entity::{chain_info, log, prelude::HoprSafeContract};
    use sea_orm::{ActiveModelTrait, EntityTrait, PaginatorTrait, Set};

    use super::*;
    use crate::{BlokliDbGeneralModelOperations, db::BlokliDb};

    #[test]
    fn test_schema_version_constant_is_valid() {
        let (major, minor) = code_version(); // panics if SCHEMA_VERSION is malformed
        assert!(major > 0, "Major schema version must be positive");
        assert!(minor > 0, "Schema version must be positive");
    }

    #[test]
    fn test_schema_version_table_id_is_valid() {
        assert_eq!(
            SCHEMA_VERSION_TABLE_ID, 1,
            "Schema version table should use singleton ID 1"
        );
    }

    #[test]
    fn test_parse_version() {
        assert_eq!(parse_version("1.1"), Some((1, 1)));
        assert_eq!(parse_version("2.5"), Some((2, 5)));
        assert_eq!(parse_version("12"), None); // old bare-integer format
        assert_eq!(parse_version(""), None);
    }

    /// Test that a schema version increase clears all data.
    #[tokio::test]
    async fn test_version_upgrade_clears_data() -> anyhow::Result<()> {
        // Create a temporary file for disk-based SQLite database
        let temp_dir = tempfile::tempdir()?;
        let db_url = format!(
            "sqlite://{}?mode=rwc",
            temp_dir.path().join("test_upgrade.db").display()
        );

        // Create database and run migrations
        let db = BlokliDb::new(&db_url, None, Default::default()).await?;

        // Initialize singletons
        db.ensure_singletons().await?;

        // Insert some test data into chain_info
        chain_info::ActiveModel {
            id: Set(1),
            last_indexed_block: Set(100),
            last_indexed_tx_index: Set(Some(5)),
            last_indexed_log_index: Set(Some(10)),
            ..Default::default()
        }
        .update(db.conn(crate::TargetDb::Index))
        .await?;

        // Verify data exists
        let chain_info_before = ChainInfo::find_by_id(1)
            .one(db.conn(crate::TargetDb::Index))
            .await?
            .unwrap();
        assert_eq!(chain_info_before.last_indexed_block, 100);

        let (code_major, code_minor) = code_version();

        // Verify the stored version matches the current code version after init.
        assert_eq!(
            get_stored_version(db.conn(crate::TargetDb::Index)).await?,
            (code_major, code_minor)
        );

        // Simulate an older minor version to trigger a resync.
        set_schema_version(
            db.conn(crate::TargetDb::Index),
            &format!("{code_major}.{}", code_minor - 1),
        )
        .await?;

        // Now call check_and_reset_if_needed - should reset data
        let was_reset = check_and_reset_if_needed(db.conn(crate::TargetDb::Index), None).await?;
        assert!(was_reset, "Data should have been reset");

        // Recreate singletons after reset (matches production flow)
        db.ensure_singletons().await?;

        // Verify version was updated
        assert_eq!(
            get_stored_version(db.conn(crate::TargetDb::Index)).await?,
            (code_major, code_minor)
        );

        // Verify data was cleared (chain_info should be reset to defaults)
        let chain_info_after = ChainInfo::find_by_id(1)
            .one(db.conn(crate::TargetDb::Index))
            .await?
            .unwrap();
        assert_eq!(
            chain_info_after.last_indexed_block, 0,
            "Data should be reset to default"
        );
        assert_eq!(chain_info_after.last_indexed_tx_index, None);
        assert_eq!(chain_info_after.last_indexed_log_index, None);

        Ok(())
    }

    /// Test that a schema version newer than the code returns an error.
    #[tokio::test]
    async fn test_version_downgrade_fails() -> anyhow::Result<()> {
        // Create a temporary file for disk-based SQLite database
        let temp_dir = tempfile::tempdir()?;
        let db_url = format!(
            "sqlite://{}?mode=rwc",
            temp_dir.path().join("test_downgrade.db").display()
        );

        // Create database and run migrations
        let db = BlokliDb::new(&db_url, None, Default::default()).await?;

        let (code_major, code_minor) = code_version();

        // Set version higher than current
        set_schema_version(
            db.conn(crate::TargetDb::Index),
            &format!("{code_major}.{}", code_minor + 1),
        )
        .await?;

        // Attempt to check version - should fail
        let result = check_and_reset_if_needed(db.conn(crate::TargetDb::Index), None).await;
        assert!(result.is_err(), "Should fail when version is too new");

        // Verify error message mentions version mismatch
        let err_msg = format!("{:?}", result.unwrap_err());
        assert!(
            err_msg.contains("newer than code version"),
            "Error should mention version mismatch"
        );

        Ok(())
    }

    /// Test that the same version preserves all data.
    #[tokio::test]
    async fn test_same_version_preserves_data() -> anyhow::Result<()> {
        // Create a temporary file for disk-based SQLite database
        let temp_dir = tempfile::tempdir()?;
        let db_url = format!(
            "sqlite://{}?mode=rwc",
            temp_dir.path().join("test_same_version.db").display()
        );

        // Create database and run migrations
        let db = BlokliDb::new(&db_url, None, Default::default()).await?;
        // Initialize singletons
        db.ensure_singletons().await?;

        // Insert some test data
        chain_info::ActiveModel {
            id: Set(1),
            last_indexed_block: Set(42),
            last_indexed_tx_index: Set(Some(7)),
            last_indexed_log_index: Set(Some(3)),
            ..Default::default()
        }
        .update(db.conn(crate::TargetDb::Index))
        .await?;

        // Verify version matches current
        assert_eq!(
            get_stored_version(db.conn(crate::TargetDb::Index)).await?,
            code_version()
        );

        // Call check_and_reset_if_needed - should NOT reset
        let was_reset = check_and_reset_if_needed(db.conn(crate::TargetDb::Index), None).await?;
        assert!(!was_reset, "Data should NOT have been reset");

        // Verify data is preserved
        let chain_info_after = ChainInfo::find_by_id(1)
            .one(db.conn(crate::TargetDb::Index))
            .await?
            .unwrap();
        assert_eq!(chain_info_after.last_indexed_block, 42, "Data should be preserved");
        assert_eq!(chain_info_after.last_indexed_tx_index, Some(7));
        assert_eq!(chain_info_after.last_indexed_log_index, Some(3));

        Ok(())
    }

    /// Test dual-database setup (SQLite with separate logs database).
    #[tokio::test]
    async fn test_dual_database_version_check() -> anyhow::Result<()> {
        // Create temporary files for both databases
        let temp_dir = tempfile::tempdir()?;
        let index_url = format!("sqlite://{}?mode=rwc", temp_dir.path().join("test_index.db").display());
        let logs_url = format!("sqlite://{}?mode=rwc", temp_dir.path().join("test_logs.db").display());

        // Create database with dual setup
        let db = BlokliDb::new(&index_url, Some(&logs_url), Default::default()).await?;

        // Initialize singletons
        db.ensure_singletons().await?;

        // Insert test data into both databases
        // Index database: chain_info
        chain_info::ActiveModel {
            id: Set(1),
            last_indexed_block: Set(999),
            ..Default::default()
        }
        .update(db.conn(crate::TargetDb::Index))
        .await?;

        // Logs database: insert a log entry
        log::ActiveModel {
            transaction_hash: Set(vec![0u8; 32]),
            tx_index: Set(0),
            log_index: Set(0),
            block_number: Set(100),
            block_hash: Set(vec![0u8; 32]),
            address: Set(vec![0u8; 20]),
            data: Set(vec![]),
            topics: Set(vec![]),
            ..Default::default()
        }
        .insert(db.conn(crate::TargetDb::Logs))
        .await?;

        // Simulate an older minor version to trigger a resync across both databases.
        let (code_major, code_minor) = code_version();
        set_schema_version(
            db.conn(crate::TargetDb::Index),
            &format!("{code_major}.{}", code_minor - 1),
        )
        .await?;

        // Call version check - should clear both databases
        let was_reset =
            check_and_reset_if_needed(db.conn(crate::TargetDb::Index), Some(db.conn(crate::TargetDb::Logs))).await?;
        assert!(was_reset, "Data should have been reset");

        // Recreate singletons after reset (matches production flow)
        db.ensure_singletons().await?;

        // Verify index data was cleared
        let chain_info_after = ChainInfo::find_by_id(1)
            .one(db.conn(crate::TargetDb::Index))
            .await?
            .unwrap();
        assert_eq!(chain_info_after.last_indexed_block, 0);

        // Verify logs data was cleared
        let log_count = Log::find().count(db.conn(crate::TargetDb::Logs)).await?;
        assert_eq!(log_count, 0, "Logs should be cleared");

        assert_eq!(
            0,
            HoprSafeContract::find().count(db.conn(crate::TargetDb::Index)).await?
        );

        Ok(())
    }
}
