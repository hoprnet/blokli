//! Readiness state management for the API server
//!
//! This module provides functionality to track and query the readiness state of the server.
//! The server is considered ready when:
//! 1. Database is connected and has chain info
//! 2. RPC is reachable
//! 3. Indexer lag is within acceptable limits
//! 4. Post-finality verification has not found a mismatch
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

/// Result of one readiness evaluation.
///
/// The report gathers every check even when an earlier one already decided the state, so the
/// `/readyz` body keeps its diagnostic details when verification failed or the database is
/// unreachable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReadinessReport {
    /// Readiness decision that feeds the cached state, the HTTP status and the health metric.
    pub state: ReadinessState,
    /// Health status label for the `blokli_health` metric.
    pub health_status: &'static str,
    /// Database check details.
    pub database: DatabaseCheck,
    /// RPC check details.
    pub rpc: RpcCheck,
    /// Indexer lag check details.
    pub indexer: IndexerCheck,
    /// Post-finality verification check details.
    pub verification: VerificationCheck,
}

/// Database check details reported by `/readyz`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DatabaseCheck {
    /// Last indexed block, when chain info exists.
    pub last_indexed_block: Option<i64>,
    /// Failure reason, when the database could not provide chain info.
    pub error: Option<String>,
}

/// RPC check details reported by `/readyz`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RpcCheck {
    /// Confirmed block reported by the chain tip tracker.
    pub block_number: Option<u64>,
    /// Failure reason, when the confirmed block could not be read.
    pub error: Option<String>,
}

/// Indexer lag check details reported by `/readyz`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct IndexerCheck {
    /// Last indexed block, when chain info exists.
    pub last_indexed_block: Option<i64>,
    /// Lag between the confirmed block and the indexer watermark.
    pub lag: Option<u64>,
    /// Whether the lag exceeds the configured threshold.
    pub lag_exceeded: bool,
}

/// Post-finality verification check details reported by `/readyz`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct VerificationCheck {
    /// Whether the verification task found a mismatch.
    pub failed: bool,
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

    /// Attaches the post-finality verification status checked for readiness.
    pub fn with_verification_status(mut self, verification_status: VerificationStatus) -> Self {
        self.verification_status = verification_status;
        self
    }

    /// Get the current cached readiness state (fast, non-blocking)
    pub async fn get(&self) -> ReadinessState {
        *self.cached_state.read().await
    }

    pub fn subscribe(&self) -> Receiver<ReadinessState> {
        self.updates.new_receiver()
    }

    /// Perform a full readiness check and update the cached state
    ///
    /// This is used by /readyz endpoint and indexer completion signals. A post-finality
    /// verification mismatch is published before the database and RPC reads, so a cached `Ready`
    /// state is replaced even while a check still waits for them, and the flag is read again when
    /// the result is stored so a check that started before the mismatch cannot restore readiness.
    pub async fn check_and_update(&self) -> ReadinessReport {
        if self.verification_status.failed() {
            self.store_state(ReadinessState::NotReady, metrics::HEALTH_STATUS_VERIFICATION_FAILED)
                .await;
        }

        let mut report = self.check_readiness().await;

        if self.verification_status.failed() {
            report.state = ReadinessState::NotReady;
            report.health_status = metrics::HEALTH_STATUS_VERIFICATION_FAILED;
            report.verification.failed = true;
        }

        self.store_state(report.state, report.health_status).await;

        report
    }

    /// Stores a readiness decision, updates the health metric and notifies subscribers when the
    /// state changes.
    async fn store_state(&self, new_state: ReadinessState, health_status: &'static str) {
        let mut cached = self.cached_state.write().await;
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

    /// Start a background task that periodically updates the readiness state
    /// This ensures the cached state is refreshed even if /readyz is not called
    pub fn start_periodic_updates(self) {
        let interval = self.health_config.readiness_check_interval;
        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(interval);
            loop {
                ticker.tick().await;
                let _ = self.check_and_update().await;
            }
        });
    }

    /// Check if the server is ready
    ///
    /// The returned report holds the readiness decision and the details of every individual check.
    /// All checks run even when an earlier one already failed, so the `/readyz` body keeps its
    /// diagnostic details. A post-finality verification mismatch wins over the other checks for
    /// the health status label.
    async fn check_readiness(&self) -> ReadinessReport {
        let verification_failed = self.verification_status.failed();

        // A post-finality verification mismatch means the indexed data can no longer be trusted,
        // so readiness stays failed until an operator restarts the daemon. Checking it first makes
        // it win over the other conditions; the transition log reports the change once.
        if verification_failed {
            debug!("Post-finality verification found a mismatch, indexed data is no longer trusted");
        }

        // 1. Check database connectivity by querying chain_info
        let mut database = DatabaseCheck::default();
        let mut database_health_status = None;
        match ChainInfo::find().one(&self.db).await {
            Ok(Some(info)) => database.last_indexed_block = Some(info.last_indexed_block),
            Ok(None) => {
                // No chain info yet - indexer hasn't started
                database.error = Some("No chain info found - indexer may not have started".to_string());
                database_health_status = Some(metrics::HEALTH_STATUS_UNSYNCHED);
            }
            Err(e) => {
                error!("Database check failed: {}", e);
                database.error = Some(e.to_string());
                database_health_status = Some(metrics::HEALTH_STATUS_CORRUPTED);
            }
        }

        // 2. Check RPC connectivity and get the tracker's confirmed tip
        // NOTE: get_block_number() returns the chain tip tracker's confirmed block
        // This ensures we only compare lag against confirmed blocks, not unconfirmed chain head
        let mut rpc = RpcCheck::default();
        match self.rpc_operations.get_block_number().await {
            Ok(block) => rpc.block_number = Some(block),
            Err(e) => {
                error!("RPC check failed: {}", e);
                rpc.error = Some(e.to_string());
            }
        }

        // 3. Check indexer lag
        let mut indexer = IndexerCheck::default();
        if let (Some(indexed), Some(rpc_block)) = (database.last_indexed_block, rpc.block_number) {
            let indexed_u64 = u64::try_from(indexed).unwrap_or(0);
            let lag = rpc_block.saturating_sub(indexed_u64);
            indexer.last_indexed_block = Some(indexed);
            indexer.lag = Some(lag);

            if lag > self.health_config.max_indexer_lag {
                warn!(
                    %lag,
                    confirmed_block = %rpc_block,
                    %indexed,
                    max_lag = self.health_config.max_indexer_lag,
                    "Indexer lag exceeds threshold (lag compared against the tracker's confirmed tip)"
                );
                indexer.lag_exceeded = true;
            } else {
                // Log successful readiness check
                debug!(
                    %lag,
                    confirmed_block = %rpc_block,
                    indexed = %indexed,
                    "Indexer within acceptable lag of confirmed blocks"
                );
            }
        } else if let Some(indexed) = database.last_indexed_block {
            // We have indexed block but no RPC block - the RPC check already failed
            indexer.last_indexed_block = Some(indexed);
        }

        let health_status = if verification_failed {
            metrics::HEALTH_STATUS_VERIFICATION_FAILED
        } else if let Some(status) = database_health_status {
            status
        } else if rpc.error.is_some() {
            metrics::HEALTH_STATUS_TIMEOUT
        } else if indexer.lag_exceeded || indexer.lag.is_none() {
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
            database,
            rpc,
            indexer,
            verification: VerificationCheck {
                failed: verification_failed,
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
        time::{Duration, Instant},
    };

    use blokli_chain_indexer::IndexerState;
    use blokli_chain_rpc::{
        rpc::{RpcOperations, RpcOperationsConfig},
        tip::ConfirmationMode,
        transport::ReqwestClient,
    };
    use blokli_db::{BlokliDbGeneralModelOperations, TargetDb, db::BlokliDb};
    use hopr_bindings::exports::alloy::{rpc::client::ClientBuilder, transports::http::ReqwestTransport};

    use super::{ReadinessChecker, ReadinessState};
    use crate::{config::HealthConfig, metrics};

    /// Answers a batched block request with a header for every tag that has a block number.
    fn tip_batch_response(
        request: &mockito::Request,
        latest: u64,
        safe: Option<u64>,
        finalized: Option<u64>,
    ) -> Vec<u8> {
        let body: serde_json::Value =
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
                    serde_json::Value::Null,
                    |number| serde_json::json!({"number": format!("0x{number:x}"), "timestamp": "0x1"}),
                );
                serde_json::json!({
                    "jsonrpc": "2.0",
                    "id": call["id"].clone(),
                    "result": result
                })
            })
            .collect::<Vec<_>>();

        serde_json::to_vec(&responses).expect("batch response should serialize")
    }

    /// Answers the chain tip batch with a latest header and no safe or finalized tag.
    fn mock_tip_without_confirmation_tags(server: &mut mockito::ServerGuard) -> mockito::Mock {
        server
            .mock("POST", "/")
            .match_body(mockito::Matcher::Regex(
                r#""method"\s*:\s*"eth_getBlockByNumber""#.to_string(),
            ))
            .with_body_from_request(|request| tip_batch_response(request, 100, None, None))
            .expect_at_least(1)
            .create()
    }

    /// Answers the chain tip batch once the test sets `release`, and reports the arrived request.
    fn mock_tip_released_by_test(
        server: &mut mockito::ServerGuard,
        arrived: Arc<AtomicBool>,
        release: Arc<AtomicBool>,
    ) -> mockito::Mock {
        server
            .mock("POST", "/")
            .match_body(mockito::Matcher::Regex(
                r#""method"\s*:\s*"eth_getBlockByNumber""#.to_string(),
            ))
            .with_body_from_request(move |request| {
                arrived.store(true, Ordering::SeqCst);

                let deadline = Instant::now() + Duration::from_secs(10);
                while !release.load(Ordering::SeqCst) && Instant::now() < deadline {
                    std::thread::sleep(Duration::from_millis(1));
                }

                tip_batch_response(request, 100, None, None)
            })
            .expect_at_least(1)
            .create()
    }

    fn rpc_operations_for(
        server: &mockito::ServerGuard,
        config: RpcOperationsConfig,
    ) -> anyhow::Result<Arc<RpcOperations<ReqwestClient>>> {
        let transport = ReqwestTransport::new(server.url().parse()?);
        let rpc_client = ClientBuilder::default().transport(transport.clone(), transport.guess_local());

        Ok(Arc::new(RpcOperations::new(
            rpc_client,
            ReqwestClient::new(),
            config,
            None,
        )?))
    }

    async fn wait_for_tip_request(arrived: &AtomicBool) -> anyhow::Result<()> {
        let deadline = Instant::now() + Duration::from_secs(10);

        while !arrived.load(Ordering::SeqCst) {
            if Instant::now() >= deadline {
                anyhow::bail!("the RPC stub did not receive the chain tip request");
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }

        Ok(())
    }

    #[tokio::test]
    async fn missing_safe_tag_reports_not_ready() -> anyhow::Result<()> {
        let mut server = mockito::Server::new_async().await;
        let tip = mock_tip_without_confirmation_tags(&mut server);

        let transport = ReqwestTransport::new(server.url().parse()?);
        let rpc_client = ClientBuilder::default().transport(transport.clone(), transport.guess_local());
        let rpc = Arc::new(
            RpcOperations::new(
                rpc_client,
                ReqwestClient::new(),
                RpcOperationsConfig {
                    confirmation_mode: ConfirmationMode::Safe,
                    ..RpcOperationsConfig::default()
                },
                None,
            )
            .expect("RPC operations should be constructible"),
        );

        let db = BlokliDb::new_in_memory().await?;
        let checker = ReadinessChecker::new(db.conn(TargetDb::Index).clone(), rpc, HealthConfig::default());

        let report = checker.check_readiness().await;

        tip.assert();
        assert_eq!(report.state, ReadinessState::NotReady);
        assert!(
            report.rpc.error.as_deref().is_some_and(|error| error.contains("safe")),
            "the RPC check must report the missing safe tag: {:?}",
            report.rpc.error
        );
        assert!(
            report.indexer.lag.is_none(),
            "a confirmed block of zero must not produce a lag of zero"
        );

        Ok(())
    }

    #[tokio::test]
    async fn verification_failure_reports_not_ready() -> anyhow::Result<()> {
        let mut server = mockito::Server::new_async().await;
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

        let report = checker.check_readiness().await;

        tip.assert();
        assert_eq!(report.state, ReadinessState::NotReady);
        assert_eq!(report.health_status, metrics::HEALTH_STATUS_VERIFICATION_FAILED);
        assert!(report.verification.failed);
        assert!(
            report.rpc.error.is_none(),
            "the RPC check must still run when verification failed: {:?}",
            report.rpc.error
        );
        assert!(
            report.rpc.block_number.is_some(),
            "the RPC stub must answer the chain tip request"
        );

        Ok(())
    }

    #[tokio::test]
    async fn verification_failure_is_published_before_the_rpc_check_completes() -> anyhow::Result<()> {
        let mut server = mockito::Server::new_async().await;
        let arrived = Arc::new(AtomicBool::new(false));
        let release = Arc::new(AtomicBool::new(false));
        let tip = mock_tip_released_by_test(&mut server, arrived.clone(), release.clone());

        let db = BlokliDb::new_in_memory().await?;
        let indexer_state = IndexerState::default();
        let checker = ReadinessChecker::new(
            db.conn(TargetDb::Index).clone(),
            rpc_operations_for(&server, RpcOperationsConfig::default())?,
            HealthConfig::default(),
        )
        .with_verification_status(indexer_state.verification_status());
        checker.force_ready().await;
        indexer_state.mark_verification_failed();

        let running = tokio::spawn({
            let checker = checker.clone();
            async move { checker.check_and_update().await }
        });

        wait_for_tip_request(&arrived).await?;
        assert_eq!(
            checker.get().await,
            ReadinessState::NotReady,
            "a known verification failure must be published before the database and RPC reads finish"
        );

        release.store(true, Ordering::SeqCst);
        let report = running.await?;

        tip.assert();
        assert_eq!(report.state, ReadinessState::NotReady);
        assert_eq!(report.health_status, metrics::HEALTH_STATUS_VERIFICATION_FAILED);

        Ok(())
    }

    #[tokio::test]
    async fn check_started_before_a_verification_failure_cannot_restore_readiness() -> anyhow::Result<()> {
        let mut server = mockito::Server::new_async().await;
        let arrived = Arc::new(AtomicBool::new(false));
        let release = Arc::new(AtomicBool::new(false));
        let tip = mock_tip_released_by_test(&mut server, arrived.clone(), release.clone());

        let db = BlokliDb::new_in_memory().await?;
        let indexer_state = IndexerState::default();
        let checker = ReadinessChecker::new(
            db.conn(TargetDb::Index).clone(),
            rpc_operations_for(&server, RpcOperationsConfig::default())?,
            HealthConfig::default(),
        )
        .with_verification_status(indexer_state.verification_status());
        checker.force_ready().await;

        let running = tokio::spawn({
            let checker = checker.clone();
            async move { checker.check_and_update().await }
        });

        wait_for_tip_request(&arrived).await?;
        indexer_state.mark_verification_failed();
        release.store(true, Ordering::SeqCst);

        let report = running.await?;

        tip.assert();
        assert_eq!(report.state, ReadinessState::NotReady);
        assert_eq!(report.health_status, metrics::HEALTH_STATUS_VERIFICATION_FAILED);
        assert!(report.verification.failed);
        assert_eq!(
            checker.get().await,
            ReadinessState::NotReady,
            "a check that started before the mismatch must not store Ready"
        );

        Ok(())
    }
}
