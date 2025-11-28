//! Readiness state management for the API server
//!
//! This module provides functionality to track and query the readiness state of the server.
//! The server is considered ready when:
//! 1. Database is connected and has chain info
//! 2. RPC is reachable
//! 3. Indexer lag is within acceptable limits

use std::sync::Arc;

use blokli_chain_rpc::rpc::RpcOperations;
use blokli_db_entity::codegen::prelude::ChainInfo;
use sea_orm::{DatabaseConnection, EntityTrait};
use tracing::error;

use crate::config::HealthConfig;

/// Readiness state of the API server
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReadinessState {
    /// Server is ready to accept GraphQL requests
    Ready,
    /// Server is not ready (usually during initial indexing)
    NotReady,
}

/// Shared readiness state tracker
#[derive(Clone)]
pub struct ReadinessChecker {
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
            db,
            rpc_operations,
            health_config,
        }
    }

    /// Get the current readiness state (checks current status)
    pub async fn get(&self) -> ReadinessState {
        self.check_readiness().await
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_readiness_state_equality() {
        assert_eq!(ReadinessState::Ready, ReadinessState::Ready);
        assert_eq!(ReadinessState::NotReady, ReadinessState::NotReady);
        assert_ne!(ReadinessState::Ready, ReadinessState::NotReady);
    }

    #[test]
    fn test_readiness_state_copy_clone() {
        let state = ReadinessState::Ready;
        let state_copy = state;
        let state_clone = state.clone();

        assert_eq!(state, state_copy);
        assert_eq!(state, state_clone);
    }
}
