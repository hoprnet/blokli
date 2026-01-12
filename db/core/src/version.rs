use blokli_db_entity::{
    hopr_safe_contract::Column as SafeContractColumn,
    prelude::{
        Account, AccountState, Announcement, ChainInfo, Channel, ChannelState, HoprBalance, HoprSafeContract, Log,
        LogStatus, LogTopicInfo, NativeBalance,
    },
};
use migration::MIGRATION_MARKER_BLOCK_ID;
use sea_orm::{ColumnTrait, ConnectionTrait, DatabaseConnection, EntityTrait, QueryFilter, Statement};
use tracing::{info, warn};

use crate::errors::{DbSqlError, Result};

/// Current schema version expected by this code.
/// Bump this constant when making schema changes that require data reset.
///
/// Version history:
/// - 1: Initial schema with INTEGER id columns
/// - 2: Changed to BIGINT id columns (m023, m024)
/// - 3: Added module_address and chain_key to hopr_safe_contract (m026)
/// - 4: Change contracts addresses
/// - 5: NodeSafeRegistered events are now also possible safe creation events
/// - 6: Changed channel status representation to smallint
/// - 7: Update account indexing to include safe address once deployed
/// - 8: Add v3 Safe deployment data
pub const CURRENT_SCHEMA_VERSION: i64 = 8;

/// The singleton ID used for the schema_version table
const SCHEMA_VERSION_TABLE_ID: i64 = 1;

/// Check the stored schema version and reset data if needed.
///
/// This function compares the stored schema version in the database with
/// `CURRENT_SCHEMA_VERSION`. If the stored version is lower, it clears all
/// data and updates the version. If the stored version is higher, it returns
/// an error (code is too old for this database).
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
/// Returns an error if:
/// - The stored version is higher than CURRENT_SCHEMA_VERSION
/// - Database operations fail
pub async fn check_and_reset_if_needed(db: &DatabaseConnection, logs_db: Option<&DatabaseConnection>) -> Result<bool> {
    let stored_version = get_stored_version(db).await?;

    if stored_version < CURRENT_SCHEMA_VERSION {
        // Version upgrade: clear all data and update version
        warn!(
            stored_version,
            CURRENT_SCHEMA_VERSION, "Schema version increased, clearing all data"
        );

        clear_all_data(db, logs_db).await?;
        set_schema_version(db, CURRENT_SCHEMA_VERSION).await?;

        info!(
            new_version = CURRENT_SCHEMA_VERSION,
            "Schema version updated and data cleared"
        );

        Ok(true)
    } else if stored_version > CURRENT_SCHEMA_VERSION {
        // Version downgrade: reject (code is too old for this database)
        Err(DbSqlError::Construction(format!(
            "Database schema version {} is newer than code version {}. Please upgrade the code to match the database \
             schema.",
            stored_version, CURRENT_SCHEMA_VERSION
        )))
    } else {
        // Same version: no action needed
        Ok(false)
    }
}

/// Get the stored schema version from the database.
///
/// # Errors
///
/// Returns an error if the database query fails or the schema_version table
/// doesn't exist (which shouldn't happen after migrations are run).
async fn get_stored_version(db: &DatabaseConnection) -> Result<i64> {
    let stmt = Statement::from_string(
        db.get_database_backend(),
        format!(
            "SELECT version FROM schema_version WHERE id = {}",
            SCHEMA_VERSION_TABLE_ID
        ),
    );

    let result = db
        .query_one_raw(stmt)
        .await?
        .ok_or_else(|| DbSqlError::Construction("schema_version table is empty".to_string()))?;

    let version: i64 = result.try_get("", "version")?;
    Ok(version)
}

/// Update the schema version in the database.
///
/// # Errors
///
/// Returns an error if the database update fails.
async fn set_schema_version(db: &DatabaseConnection, version: i64) -> Result<()> {
    let sql = format!(
        "UPDATE schema_version SET version = {}, updated_at = CURRENT_TIMESTAMP WHERE id = {}",
        version, SCHEMA_VERSION_TABLE_ID
    );

    db.execute_unprepared(&sql).await?;

    Ok(())
}

/// Clear all data from both index and logs databases.
///
/// This function deletes all data while preserving the schema structure.
/// For SQLite dual-database setup, it clears both databases.
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
/// This preserves foreign key relationships by deleting in the correct order.
///
/// # Errors
///
/// Returns an error if any database operations fail.
async fn clear_index_data(db: &DatabaseConnection) -> Result<()> {
    // Delete in order to respect foreign key constraints
    // Children first, then parents

    // Channel-related tables
    ChannelState::delete_many().exec(db).await?;
    Channel::delete_many().exec(db).await?;

    // Account-related tables
    AccountState::delete_many().exec(db).await?;
    Announcement::delete_many().exec(db).await?;
    Account::delete_many().exec(db).await?;

    // Balance tables
    HoprBalance::delete_many().exec(db).await?;
    NativeBalance::delete_many().exec(db).await?;

    // Safe contract table
    // Do not delete those rows marked with `MIGRATION_MARKER_BLOCK_ID` so that v3 Safe data
    // created during the migration are kept.
    HoprSafeContract::delete_many()
        .filter(SafeContractColumn::DeployedBlock.ne(MIGRATION_MARKER_BLOCK_ID))
        .exec(db)
        .await?;

    // Info tables
    ChainInfo::delete_many().exec(db).await?;

    Ok(())
}

/// Clear all data from the logs database.
///
/// # Errors
///
/// Returns an error if any database operations fail.
async fn clear_logs_data(db: &DatabaseConnection) -> Result<()> {
    // Delete in order to respect foreign key constraints

    // Log topics (child)
    LogTopicInfo::delete_many().exec(db).await?;

    // Log status (child)
    LogStatus::delete_many().exec(db).await?;

    // Logs (parent)
    Log::delete_many().exec(db).await?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use blokli_db_entity::{chain_info, log};
    use sea_orm::{ActiveModelTrait, EntityTrait, PaginatorTrait, Set};

    use super::*;
    use crate::{BlokliDbGeneralModelOperations, db::BlokliDb};

    #[test]
    fn test_version_constant_is_valid() {
        assert!(CURRENT_SCHEMA_VERSION > 0, "Schema version must be positive");
    }

    #[test]
    fn test_schema_version_table_id_is_valid() {
        assert_eq!(
            SCHEMA_VERSION_TABLE_ID, 1,
            "Schema version table should use singleton ID 1"
        );
    }

    /// Test that version upgrade clears all data
    #[tokio::test]
    async fn test_version_upgrade_clears_data() -> anyhow::Result<()> {
        // Create a temporary file for disk-based SQLite database
        let temp_dir = tempfile::tempdir()?;
        let db_path = temp_dir.path().join("test_upgrade.db");
        let db_url = format!("sqlite://{}?mode=rwc", db_path.display());

        // Create database and run migrations
        let db = BlokliDb::new(&db_url, None, Default::default()).await?;

        // Initialize singletons
        db.ensure_singletons().await?;

        // Insert some test data into chain_info
        let chain_info_update = chain_info::ActiveModel {
            id: Set(1),
            last_indexed_block: Set(100),
            last_indexed_tx_index: Set(Some(5)),
            last_indexed_log_index: Set(Some(10)),
            ..Default::default()
        };
        chain_info_update.update(db.conn(crate::TargetDb::Index)).await?;

        // Verify data exists
        let chain_info_before = ChainInfo::find_by_id(1)
            .one(db.conn(crate::TargetDb::Index))
            .await?
            .unwrap();
        assert_eq!(chain_info_before.last_indexed_block, 100);

        // Get the stored version (should be CURRENT_SCHEMA_VERSION after BlokliDb::new auto-upgrade)
        let version_before = get_stored_version(db.conn(crate::TargetDb::Index)).await?;
        assert_eq!(
            version_before, CURRENT_SCHEMA_VERSION,
            "Version should be current after BlokliDb::new"
        );

        // Manually update version to 0 to simulate old schema
        set_schema_version(db.conn(crate::TargetDb::Index), 0).await?;

        // Now call check_and_reset_if_needed - should reset data
        let was_reset = check_and_reset_if_needed(db.conn(crate::TargetDb::Index), None).await?;
        assert!(was_reset, "Data should have been reset");

        // Recreate singletons after reset (matches production flow)
        db.ensure_singletons().await?;

        // Verify version was updated
        let version_after = get_stored_version(db.conn(crate::TargetDb::Index)).await?;
        assert_eq!(version_after, CURRENT_SCHEMA_VERSION);

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

    /// Test that version downgrade fails with error
    #[tokio::test]
    async fn test_version_downgrade_fails() -> anyhow::Result<()> {
        // Create a temporary file for disk-based SQLite database
        let temp_dir = tempfile::tempdir()?;
        let db_path = temp_dir.path().join("test_downgrade.db");
        let db_url = format!("sqlite://{}?mode=rwc", db_path.display());

        // Create database and run migrations
        let db = BlokliDb::new(&db_url, None, Default::default()).await?;

        // Set version higher than current
        set_schema_version(db.conn(crate::TargetDb::Index), CURRENT_SCHEMA_VERSION + 1).await?;

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

    /// Test that same version preserves data
    #[tokio::test]
    async fn test_same_version_preserves_data() -> anyhow::Result<()> {
        // Create a temporary file for disk-based SQLite database
        let temp_dir = tempfile::tempdir()?;
        let db_path = temp_dir.path().join("test_same_version.db");
        let db_url = format!("sqlite://{}?mode=rwc", db_path.display());

        // Create database and run migrations
        let db = BlokliDb::new(&db_url, None, Default::default()).await?;

        // Initialize singletons
        db.ensure_singletons().await?;

        // Insert some test data
        let chain_info_update = chain_info::ActiveModel {
            id: Set(1),
            last_indexed_block: Set(42),
            last_indexed_tx_index: Set(Some(7)),
            last_indexed_log_index: Set(Some(3)),
            ..Default::default()
        };
        chain_info_update.update(db.conn(crate::TargetDb::Index)).await?;

        // Verify version matches current
        let version_before = get_stored_version(db.conn(crate::TargetDb::Index)).await?;
        assert_eq!(version_before, CURRENT_SCHEMA_VERSION);

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

    /// Test dual-database setup (SQLite with separate logs database)
    #[tokio::test]
    async fn test_dual_database_version_check() -> anyhow::Result<()> {
        // Create temporary files for both databases
        let temp_dir = tempfile::tempdir()?;
        let index_db_path = temp_dir.path().join("test_index.db");
        let logs_db_path = temp_dir.path().join("test_logs.db");
        let index_url = format!("sqlite://{}?mode=rwc", index_db_path.display());
        let logs_url = format!("sqlite://{}?mode=rwc", logs_db_path.display());

        // Create database with dual setup
        let db = BlokliDb::new(&index_url, Some(&logs_url), Default::default()).await?;

        // Initialize singletons
        db.ensure_singletons().await?;

        // Insert test data into both databases
        // Index database: chain_info
        let chain_info_update = chain_info::ActiveModel {
            id: Set(1),
            last_indexed_block: Set(999),
            ..Default::default()
        };
        chain_info_update.update(db.conn(crate::TargetDb::Index)).await?;

        // Logs database: insert a log entry
        let log_entry = log::ActiveModel {
            transaction_hash: Set(vec![0u8; 32]),
            tx_index: Set(0),
            log_index: Set(0),
            block_number: Set(100),
            block_hash: Set(vec![0u8; 32]),
            address: Set(vec![0u8; 20]),
            data: Set(vec![]),
            topics: Set(vec![]),
            ..Default::default()
        };
        log_entry.insert(db.conn(crate::TargetDb::Logs)).await?;

        // Manually set version to 0 to simulate upgrade
        set_schema_version(db.conn(crate::TargetDb::Index), 0).await?;

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

        // v3 Safe data should be kept
        assert_ne!(
            0,
            HoprSafeContract::find().count(db.conn(crate::TargetDb::Index)).await?
        );

        Ok(())
    }
}
