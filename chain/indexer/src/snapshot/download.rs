//! Secure snapshot downloading with HTTP/HTTPS and local file support.
//!
//! Provides robust download capabilities for snapshot archives with:
//! - **URL Support**: HTTP/HTTPS with retry logic, local file:// URLs
//! - **Safety**: Size limits, timeout protection, disk space validation
//! - **Reliability**: Exponential backoff, progress tracking, error recovery
//! - **Integrity**: Optional SHA-256 digest check of the archive before it is used
//! - **Cross-platform**: Uses sysinfo for disk space checking

use std::{
    fs,
    fs::File,
    io::{Error, ErrorKind, Read},
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};

use async_lock::Mutex;
use backon::{FuturesTimerSleeper, Retryable};
use futures_util::{AsyncWriteExt, TryStreamExt, io::AllowStdIo};
use reqwest::Client;
use sha2::{Digest, Sha256};
use smart_default::SmartDefault;
use sysinfo::Disks;
use tracing::{debug, error, info, warn};

use crate::{
    constants::{
        LOGS_SNAPSHOT_DOWNLOADER_MAX_RETRIES, LOGS_SNAPSHOT_DOWNLOADER_MAX_SIZE, LOGS_SNAPSHOT_DOWNLOADER_TIMEOUT,
    },
    snapshot::error::{SnapshotError, SnapshotResult},
    utils::redact_url,
};

/// Configuration for snapshot downloads with safety limits.
///
/// Controls download behavior including size limits, timeouts, and retry attempts
/// to ensure safe and reliable snapshot downloads.
#[derive(Debug, Clone, SmartDefault)]
pub struct DownloadConfig {
    /// Maximum allowed file size in bytes
    #[default(_code = "LOGS_SNAPSHOT_DOWNLOADER_MAX_SIZE")]
    pub max_size: u64,
    /// HTTP request timeout duration
    #[default(_code = "LOGS_SNAPSHOT_DOWNLOADER_TIMEOUT")]
    pub timeout: Duration,
    /// Maximum number of retry attempts for failed downloads
    #[default(_code = "LOGS_SNAPSHOT_DOWNLOADER_MAX_RETRIES")]
    pub max_retries: u32,
}

/// Downloads snapshot archives from HTTP/HTTPS and file:// URLs.
///
/// Provides secure, reliable downloading with automatic retry logic for network sources
/// and direct file copying for local sources.
///
/// # Features
///
/// - **HTTP/HTTPS**: Automatic retry with exponential backoff, progress tracking
/// - **Local Files**: Direct copying from file:// URLs with validation
/// - **Safety**: Size limits, disk space checks, timeout protection
/// - **Monitoring**: Progress reporting and detailed error messages
///
/// # Examples
///
/// ```no_run
/// use std::path::Path;
///
/// use blokli_chain_indexer::snapshot::download::SnapshotDownloader;
///
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// let downloader = SnapshotDownloader::new()?;
///
/// // Download from HTTPS
/// downloader
///     .download_snapshot(
///         "https://snapshots.hoprnet.org/logs.tar.xz",
///         Path::new("/tmp/snapshot.tar.xz"),
///         None,
///     )
///     .await?;
///
/// // Copy from local file and verify its SHA-256 digest
/// downloader
///     .download_snapshot(
///         "file:///backups/snapshot.tar.xz",
///         Path::new("/tmp/snapshot.tar.xz"),
///         Some("e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"),
///     )
///     .await?;
/// # Ok(())
/// # }
/// ```
pub struct SnapshotDownloader {
    client: Client,
    config: DownloadConfig,
}

impl SnapshotDownloader {
    /// Creates a new snapshot downloader with default configuration
    pub fn new() -> SnapshotResult<Self> {
        Self::with_config(DownloadConfig::default())
    }

    /// Creates a new snapshot downloader with custom configuration
    pub fn with_config(config: DownloadConfig) -> SnapshotResult<Self> {
        Ok(Self {
            client: Client::builder()
                .timeout(config.timeout)
                .user_agent("curl/8.14.1") // acts like curl for compatibility
                .build()
                .map_err(SnapshotError::Network)?,
            config,
        })
    }

    /// Downloads a snapshot from the given URL to the target path.
    ///
    /// Supports HTTP/HTTPS URLs with retry logic and file:// URLs for local files.
    ///
    /// # Arguments
    ///
    /// * `url` - Source URL (http://, https://, or file:// scheme)
    /// * `target_path` - Destination file path
    /// * `expected_sha256` - Optional hex-encoded SHA-256 digest the downloaded archive must match
    ///
    /// # Errors
    ///
    /// Returns [`SnapshotError`] for network failures, file system errors, or validation failures.
    /// Returns [`SnapshotError::DigestMismatch`] if `expected_sha256` is set and does not match.
    pub async fn download_snapshot(
        &self,
        url: &str,
        target_path: &Path,
        expected_sha256: Option<&str>,
    ) -> SnapshotResult<()> {
        self.download_snapshot_with_retry(url, target_path, expected_sha256, self.config.max_retries)
            .await
    }

    /// Downloads a snapshot with configurable retry logic.
    ///
    /// Implements exponential backoff between retry attempts for HTTP/HTTPS URLs.
    /// Local file:// URLs are handled without retry logic. Certain errors
    /// (like 4xx HTTP status codes or insufficient disk space) will not be retried.
    ///
    /// # Arguments
    ///
    /// * `url` - The HTTP/HTTPS or file:// URL to download/copy from
    /// * `target_path` - Local path where the downloaded file will be saved
    /// * `expected_sha256` - Optional hex-encoded SHA-256 digest the downloaded archive must match
    /// * `max_retries` - Maximum number of retry attempts (ignored for file:// URLs)
    ///
    /// # Errors
    ///
    /// Returns `SnapshotError` for various failure conditions including:
    /// - Network errors (with retry)
    /// - HTTP errors (4xx without retry, 5xx with retry)
    /// - Insufficient disk space (without retry)
    /// - File size exceeding limits (without retry)
    /// - Malformed `expected_sha256` (before any download, without retry)
    /// - Digest mismatch (without retry; the downloaded file is removed)
    pub async fn download_snapshot_with_retry(
        &self,
        url: &str,
        target_path: &Path,
        expected_sha256: Option<&str>,
        max_retries: u32,
    ) -> SnapshotResult<()> {
        let expected_sha256 = expected_sha256
            .map(|digest| {
                normalize_sha256_hex(digest).ok_or_else(|| {
                    SnapshotError::Configuration("expected snapshot SHA-256 must be 64 hex characters".to_string())
                })
            })
            .transpose()?;

        let backoff = backon::ExponentialBuilder::default().with_max_times(max_retries as usize);

        (|| async {
            self.download_snapshot_once(url, target_path, expected_sha256.as_deref())
                .await
        })
        .retry(backoff)
        .sleep(FuturesTimerSleeper)
        .when(|err| {
            !matches!(
                err,
                SnapshotError::TooLarge { .. }
                    | SnapshotError::InsufficientSpace { .. }
                    | SnapshotError::DigestMismatch { .. }
                    | SnapshotError::HttpStatus { status: 400..=499 },
            )
        })
        .notify(|error, _dur| {
            error!(%error, "Download attempt failed");
        })
        .await
    }

    /// Performs a single download attempt and verifies the result against `expected_sha256`.
    ///
    /// `expected_sha256` must already be normalized with [`normalize_sha256_hex`].
    async fn download_snapshot_once(
        &self,
        url: &str,
        target_path: &Path,
        expected_sha256: Option<&str>,
    ) -> SnapshotResult<()> {
        self.fetch_snapshot_once(url, target_path).await?;

        match expected_sha256 {
            Some(expected) => verify_sha256(target_path, expected),
            None => Ok(()),
        }
    }

    /// Fetches the snapshot into `target_path` without integrity verification.
    async fn fetch_snapshot_once(&self, url: &str, target_path: &Path) -> SnapshotResult<()> {
        let redacted_url = redact_url(url);
        info!(%redacted_url, "Downloading logs snapshot file");

        // Check available disk space
        let parent_dir = target_path
            .parent()
            .ok_or_else(|| SnapshotError::InvalidData("Target path has no parent directory".to_string()))?;
        self.check_disk_space(parent_dir).await?;

        // Handle file:// URLs for local file access
        if url.starts_with("file://") {
            return self.copy_local_file(url, target_path).await;
        }

        // Send GET request
        let response = self.client.get(url).send().await?;

        // Check response status
        if !response.status().is_success() {
            return Err(SnapshotError::HttpStatus {
                status: response.status().as_u16(),
            });
        }

        // Check content length
        if let Some(content_length) = response.content_length() {
            if content_length > self.config.max_size {
                return Err(SnapshotError::TooLarge {
                    size: content_length,
                    max_size: self.config.max_size,
                });
            }
        }

        // Create parent directory if it doesn't exist
        if let Some(parent) = target_path.parent() {
            fs::create_dir_all(parent)?;
        }

        // Fail if content length is not available
        let total_bytes = response
            .content_length()
            .ok_or_else(SnapshotError::ContentLengthMissing)?;

        // Create file writer using futures-io
        let file = File::create(target_path)?;
        let file_writer = Arc::new(Mutex::new(AllowStdIo::new(file)));

        // Use AtomicU64 for thread-safe progress tracking
        let downloaded = Arc::new(AtomicU64::new(0));

        let stream = response.bytes_stream();

        // Process each chunk with progress tracking and size checking
        stream
            .map_err(SnapshotError::Network)
            .try_for_each(|chunk| {
                let downloaded = downloaded.clone();
                let file_writer = file_writer.clone();
                let max_size = self.config.max_size;

                async move {
                    let received_bytes =
                        downloaded.fetch_add(chunk.len() as u64, Ordering::Relaxed) + chunk.len() as u64;

                    // Check size limit and abort if exceeded
                    if received_bytes > max_size {
                        return Err(SnapshotError::TooLarge {
                            size: received_bytes,
                            max_size,
                        });
                    }

                    // Progress reporting, only per 1MB or at the end
                    let progress = (received_bytes as f64 / total_bytes as f64) * 100.0;
                    if received_bytes.is_multiple_of(1024 * 1024) || received_bytes == total_bytes {
                        debug!(
                            progress = format!("{:.1}%", progress),
                            %received_bytes, %total_bytes, "Logs snapshot download progress"
                        );
                    }

                    // Write chunk to file using AsyncWriteExt
                    {
                        let mut writer = file_writer.lock().await;
                        writer.write_all(&chunk).await.map_err(SnapshotError::Io)?;
                        writer.flush().await.map_err(SnapshotError::Io)?;
                    }

                    Ok(())
                }
            })
            .await?;

        let downloaded_bytes = downloaded.load(Ordering::Relaxed);
        info!(%downloaded_bytes, to = %target_path.display(), "Logs snapshot file downloaded");

        Ok(())
    }

    /// Copies a local file from a file:// URL to the target path.
    ///
    /// Validates file existence, checks size limits, and copies the file to the target location.
    ///
    /// # Arguments
    ///
    /// * `url` - File URL in format `file:///absolute/path/to/file`
    /// * `target_path` - Destination file path
    ///
    /// # Errors
    ///
    /// * [`SnapshotError::InvalidData`] - Invalid file:// URL format
    /// * [`SnapshotError::Io`] - File not found or permission errors
    /// * [`SnapshotError::TooLarge`] - File exceeds size limit
    async fn copy_local_file(&self, url: &str, target_path: &Path) -> SnapshotResult<()> {
        // Parse the file path from the URL
        let file_path = url
            .strip_prefix("file://")
            .ok_or_else(|| SnapshotError::InvalidData("Invalid file:// URL format".to_string()))?;

        let source_path = Path::new(file_path);

        // Validate path to prevent directory traversal
        let canonical_path = source_path.canonicalize().map_err(SnapshotError::Io)?;

        // Check if source file exists
        if !canonical_path.exists() {
            return Err(SnapshotError::Io(Error::new(
                ErrorKind::NotFound,
                format!("Local file not found: {file_path}"),
            )));
        }

        // Check file size
        let metadata = fs::metadata(canonical_path.clone())?;
        if metadata.len() > self.config.max_size {
            return Err(SnapshotError::TooLarge {
                size: metadata.len(),
                max_size: self.config.max_size,
            });
        }

        // Create parent directory if it doesn't exist
        if let Some(parent) = target_path.parent() {
            fs::create_dir_all(parent)?;
        }

        // Copy the file using futures-io
        let copied_bytes = fs::copy(canonical_path.clone(), target_path)?;
        info!(
            %copied_bytes, from = %canonical_path.display(), to = %target_path.display(),
            "Copied local snapshot file",
        );

        Ok(())
    }

    /// Checks if there's sufficient disk space available for download and extraction.
    ///
    /// Validates that the target directory has at least 3x the maximum download size
    /// available to account for:
    /// 1. The downloaded archive
    /// 2. Extracted files
    /// 3. Safety margin for system operations
    ///
    /// # Arguments
    ///
    /// * `dir` - Directory to check for available space
    ///
    /// # Errors
    ///
    /// Returns `SnapshotError::InsufficientSpace` if available space is below requirements
    pub async fn check_disk_space(&self, dir: &Path) -> SnapshotResult<()> {
        // Create directory if it doesn't exist
        fs::create_dir_all(dir)?;

        // Check if directory exists and is accessible
        let metadata = fs::metadata(dir)?;
        if !metadata.is_dir() {
            return Err(SnapshotError::Io(Error::new(
                ErrorKind::NotFound,
                "Target directory does not exist",
            )));
        }

        // Get available disk space
        let available_bytes = get_available_disk_space(dir)?;

        // We need at least 3x the max size to account for:
        // 1. Downloaded archive
        // 2. Extracted files
        // 3. Safety margin for system operations
        let required_bytes = self.config.max_size * 3;

        if available_bytes < required_bytes {
            return Err(SnapshotError::InsufficientSpace {
                required: required_bytes / (1024 * 1024),
                available: available_bytes / (1024 * 1024),
            });
        }

        Ok(())
    }
}

/// Validates a hex-encoded SHA-256 digest and returns it in lowercase.
///
/// Surrounding whitespace is ignored. Returns `None` unless the digest is exactly 64 hex characters.
pub(crate) fn normalize_sha256_hex(digest: &str) -> Option<String> {
    let digest = digest.trim();
    (digest.len() == 64 && digest.bytes().all(|b| b.is_ascii_hexdigit())).then(|| digest.to_ascii_lowercase())
}

/// Computes the SHA-256 digest of the file at `path` and returns it as lowercase hex.
fn sha256_file_hex(path: &Path) -> SnapshotResult<String> {
    let mut file = File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(hasher.finalize().iter().map(|byte| format!("{byte:02x}")).collect())
}

/// Verifies that the file at `path` has the SHA-256 digest `expected` (normalized lowercase hex).
///
/// On mismatch the file is removed so it cannot be extracted later, and
/// [`SnapshotError::DigestMismatch`] is returned.
fn verify_sha256(path: &Path, expected: &str) -> SnapshotResult<()> {
    let actual = sha256_file_hex(path)?;
    if actual != expected {
        if let Err(error) = fs::remove_file(path) {
            warn!(%error, path = %path.display(), "failed to remove snapshot archive with mismatching digest");
        }
        return Err(SnapshotError::DigestMismatch {
            expected: expected.to_string(),
            actual,
        });
    }

    info!(sha256 = %actual, "Logs snapshot archive digest verified");
    Ok(())
}

/// Gets available disk space in bytes for the given directory using cross-platform sysinfo.
///
/// This function uses the sysinfo crate to provide platform-independent disk space checking.
/// It finds the disk/mount point that contains the specified directory and returns the
/// available space on that disk.
///
/// # Arguments
///
/// * `dir` - Directory path to check (will be canonicalized)
///
/// # Returns
///
/// Available space in bytes on the disk containing the directory
///
/// # Errors
///
/// - `SnapshotError::Io` if the path cannot be canonicalized
/// - `SnapshotError::InvalidData` if no disks are found on the system
fn get_available_disk_space(dir: &Path) -> SnapshotResult<u64> {
    // Find the disk that contains the given directory
    let target_path = dir.canonicalize().map_err(SnapshotError::Io)?;

    // Find the disk with the longest matching mount point
    let disks = Disks::new_with_refreshed_list();

    // Filter out disks with non matching mount points
    let mut usable_disks = disks
        .list()
        .iter()
        .filter(|d| target_path.starts_with(d.mount_point()))
        .collect::<Vec<_>>();

    // Sort disks by mount point length (longest first)
    usable_disks.sort_by(|a, b| {
        b.mount_point()
            .as_os_str()
            .len()
            .cmp(&(a.mount_point().as_os_str().len()))
    });

    // If no usable disks found, return an error
    usable_disks.first().map_or_else(
        || {
            Err(SnapshotError::InvalidData(format!(
                "Could not determine disk space for path: {dir:?}"
            )))
        },
        |disk| Ok(disk.available_space()),
    )
}

#[cfg(test)]
mod tests {
    use tempfile::TempDir;

    use super::*;

    #[tokio::test]
    async fn test_disk_space_validation() {
        let temp_dir = TempDir::new().unwrap();
        let downloader = SnapshotDownloader::new().expect("Failed to create SnapshotDownloader");

        // Test with available disk space (this should pass)
        let result = downloader.check_disk_space(temp_dir.path()).await;
        assert!(result.is_ok());

        // Test with invalid directory path
        let invalid_path = temp_dir.path().join("nonexistent/nested/path");
        let result = downloader.check_disk_space(&invalid_path).await;
        // Should create the directory and succeed
        assert!(result.is_ok());
    }

    #[test]
    fn test_normalize_sha256_hex() {
        let digest = "E3B0C44298FC1C149AFBF4C8996FB92427AE41E4649B934CA495991B7852B855";
        assert_eq!(
            normalize_sha256_hex(&format!(" {digest}\n")),
            Some(digest.to_ascii_lowercase())
        );
        assert_eq!(normalize_sha256_hex(&digest[..63]), None);
        assert_eq!(normalize_sha256_hex(&format!("{digest}0")), None);
        assert_eq!(normalize_sha256_hex(&format!("{}g", &digest[..63])), None);
        assert_eq!(normalize_sha256_hex(""), None);
    }

    #[tokio::test]
    async fn test_digest_mismatch_is_not_retried() {
        let temp_dir = TempDir::new().unwrap();
        let target_path = temp_dir.path().join("snapshot.tar.xz");
        let downloader = SnapshotDownloader::with_config(DownloadConfig {
            max_retries: 3,
            ..Default::default()
        })
        .expect("Failed to create SnapshotDownloader");

        let mut server = mockito::Server::new_async().await;
        let snapshot = server
            .mock("GET", "/snapshot")
            .with_status(200)
            .with_body("not the expected archive")
            .expect(1)
            .create_async()
            .await;

        let result = downloader
            .download_snapshot(
                &format!("{}/snapshot", server.url()),
                &target_path,
                Some("0000000000000000000000000000000000000000000000000000000000000000"),
            )
            .await;

        assert!(
            matches!(result, Err(SnapshotError::DigestMismatch { .. })),
            "expected DigestMismatch, got {result:?}"
        );
        assert!(
            !target_path.exists(),
            "archive with mismatching digest should be removed"
        );
        snapshot.assert_async().await;
    }

    #[tokio::test]
    async fn test_malformed_expected_digest_is_rejected_before_download() {
        let temp_dir = TempDir::new().unwrap();
        let target_path = temp_dir.path().join("snapshot.tar.xz");
        let downloader = SnapshotDownloader::new().expect("Failed to create SnapshotDownloader");

        let mut server = mockito::Server::new_async().await;
        let snapshot = server
            .mock("GET", "/snapshot")
            .with_status(200)
            .with_body("archive")
            .expect(0)
            .create_async()
            .await;

        let result = downloader
            .download_snapshot(&format!("{}/snapshot", server.url()), &target_path, Some("abc"))
            .await;

        assert!(
            matches!(result, Err(SnapshotError::Configuration(_))),
            "expected Configuration error, got {result:?}"
        );
        assert!(!target_path.exists());
        snapshot.assert_async().await;
    }

    const EMPTY_SHA256: &str = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";

    #[tokio::test]
    async fn test_empty_file_matches_empty_digest_and_rejects_other() {
        let temp_dir = TempDir::new().unwrap();
        let source = temp_dir.path().join("empty.tar.xz");
        fs::write(&source, b"").unwrap();
        let url = format!("file://{}", source.display());
        let downloader = SnapshotDownloader::new().unwrap();

        let ok_target = temp_dir.path().join("ok.tar.xz");
        downloader
            .download_snapshot(&url, &ok_target, Some(EMPTY_SHA256))
            .await
            .expect("empty file should match the empty-input digest");

        let bad_target = temp_dir.path().join("bad.tar.xz");
        let result = downloader
            .download_snapshot(&url, &bad_target, Some(&"1".repeat(64)))
            .await;
        assert!(
            matches!(result, Err(SnapshotError::DigestMismatch { .. })),
            "got {result:?}"
        );
        assert!(!bad_target.exists());
    }

    #[tokio::test]
    async fn test_truncated_file_is_rejected_by_digest() {
        let temp_dir = TempDir::new().unwrap();
        let full = b"0123456789abcdef".repeat(1024);
        let expected = {
            let full_path = temp_dir.path().join("full");
            fs::write(&full_path, &full).unwrap();
            sha256_file_hex(&full_path).unwrap()
        };
        let source = temp_dir.path().join("truncated.tar.xz");
        fs::write(&source, &full[..full.len() - 1]).unwrap();
        let target = temp_dir.path().join("target.tar.xz");

        let result = SnapshotDownloader::new()
            .unwrap()
            .download_snapshot(&format!("file://{}", source.display()), &target, Some(&expected))
            .await;
        match result {
            Err(SnapshotError::DigestMismatch { expected: e, actual }) => {
                assert_eq!(e, expected);
                assert_ne!(actual, expected);
            }
            other => panic!("expected DigestMismatch, got {other:?}"),
        }
        assert!(!target.exists());
    }

    #[tokio::test]
    async fn test_http_download_with_matching_uppercase_padded_digest() {
        let temp_dir = TempDir::new().unwrap();
        let target = temp_dir.path().join("snapshot.tar.xz");
        let body = b"snapshot bytes";
        let reference = temp_dir.path().join("reference");
        fs::write(&reference, body).unwrap();
        let digest = format!("\t{}\n", sha256_file_hex(&reference).unwrap().to_ascii_uppercase());

        let mut server = mockito::Server::new_async().await;
        let _m = server
            .mock("GET", "/snapshot")
            .with_status(200)
            .with_body(body)
            .create_async()
            .await;

        SnapshotDownloader::new()
            .unwrap()
            .download_snapshot(&format!("{}/snapshot", server.url()), &target, Some(&digest))
            .await
            .expect("matching digest should pass over http");
        assert_eq!(fs::read(&target).unwrap(), body);
    }

    #[tokio::test]
    async fn test_whitespace_only_digest_is_rejected() {
        let temp_dir = TempDir::new().unwrap();
        let source = temp_dir.path().join("a.tar.xz");
        fs::write(&source, b"x").unwrap();
        let target = temp_dir.path().join("t.tar.xz");
        let result = SnapshotDownloader::new()
            .unwrap()
            .download_snapshot(&format!("file://{}", source.display()), &target, Some("   "))
            .await;
        assert!(matches!(result, Err(SnapshotError::Configuration(_))), "got {result:?}");
    }

    #[tokio::test]
    async fn test_enhanced_error_messages() {
        let temp_dir = TempDir::new().unwrap();
        let downloader = SnapshotDownloader::with_config(DownloadConfig {
            max_retries: 0,
            ..Default::default()
        })
        .expect("Failed to create SnapshotDownloader");

        // Test invalid URL error
        let result = downloader
            .download_snapshot("invalid://url", temp_dir.path(), None)
            .await;
        assert!(result.is_err());

        // Test an HTTP not-found response without depending on a public service.
        let mut server = mockito::Server::new_async().await;
        let _not_found = server.mock("GET", "/snapshot").with_status(404).expect(1).create();
        let result = downloader
            .download_snapshot(&format!("{}/snapshot", server.url()), temp_dir.path(), None)
            .await;
        assert!(result.is_err());
    }
}
