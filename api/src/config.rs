//! Configuration for the blokli API server

use std::{net::SocketAddr, path::PathBuf};

use blokli_chain_types::ContractAddresses;
use hopr_primitive_types::primitives::Address;
use serde::{Deserialize, Serialize};

/// API server configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiConfig {
    /// Address to bind the server to
    #[serde(default = "default_bind_address")]
    pub bind_address: SocketAddr,

    /// Enable GraphQL playground
    #[serde(default = "default_playground_enabled")]
    pub playground_enabled: bool,

    /// Database URL
    #[serde(default = "default_database_url")]
    pub database_url: String,

    /// TLS configuration
    #[serde(default)]
    pub tls: Option<TlsConfig>,

    /// CORS allowed origins (comma-separated list, or "*" for permissive)
    /// If not specified, defaults to localhost origins only
    #[serde(default = "default_cors_allowed_origins")]
    pub cors_allowed_origins: Vec<String>,

    /// Chain ID for the blockchain network
    #[serde(default = "default_chain_id")]
    pub chain_id: u64,

    /// RPC URL for blockchain queries (required for balance passthrough)
    #[serde(default = "default_rpc_url")]
    pub rpc_url: String,

    /// Contract addresses for HOPR smart contracts
    #[serde(default = "default_contract_addresses")]
    pub contract_addresses: ContractAddresses,
}

/// TLS configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TlsConfig {
    /// Path to TLS certificate file
    pub cert_path: PathBuf,

    /// Path to TLS private key file
    pub key_path: PathBuf,
}

impl Default for ApiConfig {
    fn default() -> Self {
        Self {
            bind_address: default_bind_address(),
            playground_enabled: default_playground_enabled(),
            database_url: default_database_url(),
            tls: None,
            cors_allowed_origins: default_cors_allowed_origins(),
            chain_id: default_chain_id(),
            rpc_url: default_rpc_url(),
            contract_addresses: default_contract_addresses(),
        }
    }
}

fn default_bind_address() -> SocketAddr {
    "127.0.0.1:8080".parse().unwrap()
}

fn default_playground_enabled() -> bool {
    false
}

fn default_database_url() -> String {
    std::env::var("DATABASE_URL").unwrap_or_else(|_| "postgres://user:pw@127.0.0.1/blokli".to_string())
}

fn default_cors_allowed_origins() -> Vec<String> {
    std::env::var("CORS_ALLOWED_ORIGINS")
        .map(|s| s.split(',').map(|s| s.trim().to_string()).collect())
        .unwrap_or_else(|_| {
            vec![
                "http://localhost:8080".to_string(),
                "https://localhost:8080".to_string(),
                "http://127.0.0.1:8080".to_string(),
                "https://127.0.0.1:8080".to_string(),
            ]
        })
}

fn default_chain_id() -> u64 {
    std::env::var("CHAIN_ID")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(100) // Default to Gnosis Chain (chain ID 100)
}

fn default_rpc_url() -> String {
    std::env::var("RPC_URL").unwrap_or_else(|_| "http://localhost:8545".to_string())
}

fn default_contract_addresses() -> ContractAddresses {
    ContractAddresses {
        token: Address::default(),
        channels: Address::default(),
        announcements: Address::default(),
        safe_registry: Address::default(),
        price_oracle: Address::default(),
        win_prob_oracle: Address::default(),
        stake_factory: Address::default(),
    }
}
