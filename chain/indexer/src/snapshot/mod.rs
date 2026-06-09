//! Fast synchronization using database snapshots.
//!
//! This module enables HOPR nodes to synchronize quickly with the network by downloading
//! and installing pre-built database snapshots instead of processing all historical blockchain logs.
//!
//! # Features
//!
//! - **HTTP/HTTPS Downloads**: Secure download with retry logic and progress tracking
//! - **Local File Support**: Direct installation from local `file://` URLs
//! - **Archive Extraction**: Safe tar.xz extraction with path traversal protection
//! - **SQL Validation**: PostgreSQL-style `COPY ... FROM stdin` dump verification
//! - **Disk Space Management**: Cross-platform space validation before operations
//! - **Comprehensive Errors**: Actionable error messages with recovery suggestions
//!
//! # URL Support
//!
//! - `https://example.com/snapshot.tar.xz` - Remote HTTP/HTTPS downloads
//! - `file:///path/to/snapshot.tar.xz` - Local file system access
//!
//! # Example
//!
//! ```no_run
//! use std::path::Path;
//! use blokli_chain_indexer::snapshot::{SnapshotResult, SnapshotManager};
//!
//! # async fn example(db: impl blokli_db::BlokliDbGeneralModelOperations + Clone + Send + Sync + 'static) -> SnapshotResult<()> {
//! let manager = SnapshotManager::with_db(db)?;
//! let info = manager
//!     .download_and_setup_snapshot(
//!         "https://snapshots.hoprnet.org/logs.tar.xz",
//!         Path::new("/data/hopr")
//!     )
//!     .await?;
//!
//! println!("Installed snapshot: {} logs, latest block {}", info.log_count.unwrap_or(0), info.latest_block.unwrap_or(0));
//! # Ok(())
//! # }
//! ```

pub mod download;
pub mod error;
pub mod extract;
pub mod validate;

#[cfg(test)]
pub(crate) mod test_utils;

// Re-export commonly used types
use std::{fs, io::Cursor, path::Path};

use async_compression::futures::bufread::XzEncoder;
use async_tar::Builder;
use blokli_db::{BlokliDbGeneralModelOperations, snapshot::SNAPSHOT_SQL_FILE};
pub use error::{SnapshotError, SnapshotResult};
use futures_util::io::{AllowStdIo, AsyncReadExt, BufReader as FuturesBufReader};
use tracing::{debug, error, info};
pub use validate::SnapshotInfo;

use crate::{
    snapshot::{download::SnapshotDownloader, extract::SnapshotExtractor, validate::SnapshotValidator},
    utils::redact_url,
};

/// Trait for implementing the snapshot installation step.
///
/// This trait abstracts the final installation step of the snapshot workflow,
/// allowing different implementations for production (database integration)
/// and testing (filesystem copy) scenarios.
#[async_trait::async_trait]
trait SnapshotInstaller {
    /// Installs the validated snapshot from the temporary directory.
    ///
    /// # Arguments
    /// * `temp_dir` - Directory containing extracted and validated snapshot files
    /// * `data_dir` - Target directory for installation
    /// * `extracted_files` - List of files that were extracted from the archive
    ///
    /// # Returns
    /// Result indicating success or failure of installation
    async fn install_snapshot(
        &self,
        temp_dir: &Path,
        data_dir: &Path,
        extracted_files: &[String],
    ) -> SnapshotResult<()>;
}

/// Shared snapshot workflow implementation.
///
/// Contains the common download → extract → validate → install workflow
/// shared between SnapshotManager and TestSnapshotManager.
pub(crate) struct SnapshotWorkflow {
    downloader: SnapshotDownloader,
    extractor: SnapshotExtractor,
    validator: SnapshotValidator,
}

impl SnapshotWorkflow {
    /// Creates a new snapshot workflow with default components.
    fn new() -> Result<Self, SnapshotError> {
        Ok(Self {
            downloader: SnapshotDownloader::new()?,
            extractor: SnapshotExtractor::new(),
            validator: SnapshotValidator::new(),
        })
    }

    /// Executes the complete snapshot workflow.
    ///
    /// Downloads, extracts, validates, and installs a snapshot using the provided installer.
    async fn execute_workflow<I: SnapshotInstaller>(
        &self,
        installer: &I,
        url: &str,
        data_dir: &Path,
        use_temp_subdir: bool,
    ) -> SnapshotResult<SnapshotInfo> {
        info!("Starting snapshot download and setup from: {}", redact_url(url));

        // Create temporary directory - either as subdirectory or using tempfile
        let (temp_dir, _temp_guard) = if use_temp_subdir {
            let temp_dir = data_dir.join("snapshot_temp");
            fs::create_dir_all(&temp_dir)?;
            (temp_dir, None)
        } else {
            let temp_guard = tempfile::tempdir_in(data_dir)?;
            let temp_dir = temp_guard.path().to_path_buf();
            (temp_dir, Some(temp_guard))
        };

        // Download snapshot
        let archive_path = temp_dir.join("snapshot.tar.xz");
        self.downloader.download_snapshot(url, &archive_path).await?;

        // Extract snapshot
        let extracted_files = self.extractor.extract_snapshot(&archive_path, &temp_dir).await?;
        debug!("Extracted snapshot files: {:?}", extracted_files);

        let snapshot_info = self.validator.validate_snapshot(&temp_dir).await?;

        // Install using the provided installer
        installer
            .install_snapshot(&temp_dir, data_dir, &extracted_files)
            .await?;

        // Cleanup temporary directory if we created it manually
        if use_temp_subdir {
            if let Err(e) = fs::remove_dir_all(&temp_dir) {
                error!("Failed to cleanup temp directory: {}", e);
            }
        }
        // tempfile cleanup is automatic via Drop

        info!("Snapshot setup completed successfully");
        Ok(snapshot_info)
    }
}

/// Coordinates snapshot download, extraction, validation, and database integration.
///
/// The main interface for snapshot operations in production environments.
/// Manages the complete workflow from download to database installation.
///
/// # Architecture
///
/// - [`SnapshotDownloader`] - HTTP/HTTPS and file:// URL handling with retry logic
/// - [`SnapshotExtractor`] - Secure tar.xz extraction with path validation
/// - [`SnapshotValidator`] - SQL dump integrity verification
/// - Database integration via [`BlokliDbGeneralModelOperations::import_logs_snapshot`]
pub struct SnapshotManager<Db>
where
    Db: BlokliDbGeneralModelOperations + Clone + Send + Sync + 'static,
{
    db: Db,
    workflow: SnapshotWorkflow,
}

impl<Db> SnapshotManager<Db>
where
    Db: BlokliDbGeneralModelOperations + Clone + Send + Sync + 'static,
{
    /// Creates a snapshot manager with database integration.
    ///
    /// # Arguments
    ///
    /// * `db` - Database instance implementing [`BlokliDbGeneralModelOperations`]
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use blokli_chain_indexer::snapshot::{SnapshotResult, SnapshotManager};
    ///
    /// # fn example(db: impl blokli_db::BlokliDbGeneralModelOperations + Clone + Send + Sync + 'static) -> SnapshotResult<()> {
    /// let manager = SnapshotManager::with_db(db)?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn with_db(db: Db) -> Result<Self, SnapshotError> {
        Ok(Self {
            db,
            workflow: SnapshotWorkflow::new()?,
        })
    }

    /// Downloads, extracts, validates, and installs a snapshot.
    ///
    /// Performs the complete snapshot setup workflow:
    /// 1. Downloads archive from URL (HTTP/HTTPS/file://)
    /// 2. Extracts tar.xz archive safely
    /// 3. Validates SQL dump integrity
    /// 4. Installs via [`BlokliDbGeneralModelOperations::import_logs_snapshot`]
    /// 5. Cleans up temporary files
    ///
    /// # Arguments
    ///
    /// * `url` - Snapshot URL (`https://`, `http://`, or `file://` scheme)
    /// * `data_dir` - Target directory for temporary files during installation
    ///
    /// # Returns
    ///
    /// [`SnapshotInfo`] containing log count, block count, and metadata on success
    ///
    /// # Errors
    ///
    /// Returns [`SnapshotError`] for network failures, validation errors, or installation issues
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # use std::path::Path;
    /// # use blokli_chain_indexer::snapshot::SnapshotManager;
    /// # async fn example(manager: SnapshotManager<impl blokli_db::BlokliDbGeneralModelOperations + Clone + Send + Sync + 'static>) -> Result<(), Box<dyn std::error::Error>> {
    /// // Download from HTTPS
    /// let info = manager
    ///     .download_and_setup_snapshot("https://snapshots.hoprnet.org/logs.tar.xz", Path::new("/data"))
    ///     .await?;
    ///
    /// // Use local file
    /// let info = manager
    ///     .download_and_setup_snapshot("file:///backups/snapshot.tar.xz", Path::new("/data"))
    ///     .await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn download_and_setup_snapshot(&self, url: &str, data_dir: &Path) -> SnapshotResult<SnapshotInfo> {
        self.workflow.execute_workflow(self, url, data_dir, true).await
    }

    /// Exports the current logs database as a `tar.xz` snapshot archive.
    ///
    /// The archive written to `output_path` contains a single `hopr_logs.sql`
    /// file that can later be restored through
    /// [`SnapshotManager::download_and_setup_snapshot`].
    ///
    /// # Arguments
    ///
    /// * `output_path` - Destination archive path. Bare filenames are written relative to the current working
    ///   directory.
    ///
    /// # Returns
    ///
    /// [`SnapshotInfo`] describing the exported snapshot contents.
    ///
    /// # Errors
    ///
    /// Returns [`SnapshotError`] if the output directory cannot be created, the
    /// temporary SQL dump cannot be produced, or archive creation fails.
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use std::path::Path;
    /// # use blokli_chain_indexer::snapshot::{SnapshotManager, SnapshotResult};
    /// # async fn example(db: impl blokli_db::BlokliDbGeneralModelOperations + Clone + Send + Sync + 'static) -> SnapshotResult<()> {
    /// let manager = SnapshotManager::with_db(db)?;
    /// let info = manager
    ///     .export_snapshot(Path::new("logs-snapshot.tar.xz"))
    ///     .await?;
    ///
    /// println!("exported {} logs", info.log_count.unwrap_or(0));
    /// # Ok(())
    /// # }
    /// ```
    pub async fn export_snapshot(&self, output_path: &Path) -> SnapshotResult<SnapshotInfo> {
        let parent_dir = match output_path.parent() {
            Some(parent) if !parent.as_os_str().is_empty() => parent,
            _ => Path::new("."),
        };
        fs::create_dir_all(parent_dir)?;

        let temp_guard = tempfile::tempdir_in(parent_dir)?;
        let temp_dir = temp_guard.path().to_path_buf();
        let info = self
            .db
            .export_logs_snapshot(temp_dir.clone())
            .await
            .map_err(|error| SnapshotError::Installation(error.to_string()))?;

        archive_snapshot_dir(&temp_dir, output_path).await?;

        Ok(SnapshotInfo {
            log_count: Some(info.log_count),
            latest_block: info.latest_block,
            tables: 3,
        })
    }
}

#[async_trait::async_trait]
impl<Db> SnapshotInstaller for SnapshotManager<Db>
where
    Db: BlokliDbGeneralModelOperations + Clone + Send + Sync + 'static,
{
    async fn install_snapshot(
        &self,
        temp_dir: &Path,
        _data_dir: &Path,
        _extracted_files: &[String],
    ) -> SnapshotResult<()> {
        // Update database using the imported logs database
        self.db
            .clone()
            .import_logs_snapshot(temp_dir.to_path_buf())
            .await
            .map_err(|e| SnapshotError::Installation(e.to_string()))?;

        Ok(())
    }
}

async fn archive_snapshot_dir(source_dir: &Path, output_path: &Path) -> SnapshotResult<()> {
    let sql_path = source_dir.join(SNAPSHOT_SQL_FILE);
    if !sql_path.is_file() {
        return Err(SnapshotError::Configuration(format!(
            "snapshot export did not produce {} in {}",
            SNAPSHOT_SQL_FILE,
            source_dir.display()
        )));
    }

    let mut tar_data = Vec::new();
    {
        let mut builder = Builder::new(&mut tar_data);
        builder.append_path_with_name(&sql_path, SNAPSHOT_SQL_FILE).await?;

        builder.into_inner().await?;
    }

    let cursor = Cursor::new(tar_data);
    let reader = FuturesBufReader::new(AllowStdIo::new(cursor));
    let mut encoder = XzEncoder::new(reader);
    let mut compressed = Vec::new();
    encoder.read_to_end(&mut compressed).await?;
    fs::write(output_path, compressed)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use blokli_db::{api::logs::BlokliDbLogOperations, db::BlokliDb, snapshot::SNAPSHOT_SQL_FILE};
    use hopr_types::{
        crypto::types::Hash,
        primitive::prelude::{DateTime, SerializableLog},
    };
    use tempfile::TempDir;

    use super::{test_utils::*, *};
    use crate::snapshot::SnapshotInfo;

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
    async fn test_snapshot_manager_integration() {
        let temp_dir = TempDir::new().unwrap();

        // Create test archive
        let archive_path = create_test_archive(&temp_dir, None).await.unwrap();

        // Use TestSnapshotManager for testing
        let manager = TestSnapshotManager::new().expect("Failed to create TestSnapshotManager");
        let data_dir = temp_dir.path().join("data");
        fs::create_dir_all(&data_dir).unwrap();

        // Test file:// URL using TestSnapshotManager
        let file_url = format!("file://{}", archive_path.display());
        let result = manager.download_and_setup_snapshot(&file_url, &data_dir).await;

        assert!(result.is_ok(), "TestSnapshotManager should handle file:// URLs");
        let info = result.unwrap();
        assert_eq!(info.log_count, Some(2));

        assert!(data_dir.join(SNAPSHOT_SQL_FILE).exists());
    }

    #[tokio::test]
    async fn test_snapshot_manager_with_data_directory() {
        let temp_dir = TempDir::new().unwrap();
        let data_dir = temp_dir.path().join("hopr_data");
        fs::create_dir_all(&data_dir).unwrap();

        // Create a test archive
        let archive_path = create_test_archive(&temp_dir, None).await.unwrap();

        // Test file:// URL support using TestSnapshotManager
        let manager = TestSnapshotManager::new().expect("Failed to create TestSnapshotManager");

        // Test with file:// URL for local testing
        let file_url = format!("file://{}", archive_path.display());

        // Test the full workflow through TestSnapshotManager
        let result = manager.download_and_setup_snapshot(&file_url, &data_dir).await;
        assert!(result.is_ok(), "TestSnapshotManager should handle complete workflow");

        let info = result.unwrap();
        assert_eq!(info.log_count, Some(2));

        // Verify the SQL dump file exists in the data directory
        assert!(data_dir.join(SNAPSHOT_SQL_FILE).exists());

        // Also test individual component access
        let downloader = manager.workflow.downloader;
        let downloaded_archive = data_dir.join("test_download.tar.xz");
        let download_result = downloader.download_snapshot(&file_url, &downloaded_archive).await;
        assert!(download_result.is_ok(), "file:// URL download should succeed");
        assert!(downloaded_archive.exists(), "Downloaded archive should exist");
    }

    #[tokio::test]
    async fn test_snapshot_manager_export_then_import_round_trip() {
        let source_db = BlokliDb::new_in_memory().await.expect("source db");
        source_db
            .ensure_logs_origin(vec![(sample_log(10, 1, 0).address, Hash::create(&[b"topic"]))])
            .await
            .expect("logs origin");
        source_db.store_log(sample_log(10, 1, 0)).await.expect("first log");
        source_db.store_log(sample_log(11, 1, 1)).await.expect("second log");
        source_db.update_logs_checksums().await.expect("checksums");

        let temp_dir = TempDir::new().expect("tempdir");
        let archive_path = temp_dir.path().join("logs-snapshot.tar.xz");

        let exporter = SnapshotManager::with_db(source_db.clone()).expect("snapshot manager");
        let exported = exporter.export_snapshot(&archive_path).await.expect("export snapshot");

        assert!(archive_path.exists(), "snapshot archive should exist");

        let restored_db = BlokliDb::new_in_memory().await.expect("restored db");
        let importer = SnapshotManager::with_db(restored_db.clone()).expect("snapshot manager");
        let restored = importer
            .download_and_setup_snapshot(&format!("file://{}", archive_path.display()), temp_dir.path())
            .await
            .expect("import snapshot");

        assert_eq!(
            exported,
            SnapshotInfo {
                log_count: Some(2),
                latest_block: Some(11),
                tables: 3,
            }
        );
        assert_eq!(exported, restored);
        assert_eq!(restored_db.get_logs_count(None, None).await.expect("logs count"), 2);
        assert_eq!(
            restored_db
                .get_logs_block_numbers(None, None, Some(true))
                .await
                .expect("processed block numbers"),
            vec![10, 11]
        );

        let restored_logs = restored_db.get_logs(None, None).await.expect("restored logs");
        assert_eq!(restored_logs.len(), 2);
        for log in restored_logs {
            assert_eq!(log.processed, Some(true), "restored log should keep processed state");
            assert!(
                log.processed_at.is_some(),
                "restored log should keep processed timestamp"
            );
            assert!(log.checksum.is_some(), "restored log should keep checksum");
        }
    }
}
