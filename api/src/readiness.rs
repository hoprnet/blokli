//! Readiness state management for the API server
//!
//! This module provides functionality to track and query the readiness state of the server.
//! The server is considered ready when:
//! 1. Database is connected and has chain info
//! 2. RPC is reachable
//! 3. Indexer lag is within acceptable limits
//! 4. Post-finality verification found no mismatch
//!
//! The readiness state is cached and updated periodically, with out-of-band refreshes for fast transitions.
//! Updates are triggered by:
//! - /readyz endpoint calls (immediate check)
//! - Indexer completion signals (immediate check)
//! - Readiness subscription connection checks
//! - Periodic background refreshes (`readiness_check_interval`)

use std::sync::Arc;

use async_broadcast::{Receiver, Sender, broadcast};
use async_graphql::Enum;
use blokli_chain_indexer::verification::VerificationStatus;
use blokli_chain_rpc::rpc::RpcOperations;
use blokli_db_entity::prelude::ChainInfo;
use sea_orm::{DatabaseConnection, EntityTrait};
use serde::Serialize;
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};

use crate::{config::HealthConfig, metrics};

/// Readiness state of the API server
#[derive(Debug, Clone, Copy, PartialEq, Eq, Enum)]
pub enum ReadinessState {
    /// Server is ready to accept GraphQL requests
    Ready,
    /// Server is not ready (usually during initial indexing)
    NotReady,
}

/// Status value of a check that passed.
const STATUS_HEALTHY: &str = "healthy";

/// Status value of a check that failed.
const STATUS_UNHEALTHY: &str = "unhealthy";

/// Error text of the verification check when post-finality verification found a mismatch.
const VERIFICATION_MISMATCH_ERROR: &str = "Post-finality verification found a mismatch";

/// Result of one readiness evaluation.
///
/// The report contains every check, also when an earlier check already decided the state.
/// Thus the `/readyz` body keeps its diagnostic details when verification failed or the
/// database is unreachable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReadinessReport {
    /// Readiness decision for the cached state, the HTTP status, and the health metric.
    pub state: ReadinessState,
    /// Health status label for the `blokli_health` metric.
    pub health_status: &'static str,
    /// Diagnostic status of each subsystem, as `/readyz` shows it.
    pub checks: ReadinessChecks,
}

/// Individual readiness checks that `/readyz` shows.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ReadinessChecks {
    /// Database connectivity check.
    pub database: CheckStatus,
    /// RPC tip check.
    pub rpc: RpcCheckStatus,
    /// Indexer lag check.
    pub indexer: IndexerCheckStatus,
    /// Post-finality verification check.
    pub verification: CheckStatus,
}

/// Generic check status.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CheckStatus {
    /// `"healthy"` or `"unhealthy"`.
    pub status: String,
    /// Description of the failure when the check is unhealthy.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl CheckStatus {
    /// Creates a healthy check status.
    pub fn healthy() -> Self {
        Self {
            status: STATUS_HEALTHY.to_string(),
            error: None,
        }
    }

    /// Creates an unhealthy check status with an error message.
    pub fn unhealthy(error: impl Into<String>) -> Self {
        Self {
            status: STATUS_UNHEALTHY.to_string(),
            error: Some(error.into()),
        }
    }
}

/// RPC check status with the confirmed block number.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RpcCheckStatus {
    /// `"healthy"` or `"unhealthy"`.
    pub status: String,
    /// Confirmed block from the chain tip tracker.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub block_number: Option<u64>,
    /// Description of the failure when the check is unhealthy.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Indexer check status with lag information.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct IndexerCheckStatus {
    /// `"healthy"` or `"unhealthy"`.
    pub status: String,
    /// Last indexed block, when chain info exists.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_indexed_block: Option<i64>,
    /// Number of blocks between the confirmed block and the indexer watermark.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lag: Option<u64>,
    /// Description of the failure when the lag is more than the threshold.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Shared readiness state tracker with caching and periodic updates
#[derive(Clone)]
pub struct ReadinessChecker {
    cached_state: Arc<RwLock<ReadinessState>>,
    updates: Sender<ReadinessState>,
    _updates_rx: Arc<Receiver<ReadinessState>>,
    db: DatabaseConnection,
    rpc_operations: Arc<RpcOperations<blokli_chain_rpc::transport::ReqwestClient>>,
    health_config: HealthConfig,
    verification_status: VerificationStatus,
}

impl ReadinessChecker {
    /// Create a new readiness checker
    pub fn new(
        db: DatabaseConnection,
        rpc_operations: Arc<RpcOperations<blokli_chain_rpc::transport::ReqwestClient>>,
        health_config: HealthConfig,
    ) -> Self {
        let (mut updates, updates_rx) = broadcast(16);
        updates.set_overflow(true);

        Self {
            cached_state: Arc::new(RwLock::new(ReadinessState::NotReady)),
            updates,
            _updates_rx: Arc::new(updates_rx),
            db,
            rpc_operations,
            health_config,
            verification_status: VerificationStatus::default(),
        }
    }

    /// Attaches the post-finality verification status that the readiness check reads.
    #[must_use]
    pub fn with_verification_status(mut self, verification_status: VerificationStatus) -> Self {
        self.verification_status = verification_status;
        self
    }

    /// Get the current readiness state. Returns `NotReady` when verification failed.
    pub async fn get(&self) -> ReadinessState {
        let cached = self.cached_state.read().await;
        if self.verification_status.failed() {
            return ReadinessState::NotReady;
        }
        *cached
    }

    pub fn subscribe(&self) -> Receiver<ReadinessState> {
        self.updates.new_receiver()
    }

    /// Perform a full readiness check and update the cached state.
    ///
    /// `/readyz` and the indexer completion signals use this function. It reads the verification
    /// status again under the publication lock. Thus a check that started before a mismatch
    /// cannot restore readiness.
    pub async fn check_and_update(&self) -> ReadinessReport {
        let mut report = self.check_readiness().await;
        let mut cached = self.cached_state.write().await;
        if self.verification_status.failed() {
            report.state = ReadinessState::NotReady;
            report.health_status = metrics::HEALTH_STATUS_VERIFICATION_FAILED;
            report.checks.verification = CheckStatus::unhealthy(VERIFICATION_MISMATCH_ERROR);
        }

        self.store_state(&mut cached, report.state, report.health_status);

        report
    }

    /// Stores a readiness decision and updates the health metric.
    ///
    /// When the state changes, it also notifies the subscribers.
    fn store_state(&self, cached: &mut ReadinessState, new_state: ReadinessState, health_status: &'static str) {
        let old_state = *cached;
        *cached = new_state;

        // Update Prometheus metrics
        metrics::set_health_status(health_status);

        // Log state transitions
        if old_state != new_state {
            let _ = self.updates.try_broadcast(new_state);
            match new_state {
                ReadinessState::Ready => {
                    info!(status = health_status, "Readiness state transitioned to READY");
                }
                ReadinessState::NotReady => {
                    info!(status = health_status, "Readiness state transitioned to NOT_READY");
                }
            }
        }
    }

    /// Trigger an out-of-band readiness check (used by indexer completion signal)
    pub async fn trigger_update(&self) {
        let _ = self.check_and_update().await;
    }

    /// Unconditionally set the cached state to `Ready`. Only for use in tests.
    pub async fn force_ready(&self) {
        *self.cached_state.write().await = ReadinessState::Ready;
    }

    /// Start a background task that periodically updates the readiness state.
    ///
    /// It also starts the background driver of the chain tip tracker, if it is not running.
    /// Thus API-only pods and halted processes still serve cached tips and a live `fcrStatus`.
    pub fn start_periodic_updates(self) {
        self.rpc_operations.tip_tracker().ensure_started();
        let interval = self.health_config.readiness_check_interval;
        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(interval);
            loop {
                ticker.tick().await;
                let _ = self.check_and_update().await;
            }
        });
    }

    /// Collect the database, RPC, and lag diagnostics without the publication lock.
    ///
    /// `check_and_update` applies the verification status later, under that lock.
    async fn check_readiness(&self) -> ReadinessReport {
        // 1. Check database connectivity by querying chain_info
        let (last_indexed_block, database, database_health_status) = match ChainInfo::find().one(&self.db).await {
            Ok(Some(info)) => {
                if info.verification_failed_at.is_some() {
                    self.verification_status.mark_failed();
                }
                (Some(info.last_indexed_block), CheckStatus::healthy(), None)
            }
            Ok(None) => (
                None,
                CheckStatus::unhealthy("No chain info found - indexer may not have started"),
                Some(metrics::HEALTH_STATUS_UNSYNCHED),
            ),
            Err(e) => {
                error!("Database check failed: {}", e);
                (
                    None,
                    CheckStatus::unhealthy(e.to_string()),
                    Some(metrics::HEALTH_STATUS_CORRUPTED),
                )
            }
        };

        // 2. Check RPC connectivity and get the tracker's confirmed tip
        let rpc = match self.rpc_operations.get_block_number().await {
            Ok(block) => RpcCheckStatus {
                status: STATUS_HEALTHY.to_string(),
                block_number: Some(block),
                error: None,
            },
            Err(e) => {
                error!("RPC check failed: {}", e);
                RpcCheckStatus {
                    status: STATUS_UNHEALTHY.to_string(),
                    block_number: None,
                    error: Some(e.to_string()),
                }
            }
        };

        // 3. Check indexer lag
        let mut indexer = IndexerCheckStatus {
            status: STATUS_HEALTHY.to_string(),
            last_indexed_block,
            lag: None,
            error: None,
        };
        if let (Some(indexed), Some(rpc_block)) = (last_indexed_block, rpc.block_number) {
            let lag = rpc_block.saturating_sub(u64::try_from(indexed).unwrap_or(0));
            indexer.lag = Some(lag);

            if lag > self.health_config.max_indexer_lag {
                warn!(
                    %lag,
                    confirmed_block = %rpc_block,
                    %indexed,
                    max_lag = self.health_config.max_indexer_lag,
                    "Indexer lag exceeds threshold (lag compared against the tracker's confirmed tip)"
                );
                indexer.status = STATUS_UNHEALTHY.to_string();
                indexer.error = Some(format!(
                    "indexer lag exceeds threshold ({} > {} blocks)",
                    lag, self.health_config.max_indexer_lag
                ));
            } else {
                debug!(
                    %lag,
                    confirmed_block = %rpc_block,
                    indexed = %indexed,
                    "Indexer within acceptable lag of confirmed blocks"
                );
            }
        }

        let health_status = if let Some(status) = database_health_status {
            status
        } else if rpc.error.is_some() {
            metrics::HEALTH_STATUS_TIMEOUT
        } else if indexer.error.is_some() || indexer.lag.is_none() {
            metrics::HEALTH_STATUS_UNSYNCHED
        } else {
            metrics::HEALTH_STATUS_OK
        };

        let state = if health_status == metrics::HEALTH_STATUS_OK {
            ReadinessState::Ready
        } else {
            ReadinessState::NotReady
        };

        ReadinessReport {
            state,
            health_status,
            checks: ReadinessChecks {
                database,
                rpc,
                indexer,
                verification: CheckStatus::healthy(),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{
        sync::{
            Arc,
            atomic::{AtomicBool, Ordering},
        },
        thread,
        time::{Duration, Instant},
    };

    use anyhow::{Result, bail};
    use blokli_chain_indexer::IndexerState;
    use blokli_chain_rpc::{
        rpc::{RpcOperations, RpcOperationsConfig},
        tip::ConfirmationMode,
        transport::ReqwestClient,
    };
    use blokli_db::{BlokliDbGeneralModelOperations, TargetDb, db::BlokliDb, info::BlokliDbInfoOperations};
    use hopr_bindings::exports::alloy::{rpc::client::ClientBuilder, transports::http::ReqwestTransport};
    use mockito::{Matcher, Mock, Request, Server, ServerGuard};
    use serde_json::{Value, json};
    use tokio::{task, time};

    use super::{ReadinessChecker, ReadinessState};
    use crate::{config::HealthConfig, metrics};

    /// Block number of the latest header in the RPC stub answers.
    const LATEST_BLOCK: u64 = 100;

    /// Longest time that a test waits for the RPC stub or for the checker.
    const STUB_TIMEOUT: Duration = Duration::from_secs(10);

    /// Request body pattern that matches block requests to the RPC stub.
    const GET_BLOCK_BY_NUMBER_PATTERN: &str = r#""method"\s*:\s*"eth_getBlockByNumber""#;

    /// Answers a batched block request with a header for every tag that has a block number.
    fn tip_batch_response(request: &Request, latest: u64, safe: Option<u64>, finalized: Option<u64>) -> Vec<u8> {
        let body: Value =
            serde_json::from_slice(request.body().expect("request body")).expect("valid JSON request body");
        let responses = body
            .as_array()
            .expect("a batched block request is a JSON array")
            .iter()
            .map(|call| {
                let tag = call["params"][0].as_str().unwrap_or_default();
                let number = match tag {
                    "latest" => Some(latest),
                    "safe" => safe,
                    "finalized" => finalized,
                    _ => None,
                };
                let result = number.map_or(
                    Value::Null,
                    |number| json!({"number": format!("0x{number:x}"), "timestamp": "0x1"}),
                );
                json!({
                    "jsonrpc": "2.0",
                    "id": call["id"].clone(),
                    "result": result
                })
            })
            .collect::<Vec<_>>();

        serde_json::to_vec(&responses).expect("batch response should serialize")
    }

    /// Answers the chain tip batch with a latest header and no safe or finalized tag.
    fn mock_tip_without_confirmation_tags(server: &mut ServerGuard) -> Mock {
        server
            .mock("POST", "/")
            .match_body(Matcher::Regex(GET_BLOCK_BY_NUMBER_PATTERN.to_string()))
            .with_body_from_request(|request| tip_batch_response(request, LATEST_BLOCK, None, None))
            .expect_at_least(1)
            .create()
    }

    /// Answers the chain tip batch after the test sets `release`.
    ///
    /// It sets `arrived` when the request arrives.
    fn mock_tip_released_by_test(server: &mut ServerGuard, arrived: Arc<AtomicBool>, release: Arc<AtomicBool>) -> Mock {
        server
            .mock("POST", "/")
            .match_body(Matcher::Regex(GET_BLOCK_BY_NUMBER_PATTERN.to_string()))
            .with_body_from_request(move |request| {
                arrived.store(true, Ordering::SeqCst);

                let deadline = Instant::now() + STUB_TIMEOUT;
                while !release.load(Ordering::SeqCst) && Instant::now() < deadline {
                    thread::sleep(Duration::from_millis(1));
                }

                tip_batch_response(request, LATEST_BLOCK, None, None)
            })
            .expect_at_least(1)
            .create()
    }

    fn rpc_operations_for(
        server: &ServerGuard,
        config: RpcOperationsConfig,
    ) -> Result<Arc<RpcOperations<ReqwestClient>>> {
        let transport = ReqwestTransport::new(server.url().parse()?);
        let rpc_client = ClientBuilder::default().transport(transport.clone(), transport.guess_local());

        Ok(Arc::new(RpcOperations::new(
            rpc_client,
            ReqwestClient::new(),
            config,
            None,
        )?))
    }

    async fn wait_for_tip_request(arrived: &AtomicBool) -> Result<()> {
        let deadline = Instant::now() + STUB_TIMEOUT;

        while !arrived.load(Ordering::SeqCst) {
            if Instant::now() >= deadline {
                bail!("the RPC stub did not receive the chain tip request");
            }
            time::sleep(Duration::from_millis(5)).await;
        }

        Ok(())
    }

    #[tokio::test]
    async fn missing_safe_tag_reports_not_ready() -> Result<()> {
        let mut server = Server::new_async().await;
        let tip = mock_tip_without_confirmation_tags(&mut server);
        let rpc = rpc_operations_for(
            &server,
            RpcOperationsConfig {
                confirmation_mode: ConfirmationMode::Safe,
                ..RpcOperationsConfig::default()
            },
        )?;
        let db = BlokliDb::new_in_memory().await?;
        let checker = ReadinessChecker::new(db.conn(TargetDb::Index).clone(), rpc, HealthConfig::default());

        let report = checker.check_and_update().await;

        tip.assert();
        assert_eq!(report.state, ReadinessState::NotReady);
        assert!(report.checks.rpc.error.as_deref().is_some_and(|e| e.contains("safe")));
        assert!(report.checks.indexer.lag.is_none());
        Ok(())
    }

    #[tokio::test]
    async fn verification_failure_reports_not_ready() -> Result<()> {
        let mut server = Server::new_async().await;
        let tip = mock_tip_without_confirmation_tags(&mut server);

        let db = BlokliDb::new_in_memory().await?;
        let indexer_state = IndexerState::default();
        indexer_state.mark_verification_failed();

        let checker = ReadinessChecker::new(
            db.conn(TargetDb::Index).clone(),
            rpc_operations_for(&server, RpcOperationsConfig::default())?,
            HealthConfig::default(),
        )
        .with_verification_status(indexer_state.verification_status());

        let report = checker.check_and_update().await;

        tip.assert();
        assert_eq!(report.state, ReadinessState::NotReady);
        assert_eq!(report.health_status, metrics::HEALTH_STATUS_VERIFICATION_FAILED);
        assert_eq!(report.checks.verification.status, "unhealthy");
        assert!(report.checks.rpc.error.is_none());
        assert!(report.checks.rpc.block_number.is_some());
        Ok(())
    }

    #[tokio::test]
    async fn persisted_verification_failure_in_db_reports_not_ready() -> Result<()> {
        let mut server = Server::new_async().await;
        let tip = mock_tip_without_confirmation_tags(&mut server);

        let db = BlokliDb::new_in_memory().await?;
        db.set_indexer_state_info(None, 100).await?;
        db.set_verification_failed_at(Some(95)).await?;

        // Standalone API replicas have a fresh in-memory VerificationStatus and must honor
        // the persisted chain_info.verification_failed_at halt marker.
        let checker = ReadinessChecker::new(
            db.conn(TargetDb::Index).clone(),
            rpc_operations_for(&server, RpcOperationsConfig::default())?,
            HealthConfig::default(),
        );

        let report = checker.check_and_update().await;

        tip.assert();
        assert_eq!(report.state, ReadinessState::NotReady);
        assert_eq!(report.health_status, metrics::HEALTH_STATUS_VERIFICATION_FAILED);
        assert_eq!(report.checks.verification.status, "unhealthy");
        assert_eq!(checker.get().await, ReadinessState::NotReady);
        Ok(())
    }

    #[tokio::test]
    async fn start_periodic_updates_starts_tip_tracker() -> Result<()> {
        let mut server = Server::new_async().await;
        let tip = mock_tip_without_confirmation_tags(&mut server);

        let db = BlokliDb::new_in_memory().await?;
        let rpc = rpc_operations_for(&server, RpcOperationsConfig::default())?;
        let checker = ReadinessChecker::new(
            db.conn(TargetDb::Index).clone(),
            rpc.clone(),
            HealthConfig {
                readiness_check_interval: Duration::from_secs(3600),
                ..HealthConfig::default()
            },
        );

        assert!(rpc.tip_tracker().current().is_none());
        checker.start_periodic_updates();

        time::timeout(Duration::from_secs(5), async {
            while rpc.tip_tracker().current().is_none() {
                time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await?;

        tip.assert();
        assert!(rpc.tip_tracker().current().is_some());
        Ok(())
    }

    #[tokio::test]
    async fn check_started_before_a_verification_failure_cannot_restore_readiness() -> Result<()> {
        let mut server = Server::new_async().await;
        let arrived = Arc::new(AtomicBool::new(false));
        let release = Arc::new(AtomicBool::new(false));
        let tip = mock_tip_released_by_test(&mut server, arrived.clone(), release.clone());

        let db = BlokliDb::new_in_memory().await?;
        let indexer_state = IndexerState::default();
        let checker = ReadinessChecker::new(
            db.conn(TargetDb::Index).clone(),
            rpc_operations_for(&server, RpcOperationsConfig::default())?,
            HealthConfig {
                max_indexer_lag: 100,
                ..HealthConfig::default()
            },
        )
        .with_verification_status(indexer_state.verification_status());
        checker.force_ready().await;
        let mut updates = checker.subscribe();

        let running = task::spawn({
            let checker = checker.clone();
            async move { checker.check_and_update().await }
        });

        wait_for_tip_request(&arrived).await?;
        // Hold a read lock until the check collects its diagnostics and queues its write.
        let cached = checker.cached_state.read().await;
        release.store(true, Ordering::SeqCst);
        time::timeout(STUB_TIMEOUT, async {
            while checker.cached_state.try_read().is_ok() {
                task::yield_now().await;
            }
        })
        .await?;
        indexer_state.mark_verification_failed();
        drop(cached);

        let report = running.await?;

        tip.assert();
        assert_eq!(report.state, ReadinessState::NotReady);
        assert_eq!(report.health_status, metrics::HEALTH_STATUS_VERIFICATION_FAILED);
        assert_eq!(report.checks.verification.status, "unhealthy");
        assert_eq!(updates.try_recv()?, report.state);
        assert_eq!(checker.get().await, report.state);

        Ok(())
    }
}
