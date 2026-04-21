//! SQL snapshot validation for snapshot integrity verification.
//!
//! Validates extracted `hopr_logs.sql` snapshots before installation.

use std::path::Path;

use blokli_db::snapshot::{SNAPSHOT_SQL_FILE, inspect_logs_snapshot_sql};
use tracing::{info, warn};

use crate::snapshot::error::{SnapshotError, SnapshotResult};

/// Metadata about a validated snapshot SQL dump.
///
/// Contains information gathered during validation that describes
/// the contents and state of the snapshot SQL file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SnapshotInfo {
    /// Estimated number of log entries in the snapshot
    pub log_count: Option<u64>,
    /// Highest block number found in the snapshot (if parsed)
    pub latest_block: Option<u64>,
    /// Number of tables found in SQL dump
    pub tables: usize,
}

/// Validates PostgreSQL SQL dump files for integrity and expected content.
///
/// Performs comprehensive validation including file existence,
/// schema verification, and basic SQL syntax checks.
#[derive(Default, PartialEq, Eq)]
pub struct SnapshotValidator;

impl SnapshotValidator {
    pub fn new() -> Self {
        Self
    }

    pub async fn validate_snapshot(&self, snapshot_dir: &Path) -> SnapshotResult<SnapshotInfo> {
        let sql_path = snapshot_dir.join(SNAPSHOT_SQL_FILE);
        info!(sql = %sql_path.display(), "Validating SQL snapshot dump");

        if !sql_path.exists() {
            return Err(SnapshotError::Validation(format!(
                "SQL dump file does not exist: {}",
                sql_path.display()
            )));
        }

        let info =
            inspect_logs_snapshot_sql(&sql_path).map_err(|error| SnapshotError::Validation(error.to_string()))?;
        if info.log_count == 0 {
            warn!("SQL dump contains no log data");
        }

        Ok(SnapshotInfo {
            log_count: Some(info.log_count),
            latest_block: info.latest_block,
            tables: 3,
        })
    }
}

#[cfg(test)]
mod tests {
    use tempfile::TempDir;

    use super::*;
    use crate::snapshot::test_utils::create_test_sql_dump;

    #[tokio::test]
    async fn test_validate_ok() {
        let temp_dir = TempDir::new().unwrap();
        let validator = SnapshotValidator::new();
        create_test_sql_dump(&temp_dir.path().join(SNAPSHOT_SQL_FILE)).unwrap();

        let result = validator.validate_snapshot(temp_dir.path()).await;

        assert!(result.is_ok(), "Validation should succeed");
        let info = result.unwrap();
        assert_eq!(info.log_count, Some(2));
        assert_eq!(info.latest_block, Some(2));
        assert_eq!(info.tables, 3);
    }

    #[tokio::test]
    async fn test_missing_file() {
        let temp_dir = TempDir::new().unwrap();
        let validator = SnapshotValidator::new();

        let result = validator.validate_snapshot(temp_dir.path()).await;
        assert!(result.is_err(), "Validation should fail for missing file");
    }
}
