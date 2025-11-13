use std::{fs, io::Cursor, path::Path};

use async_compression::futures::bufread::XzEncoder;
use async_tar::Builder;
use futures_util::io::{AllowStdIo, AsyncReadExt, BufReader as FuturesBufReader};
use tempfile::TempDir;
use tracing::debug;

use crate::snapshot::{SnapshotInfo, SnapshotInstaller, SnapshotWorkflow, error::SnapshotResult};

/// Test-only snapshot manager without database dependencies.
///
/// Provides the same snapshot workflow as [`SnapshotManager`] but installs
/// files directly to the filesystem instead of integrating with a database.
/// Used in unit tests where database setup would add unnecessary complexity.
pub(crate) struct TestSnapshotManager {
    pub(crate) workflow: SnapshotWorkflow,
}

impl TestSnapshotManager {
    /// Creates a test snapshot manager without database dependencies.
    pub fn new() -> SnapshotResult<Self> {
        Ok(Self {
            workflow: SnapshotWorkflow::new()?,
        })
    }

    /// Downloads, extracts, validates, and installs a snapshot (test mode).
    ///
    /// Performs the same workflow as [`SnapshotManager::download_and_setup_snapshot`]
    /// but installs files directly to the filesystem instead of database integration.
    ///
    /// # Arguments
    ///
    /// * `url` - Snapshot URL (`https://`, `http://`, or `file://` scheme)
    /// * `data_dir` - Target directory for extracted files
    ///
    /// # Returns
    ///
    /// [`SnapshotInfo`] containing validation results
    pub async fn download_and_setup_snapshot(&self, url: &str, data_dir: &Path) -> SnapshotResult<SnapshotInfo> {
        self.workflow.execute_workflow(self, url, data_dir, false).await
    }
}

#[async_trait::async_trait]
impl SnapshotInstaller for TestSnapshotManager {
    async fn install_snapshot(
        &self,
        temp_dir: &Path,
        data_dir: &Path,
        extracted_files: &[String],
    ) -> SnapshotResult<()> {
        // Install files directly to data directory (test mode)
        fs::create_dir_all(data_dir)?;

        for file in extracted_files {
            let src = temp_dir.join(file);
            let dst = data_dir.join(file);

            // Remove existing file if it exists
            if dst.exists() {
                fs::remove_file(&dst)?;
            }

            // Copy file to final location
            fs::copy(&src, &dst)?;
            debug!(from = %file, to = %dst.display(), "Installed snapshot file");
        }

        Ok(())
    }
}

/// Creates a test SQL dump file for testing
pub fn create_test_sql_dump(path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let sql_content = r#"
--
-- PostgreSQL database dump
--

SET statement_timeout = 0;
SET lock_timeout = 0;
SET idle_in_transaction_session_timeout = 0;
SET client_encoding = 'UTF8';

CREATE TABLE log (
    transaction_index BIGINT NOT NULL,
    log_index BIGINT NOT NULL,
    block_number BIGINT NOT NULL,
    block_hash BYTEA NOT NULL,
    transaction_hash BYTEA NOT NULL,
    address BYTEA NOT NULL,
    topics BYTEA NOT NULL,
    data BYTEA NOT NULL,
    removed BOOLEAN NOT NULL
);

CREATE TABLE log_status (
    id SERIAL PRIMARY KEY,
    status TEXT NOT NULL
);

CREATE TABLE log_topic_info (
    id SERIAL PRIMARY KEY,
    topic_hash TEXT NOT NULL
);

--
-- Data for Name: log; Type: TABLE DATA; Schema: public; Owner: -
--

COPY log (transaction_index, log_index, block_number, block_hash, transaction_hash, address, topics, data, removed) FROM stdin;
1	1	1	\\x0000000000000000000000000000000000000000000000000000000000000000	\\x0000000000000000000000000000000000000000000000000000000000000000	\\x0000000000000000000000000000000000000000	\\x00	\\x00	f
2	2	2	\\x0000000000000000000000000000000000000000000000000000000000000000	\\x0000000000000000000000000000000000000000000000000000000000000000	\\x0000000000000000000000000000000000000000	\\x00	\\x00	f
\.

--
-- Data for Name: log_status; Type: TABLE DATA; Schema: public; Owner: -
--

COPY log_status (id, status) FROM stdin;
1	active
\.

--
-- Data for Name: log_topic_info; Type: TABLE DATA; Schema: public; Owner: -
--

COPY log_topic_info (id, topic_hash) FROM stdin;
1	0x123
\.
"#;

    fs::write(path, sql_content)?;
    Ok(())
}

/// Creates a test tar.xz archive containing a PostgreSQL SQL dump
pub(crate) async fn create_test_archive(
    temp_dir: &TempDir,
    sql_target_path: Option<String>,
) -> Result<std::path::PathBuf, Box<dyn std::error::Error>> {
    // Create the SQL dump
    let sql_target_path_final = sql_target_path.unwrap_or_else(|| "hopr_logs.sql".to_string());
    let sql_path = temp_dir.path().join("hopr_logs.sql");
    create_test_sql_dump(&sql_path)?;

    // First create uncompressed tar in memory
    let mut tar_data = Vec::new();
    {
        let mut tar = Builder::new(&mut tar_data);
        tar.append_path_with_name(&sql_path, sql_target_path_final).await?;
        tar.into_inner().await?;
    }

    // Now compress with xz using async_compression
    let cursor = Cursor::new(tar_data);
    let buf_reader = FuturesBufReader::new(AllowStdIo::new(cursor));
    let mut encoder = XzEncoder::new(buf_reader);

    // Read compressed data
    let mut compressed_data = Vec::new();
    encoder.read_to_end(&mut compressed_data).await?;

    // Write to final archive file
    let archive_path = temp_dir.path().join("test_snapshot.tar.xz");
    fs::write(&archive_path, compressed_data)?;

    // Clean up the temporary SQL file to avoid test interference
    fs::remove_file(&sql_path)?;

    Ok(archive_path)
}
