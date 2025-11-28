//! Readiness state management for the API server
//!
//! This module provides functionality to track and query the readiness state of the server.
//! The server is considered ready when:
//! 1. Database is connected and has chain info
//! 2. RPC is reachable
//! 3. Indexer lag is within acceptable limits
//!
//! The readiness state is cached and updated periodically via a background task.
//! Out-of-band updates are triggered by:
//! - /readyz endpoint calls (immediate check)
//! - Indexer completion signals (immediate check)
//! - Periodic background task (every `readiness_check_interval`)

use std::sync::Arc;

use blokli_chain_rpc::rpc::RpcOperations;
use blokli_db_entity::codegen::prelude::ChainInfo;
use sea_orm::{DatabaseConnection, EntityTrait};
use tokio::sync::RwLock;
use tracing::{error, info};

use crate::config::HealthConfig;

/// Readiness state of the API server
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReadinessState {
    /// Server is ready to accept GraphQL requests
    Ready,
    /// Server is not ready (usually during initial indexing)
    NotReady,
}

/// Shared readiness state tracker with caching and periodic updates
#[derive(Clone)]
pub struct ReadinessChecker {
    cached_state: Arc<RwLock<ReadinessState>>,
    db: DatabaseConnection,
    rpc_operations: Arc<RpcOperations<blokli_chain_rpc::transport::ReqwestClient>>,
    health_config: HealthConfig,
}

impl ReadinessChecker {
    /// Create a new readiness checker
    pub fn new(
        db: DatabaseConnection,
        rpc_operations: Arc<RpcOperations<blokli_chain_rpc::transport::ReqwestClient>>,
        health_config: HealthConfig,
    ) -> Self {
        Self {
            cached_state: Arc::new(RwLock::new(ReadinessState::NotReady)),
            db,
            rpc_operations,
            health_config,
        }
    }

    /// Get the current cached readiness state (fast, non-blocking)
    pub async fn get(&self) -> ReadinessState {
        *self.cached_state.read().await
    }

    /// Perform a full readiness check and update the cached state
    /// This is used by /readyz endpoint and indexer completion signals
    pub async fn check_and_update(&self) {
        let new_state = self.check_readiness().await;
        let mut cached = self.cached_state.write().await;
        let old_state = *cached;
        *cached = new_state;

        // Log state transitions
        if old_state != new_state {
            match new_state {
                ReadinessState::Ready => {
                    info!("Readiness state transitioned to READY");
                }
                ReadinessState::NotReady => {
                    info!("Readiness state transitioned to NOT_READY");
                }
            }
        }
    }

    /// Trigger an out-of-band readiness check (used by indexer completion signal)
    pub async fn trigger_update(&self) {
        self.check_and_update().await;
    }

    /// Start a background task that periodically updates the readiness state
    /// This ensures the cached state is refreshed even if /readyz is not called
    pub fn start_periodic_updates(self) {
        let interval = self.health_config.readiness_check_interval;
        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(interval);
            loop {
                ticker.tick().await;
                self.check_and_update().await;
            }
        });
    }

    /// Check if the server is ready
    async fn check_readiness(&self) -> ReadinessState {
        // 1. Check database connectivity by querying chain_info
        let indexed_block = match ChainInfo::find().one(&self.db).await {
            Ok(Some(info)) => Some(info.last_indexed_block),
            Ok(None) => {
                // No chain info yet - indexer hasn't started
                return ReadinessState::NotReady;
            }
            Err(e) => {
                error!("Database check failed: {}", e);
                return ReadinessState::NotReady;
            }
        };

        // 2. Check RPC connectivity and get current block
        let rpc_block = match self.rpc_operations.get_block_number().await {
            Ok(block) => Some(block),
            Err(e) => {
                error!("RPC check failed: {}", e);
                return ReadinessState::NotReady;
            }
        };

        // 3. Check indexer lag
        if let (Some(indexed), Some(rpc_block)) = (indexed_block, rpc_block) {
            let indexed_u64 = u64::try_from(indexed).unwrap_or(0);
            let lag = rpc_block.saturating_sub(indexed_u64);

            if lag > self.health_config.max_indexer_lag {
                return ReadinessState::NotReady;
            }
        } else {
            return ReadinessState::NotReady;
        }

        ReadinessState::Ready
    }
}
