use blokli_db_entity::prelude::{
    Account, AccountState, Announcement, ChainInfo, Channel, ChannelState, HoprBalance, HoprNodeSafeRegistration,
    HoprSafeContract, HoprSafeContractState, HoprSafeRedeemedStats, Log, LogStatus, LogTopicInfo, NativeBalance,
    ServiceEntry, ServiceEntryState, ServiceRegistryConfig, ServiceType as ServiceTypeEntity, ServiceTypeState,
};
use migration::{Migrator, MigratorChainLogs, MigratorIndex, MigratorTrait};
use sea_orm::{ConnectionTrait, DatabaseConnection, EntityTrait, Statement};
use semver::Version;
use tracing::info;

use crate::errors::{DbSqlError, Result};

/// Current schema version in SemVer `"MAJOR.MINOR.PATCH"` format. This single constant is
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
/// **PATCH** component — reserved for future use; currently always `0`.
///
/// Version history:
/// - `"1.1.0"`: Initial schema (consolidated from prior migration stack)
/// - `"1.2.0"`:
/// - `"1.3.0"`: SC set updated through a binding update.
/// - `"1.4.0"`: Service registry contract added to the indexed set.
pub const SCHEMA_VERSION: &str = "1.4.0";

/// The singleton ID used for the schema_version table.
const SCHEMA_VERSION_TABLE_ID: i64 = 1;

/// Return the current code version parsed from [`SCHEMA_VERSION`].
///
/// # Panics
///
/// Panics if [`SCHEMA_VERSION`] is not valid SemVer — this is a developer error caught at startup.
fn code_version() -> Version {
    Version::parse(SCHEMA_VERSION)
        .unwrap_or_else(|_| panic!("SCHEMA_VERSION constant '{SCHEMA_VERSION}' is not valid semver"))
}

/// Attempt to read the stored version from the database.
///
/// Returns `None` when the table or row does not exist (fresh install).
/// Any value that cannot be parsed as SemVer is treated as `0.0.0`, which
/// always triggers a full wipe on the next startup.
async fn try_get_stored_version(db: &DatabaseConnection) -> Result<Option<Version>> {
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
    let version = Version::parse(&version_str).unwrap_or_else(|_| Version::new(0, 0, 0));
    Ok(Some(version))
}

/// Read the stored version (expects the table to exist; called after migrations).
///
/// # Errors
///
/// Returns an error if the table is empty or the query fails.
async fn get_stored_version(db: &DatabaseConnection) -> Result<Version> {
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
/// is wiped — including `seaql_migrations` and the `schema_version` row — and
/// migrations are re-applied from scratch.
///
/// An unparseable stored value (e.g. a bare integer from an old schema) is treated
/// as version `0.0.0`, which always triggers the wipe.
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
    let stored = match try_get_stored_version(db).await? {
        None => return Ok(false), // Fresh install; nothing to wipe.
        Some(v) => v,
    };

    let code = code_version();

    if stored.major < code.major {
        info!(
            stored_major = stored.major,
            code_major = code.major,
            "Major schema version changed; resetting database schema for a full re-sync"
        );

        reset_database_schema_and_migrations(db, logs_db).await?;

        info!("Full database wipe complete; migrations will run fresh");
        Ok(true)
    } else if stored.major > code.major {
        Err(DbSqlError::Construction(format!(
            "Database major version {} is newer than code major version {}. Please upgrade the code.",
            stored.major, code.major
        )))
    } else {
        Ok(false) // Major version matches; no full wipe needed.
    }
}

async fn reset_database_schema_and_migrations(
    db: &DatabaseConnection,
    logs_db: Option<&DatabaseConnection>,
) -> Result<()> {
    if matches!(db.get_database_backend(), sea_orm::DbBackend::Sqlite) {
        if let Some(logs_conn) = logs_db {
            MigratorIndex::fresh(db).await?;
            MigratorChainLogs::fresh(logs_conn).await?;
            return Ok(());
        }
    }

    Migrator::fresh(db).await?;
    Ok(())
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
    let stored = get_stored_version(db).await?;
    let code = code_version();

    if stored.minor < code.minor {
        // Version upgrade: clear all data and update version
        info!(
            stored_minor = stored.minor,
            code_minor = code.minor,
            "Schema version increased; clearing data for re-sync"
        );

        clear_all_data(db, logs_db).await?;
        set_schema_version(db, SCHEMA_VERSION).await?;

        info!(new_version = SCHEMA_VERSION, "Schema version updated and data cleared");

        Ok(true)
    } else if stored.minor > code.minor {
        // Version downgrade: reject (code is too old for this database)
        Err(DbSqlError::Construction(format!(
            "Database schema version {} is newer than code version {}. Please upgrade the code to match the database \
             schema.",
            stored.minor, code.minor
        )))
    } else {
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

    ServiceEntryState::delete_many().exec(db).await?;
    ServiceEntry::delete_many().exec(db).await?;
    ServiceTypeState::delete_many().exec(db).await?;
    ServiceTypeEntity::delete_many().exec(db).await?;
    ServiceRegistryConfig::delete_many().exec(db).await?;

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
    use blokli_db_entity::{
        chain_info, log, prelude::HoprSafeContract, service_entry, service_entry_state, service_type,
        service_type_state,
    };
    use sea_orm::{ActiveModelTrait, DbBackend, EntityTrait, PaginatorTrait, Set, Statement};

    use super::*;
    use crate::{BlokliDbGeneralModelOperations, db::BlokliDb};

    #[test]
    fn test_schema_version_table_id_is_valid() {
        assert_eq!(
            SCHEMA_VERSION_TABLE_ID, 1,
            "Schema version table should use singleton ID 1"
        );
    }

    async fn sqlite_object_exists(
        db: &DatabaseConnection,
        object_type: &str,
        object_name: &str,
    ) -> anyhow::Result<bool> {
        let escaped_name = object_name.replace('\'', "''");
        let stmt = Statement::from_string(
            DbBackend::Sqlite,
            format!("SELECT name FROM sqlite_master WHERE type = '{object_type}' AND name = '{escaped_name}' LIMIT 1"),
        );
        Ok(db.query_one_raw(stmt).await?.is_some())
    }

    #[tokio::test]
    async fn test_major_version_upgrade_resets_schema() -> anyhow::Result<()> {
        let temp_dir = tempfile::tempdir()?;
        let db_url = format!(
            "sqlite://{}?mode=rwc",
            temp_dir.path().join("test_major_reset.db").display()
        );

        let db = BlokliDb::new(&db_url, None, Default::default()).await?;
        db.ensure_singletons().await?;

        db.conn(crate::TargetDb::Index)
            .execute_unprepared("CREATE TABLE legacy_artifact (id INTEGER PRIMARY KEY)")
            .await?;

        assert!(sqlite_object_exists(db.conn(crate::TargetDb::Index), "table", "account").await?);
        assert!(sqlite_object_exists(db.conn(crate::TargetDb::Index), "table", "legacy_artifact").await?);

        let code = code_version();
        set_schema_version(db.conn(crate::TargetDb::Index), &format!("0.{}.0", code.minor)).await?;

        let did_reset = check_major_version_and_reset(db.conn(crate::TargetDb::Index), None).await?;
        assert!(did_reset, "Major version upgrade should trigger a schema reset");

        assert!(!sqlite_object_exists(db.conn(crate::TargetDb::Index), "table", "legacy_artifact").await?);
        assert!(sqlite_object_exists(db.conn(crate::TargetDb::Index), "table", "account").await?);
        assert!(sqlite_object_exists(db.conn(crate::TargetDb::Index), "table", "seaql_migrations").await?);

        let stored_after_major_reset = get_stored_version(db.conn(crate::TargetDb::Index)).await?;
        assert_eq!(
            stored_after_major_reset.major, code.major,
            "Major reset should recreate the schema in the current major epoch"
        );
        assert!(
            stored_after_major_reset <= code,
            "Fresh migrations should not produce a schema version newer than the running code"
        );

        let did_minor_reset = check_and_reset_if_needed(db.conn(crate::TargetDb::Index), None).await?;
        assert_eq!(
            did_minor_reset,
            stored_after_major_reset.minor < code.minor,
            "Post-migration reset should only run when the recreated schema is from an older minor version"
        );
        assert_eq!(get_stored_version(db.conn(crate::TargetDb::Index)).await?, code);

        Ok(())
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

        let code = code_version();

        // Verify the stored version matches the current code version after init.
        assert_eq!(get_stored_version(db.conn(crate::TargetDb::Index)).await?, code);

        // Simulate an older minor version to trigger a resync.
        set_schema_version(
            db.conn(crate::TargetDb::Index),
            &format!("{}.{}.0", code.major, code.minor - 1),
        )
        .await?;

        // Now call check_and_reset_if_needed - should reset data
        let was_reset = check_and_reset_if_needed(db.conn(crate::TargetDb::Index), None).await?;
        assert!(was_reset, "Data should have been reset");

        // Recreate singletons after reset (matches production flow)
        db.ensure_singletons().await?;

        // Verify version was updated
        assert_eq!(get_stored_version(db.conn(crate::TargetDb::Index)).await?, code);
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

    /// The `1.3.0` -> `1.4.0` upgrade adds the service registry to the indexed contract set, so
    /// the stored logs no longer cover every `(address, topic)` pair the indexer asks for. The
    /// minor bump must therefore clear the logs *and* the service registry tables, letting
    /// `ensure_logs_origin` prime the new topic set on an empty logs database.
    #[tokio::test]
    async fn test_service_registry_upgrade_clears_logs_and_service_data() -> anyhow::Result<()> {
        /// The schema version that shipped before the service registry was indexed.
        const PRE_SERVICE_REGISTRY_VERSION: &str = "1.3.0";

        let temp_dir = tempfile::tempdir()?;
        let index_url = format!(
            "sqlite://{}?mode=rwc",
            temp_dir.path().join("test_service_index.db").display()
        );
        let logs_url = format!(
            "sqlite://{}?mode=rwc",
            temp_dir.path().join("test_service_logs.db").display()
        );

        let db = BlokliDb::new(&index_url, Some(&logs_url), Default::default()).await?;
        db.ensure_singletons().await?;

        let index_conn = db.conn(crate::TargetDb::Index);
        let logs_conn = db.conn(crate::TargetDb::Logs);

        let service_type_id = service_type::ActiveModel {
            service_type: Set(vec![0xAA; 32]),
            ..Default::default()
        }
        .insert(index_conn)
        .await?
        .id;

        service_type_state::ActiveModel {
            service_type_id: Set(service_type_id),
            owner_address: Set(Some(vec![0x11; 20])),
            requirement_address: Set(None),
            registration_burn: Set(vec![0u8; 32]),
            update_burn: Set(vec![0u8; 32]),
            published_block: Set(10),
            published_tx_index: Set(0),
            published_log_index: Set(0),
            ..Default::default()
        }
        .insert(index_conn)
        .await?;

        let service_entry_id = service_entry::ActiveModel {
            service_type_id: Set(service_type_id),
            node_address: Set(vec![0x22; 20]),
            ..Default::default()
        }
        .insert(index_conn)
        .await?
        .id;

        service_entry_state::ActiveModel {
            service_entry_id: Set(service_entry_id),
            safe_address: Set(Some(vec![0x33; 20])),
            metadata: Set(Some(vec![1, 2, 3])),
            registered_at: Set(None),
            updated_at: Set(None),
            deregistered: Set(false),
            published_block: Set(10),
            published_tx_index: Set(0),
            published_log_index: Set(1),
            ..Default::default()
        }
        .insert(index_conn)
        .await?;

        log::ActiveModel {
            transaction_hash: Set(vec![0u8; 32]),
            tx_index: Set(0),
            log_index: Set(0),
            block_number: Set(10),
            block_hash: Set(vec![0u8; 32]),
            address: Set(vec![0u8; 20]),
            data: Set(vec![]),
            topics: Set(vec![]),
            ..Default::default()
        }
        .insert(logs_conn)
        .await?;

        // The singleton config row is created by `ensure_singletons`.
        assert_eq!(ServiceRegistryConfig::find().count(index_conn).await?, 1);

        let stored = Version::parse(PRE_SERVICE_REGISTRY_VERSION)?;
        assert!(
            stored.minor < code_version().minor,
            "the pre-service-registry version must be older than the code version"
        );
        set_schema_version(index_conn, PRE_SERVICE_REGISTRY_VERSION).await?;

        let was_reset = check_and_reset_if_needed(index_conn, Some(logs_conn)).await?;
        assert!(was_reset, "a minor version increase should clear the data");

        assert_eq!(ServiceEntryState::find().count(index_conn).await?, 0);
        assert_eq!(ServiceEntry::find().count(index_conn).await?, 0);
        assert_eq!(ServiceTypeState::find().count(index_conn).await?, 0);
        assert_eq!(ServiceTypeEntity::find().count(index_conn).await?, 0);
        assert_eq!(ServiceRegistryConfig::find().count(index_conn).await?, 0);

        assert_eq!(Log::find().count(logs_conn).await?, 0);
        assert_eq!(LogStatus::find().count(logs_conn).await?, 0);
        assert_eq!(LogTopicInfo::find().count(logs_conn).await?, 0);

        assert_eq!(get_stored_version(index_conn).await?, code_version());

        // Production recreates the singletons after a reset.
        db.ensure_singletons().await?;
        assert_eq!(ServiceRegistryConfig::find().count(index_conn).await?, 1);

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

        let code = code_version();

        // Set version higher than current
        set_schema_version(
            db.conn(crate::TargetDb::Index),
            &format!("{}.{}.0", code.major, code.minor + 1),
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
        let code = code_version();
        set_schema_version(
            db.conn(crate::TargetDb::Index),
            &format!("{}.{}.0", code.major, code.minor - 1),
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
