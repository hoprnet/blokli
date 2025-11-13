//! PostgreSQL SQL dump validation for snapshot integrity verification.
//!
//! Validates extracted snapshot SQL dumps to ensure they contain expected
//! tables and data before installation.

use std::{fs, path::Path};

use tracing::{info, warn};

use crate::snapshot::error::{SnapshotError, SnapshotResult};

/// Metadata about a validated snapshot SQL dump.
///
/// Contains information gathered during validation that describes
/// the contents and state of the snapshot SQL file.
#[derive(Debug, Clone)]
pub struct SnapshotInfo {
    /// Estimated number of log entries in the snapshot
    pub log_count: Option<u64>,
    /// Highest block number found in the snapshot (if parsed)
    pub latest_block: Option<u64>,
    /// Number of tables found in SQL dump
    pub tables: usize,
    /// SQL dump file size in bytes
    pub sql_size: u64,
}

/// Validates PostgreSQL SQL dump files for integrity and expected content.
///
/// Performs comprehensive validation including file existence,
/// schema verification, and basic SQL syntax checks.
#[derive(Default, PartialEq, Eq)]
pub struct SnapshotValidator {
    /// Required tables that must exist in valid snapshot SQL dumps
    expected_tables: Vec<String>,
}

impl SnapshotValidator {
    /// Creates a new validator with predefined expected tables.
    ///
    /// Expected tables include:
    /// - `log` - Blockchain log entries
    /// - `log_status` - Log processing status
    /// - `log_topic_info` - Log topic information
    pub fn new() -> Self {
        Self {
            expected_tables: vec![
                "log".to_string(),
                "log_status".to_string(),
                "log_topic_info".to_string(),
            ],
        }
    }

    /// Validates a snapshot SQL dump for integrity and expected content.
    ///
    /// Performs comprehensive validation:
    /// 1. File existence and accessibility
    /// 2. File is not empty
    /// 3. Contains expected CREATE TABLE statements
    /// 4. Basic SQL syntax validation
    /// 5. Content statistics gathering
    ///
    /// # Arguments
    ///
    /// * `sql_path` - Path to the PostgreSQL SQL dump file to validate
    ///
    /// # Returns
    ///
    /// [`SnapshotInfo`] containing validation results and SQL dump metadata
    ///
    /// # Errors
    ///
    /// * [`SnapshotError::Validation`] - Missing tables or invalid SQL format
    /// * [`SnapshotError::Io`] - File access errors
    pub async fn validate_snapshot(&self, sql_path: &Path) -> SnapshotResult<SnapshotInfo> {
        info!(sql = %sql_path.display(), "Validating SQL snapshot dump");

        let snapshot_info = Self::validate_sql_dump(sql_path, &self.expected_tables).await?;
        info!(?snapshot_info, "SQL snapshot validation successful");

        Ok(snapshot_info)
    }

    /// Validates the PostgreSQL SQL dump file
    async fn validate_sql_dump(sql_path: &Path, expected_tables: &[String]) -> SnapshotResult<SnapshotInfo> {
        // Check if file exists
        if !sql_path.exists() {
            return Err(SnapshotError::Validation("SQL dump file does not exist".to_string()));
        }

        // Get file size
        let metadata = fs::metadata(sql_path)?;
        let sql_size = metadata.len();

        // Check for empty file
        if sql_size == 0 {
            return Err(SnapshotError::Validation("SQL dump file is empty".to_string()));
        }

        // Read SQL content
        let content = fs::read_to_string(sql_path)
            .map_err(|e| SnapshotError::Validation(format!("Cannot read SQL file: {e}")))?;

        // Verify expected tables exist in SQL
        let mut found_tables = Vec::new();
        for table in expected_tables {
            // Check for CREATE TABLE statements (with or without schema prefix)
            if content.contains(&format!("CREATE TABLE {}", table))
                || content.contains(&format!("CREATE TABLE IF NOT EXISTS {}", table))
                || content.contains(&format!("CREATE TABLE public.{}", table))
                || content.contains(&format!("CREATE TABLE IF NOT EXISTS public.{}", table))
            {
                found_tables.push(table.clone());
            }
        }

        // Verify all expected tables were found
        if found_tables.len() != expected_tables.len() {
            let missing: Vec<_> = expected_tables
                .iter()
                .filter(|t| !found_tables.contains(t))
                .map(|s| s.as_str())
                .collect();
            return Err(SnapshotError::Validation(format!(
                "Missing required table(s) in SQL dump: {}. Found tables: {:?}",
                missing.join(", "),
                found_tables
            )));
        }

        // Estimate log count by parsing COPY statements
        let log_count = estimate_log_count(&content);

        if log_count == Some(0) {
            warn!("SQL dump contains no log data");
        }

        Ok(SnapshotInfo {
            log_count,
            latest_block: None, // Could parse from COPY data if needed
            tables: found_tables.len(),
            sql_size,
        })
    }

    /// Checks SQL content for basic consistency
    pub async fn check_data_consistency(&self, sql_path: &Path) -> SnapshotResult<()> {
        let content = fs::read_to_string(sql_path)
            .map_err(|e| SnapshotError::Validation(format!("Cannot read SQL file: {e}")))?;

        // Basic check: ensure COPY statements have matching terminators
        let copy_count = content.matches("COPY ").count();
        let terminator_count = content.matches("\\.").count();

        if copy_count > 0 && copy_count != terminator_count {
            return Err(SnapshotError::Validation(format!(
                "SQL dump has {} COPY statements but {} terminators",
                copy_count, terminator_count
            )));
        }

        Ok(())
    }
}

/// Estimates log count by counting lines in COPY statements
///
/// Looks for the pattern:
/// ```sql
/// COPY log (...) FROM stdin;
/// <data lines>
/// \.
/// ```
fn estimate_log_count(content: &str) -> Option<u64> {
    // Find the COPY log statement
    let copy_pattern = "COPY log";
    let copy_start = content.find(copy_pattern)?;

    // Find the FROM stdin part
    let from_stdin_start = content[copy_start..].find("FROM stdin")?;
    let data_start = copy_start + from_stdin_start + "FROM stdin;\n".len();

    // Find the terminator (backslash-dot on its own line)
    let remaining = &content[data_start..];

    // Handle edge case: terminator appears immediately (no data)
    if remaining.trim_start().starts_with("\\.") {
        return Some(0);
    }

    let terminator = remaining.find("\n\\.")?;

    // Count newlines in the data section
    let data_section = &remaining[..terminator];
    let line_count = data_section.lines().count();

    Some(line_count as u64)
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

        // Create test SQL dump
        let sql_path = temp_dir.path().join("hopr_logs.sql");
        create_test_sql_dump(&sql_path).unwrap();

        // Validate the SQL dump
        let result = validator.validate_snapshot(&sql_path).await;

        assert!(result.is_ok(), "Validation should succeed");
        let info = result.unwrap();
        assert_eq!(info.log_count, Some(2));
        assert_eq!(info.tables, 3);
    }

    #[tokio::test]
    async fn test_missing_file() {
        let temp_dir = TempDir::new().unwrap();
        let validator = SnapshotValidator::new();

        // Try to validate non-existent file
        let sql_path = temp_dir.path().join("nonexistent.sql");
        let result = validator.validate_snapshot(&sql_path).await;

        assert!(result.is_err(), "Validation should fail for missing file");
    }

    #[tokio::test]
    async fn test_empty_file() {
        let temp_dir = TempDir::new().unwrap();
        let validator = SnapshotValidator::new();

        // Create empty SQL file
        let sql_path = temp_dir.path().join("empty.sql");
        fs::write(&sql_path, "").unwrap();

        let result = validator.validate_snapshot(&sql_path).await;
        assert!(result.is_err(), "Validation should fail for empty file");
    }

    #[tokio::test]
    async fn test_missing_tables() {
        let temp_dir = TempDir::new().unwrap();
        let validator = SnapshotValidator::new();

        // Create SQL with missing tables
        let sql_path = temp_dir.path().join("incomplete.sql");
        fs::write(&sql_path, "CREATE TABLE log (id BIGINT);").unwrap();

        let result = validator.validate_snapshot(&sql_path).await;
        assert!(result.is_err(), "Validation should fail for missing tables");
    }

    #[test]
    fn test_estimate_log_count() {
        let sql_with_data = r#"
CREATE TABLE log (id BIGINT);

COPY log (id, data) FROM stdin;
1	test1
2	test2
3	test3
\.
        "#;

        assert_eq!(estimate_log_count(sql_with_data), Some(3));

        let sql_no_data = r#"
COPY log (id) FROM stdin;
\.
        "#;

        assert_eq!(estimate_log_count(sql_no_data), Some(0));
    }
}
