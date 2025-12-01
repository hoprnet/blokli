//! Configuration for the blokli API server

use std::{net::SocketAddr, path::PathBuf, time::Duration};

use blokli_chain_types::ContractAddresses;
use serde::{Deserialize, Serialize};

/// API server configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
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

    /// Health check configuration
    #[serde(default)]
    pub health: HealthConfig,
}

/// TLS configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TlsConfig {
    /// Path to TLS certificate file
    pub cert_path: PathBuf,

    /// Path to TLS private key file
    pub key_path: PathBuf,
}

/// Health check configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HealthConfig {
    /// Maximum allowed indexer lag (in blocks) before readiness check fails
    #[serde(default = "default_max_indexer_lag")]
    pub max_indexer_lag: u64,

    /// Timeout for health check queries
    #[serde(default = "default_health_timeout", with = "humantime_serde")]
    pub timeout: Duration,

    /// Interval for periodic readiness checks
    #[serde(default = "default_readiness_check_interval", with = "humantime_serde")]
    pub readiness_check_interval: Duration,
}

impl Default for HealthConfig {
    fn default() -> Self {
        Self {
            max_indexer_lag: default_max_indexer_lag(),
            timeout: default_health_timeout(),
            readiness_check_interval: default_readiness_check_interval(),
        }
    }
}

fn default_max_indexer_lag() -> u64 {
    10
}

fn default_health_timeout() -> Duration {
    Duration::from_millis(5000)
}

fn default_readiness_check_interval() -> Duration {
    Duration::from_secs(60)
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
            health: HealthConfig::default(),
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
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(100) // Default to Gnosis Chain (chain ID 100)
}

fn default_rpc_url() -> String {
    std::env::var("RPC_URL").unwrap_or_else(|_| "http://localhost:8545".to_string())
}

fn default_contract_addresses() -> ContractAddresses {
    ContractAddresses::default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_chain_id_from_valid_env_var() {
        temp_env::with_var("CHAIN_ID", Some("137"), || {
            assert_eq!(default_chain_id(), 137);
        });
    }

    #[test]
    fn test_chain_id_from_zero_env_var() {
        temp_env::with_var("CHAIN_ID", Some("0"), || {
            assert_eq!(default_chain_id(), 0);
        });
    }

    #[test]
    fn test_chain_id_from_large_env_var() {
        temp_env::with_var("CHAIN_ID", Some("18446744073709551615"), || {
            assert_eq!(default_chain_id(), 18446744073709551615);
        });
    }

    #[test]
    fn test_chain_id_defaults_when_env_missing() {
        temp_env::with_var("CHAIN_ID", None::<&str>, || {
            assert_eq!(default_chain_id(), 100);
        });
    }

    #[test]
    fn test_chain_id_defaults_when_env_invalid() {
        // Invalid chain ID (not a number)
        temp_env::with_var("CHAIN_ID", Some("invalid"), || {
            assert_eq!(default_chain_id(), 100);
        });

        // Invalid chain ID (negative number)
        temp_env::with_var("CHAIN_ID", Some("-1"), || {
            assert_eq!(default_chain_id(), 100);
        });

        // Invalid chain ID (overflow)
        temp_env::with_var("CHAIN_ID", Some("18446744073709551616"), || {
            assert_eq!(default_chain_id(), 100);
        });

        // Invalid chain ID (float)
        temp_env::with_var("CHAIN_ID", Some("137.5"), || {
            assert_eq!(default_chain_id(), 100);
        });
    }

    #[test]
    fn test_database_url_from_env_var() {
        temp_env::with_var("DATABASE_URL", Some("postgres://custom:pass@host/db"), || {
            assert_eq!(default_database_url(), "postgres://custom:pass@host/db");
        });
    }

    #[test]
    fn test_database_url_defaults_when_env_missing() {
        temp_env::with_var("DATABASE_URL", None::<&str>, || {
            assert_eq!(default_database_url(), "postgres://user:pw@127.0.0.1/blokli");
        });
    }

    #[test]
    fn test_cors_allowed_origins_from_env_var() {
        temp_env::with_var(
            "CORS_ALLOWED_ORIGINS",
            Some("https://example.com, https://api.example.com"),
            || {
                let origins = default_cors_allowed_origins();
                assert_eq!(origins.len(), 2);
                assert_eq!(origins[0], "https://example.com");
                assert_eq!(origins[1], "https://api.example.com");
            },
        );
    }

    #[test]
    fn test_cors_allowed_origins_single_entry() {
        temp_env::with_var("CORS_ALLOWED_ORIGINS", Some("https://example.com"), || {
            let origins = default_cors_allowed_origins();
            assert_eq!(origins.len(), 1);
            assert_eq!(origins[0], "https://example.com");
        });
    }

    #[test]
    fn test_cors_allowed_origins_with_whitespace_trimmed() {
        temp_env::with_var(
            "CORS_ALLOWED_ORIGINS",
            Some("  https://example.com  ,  https://api.example.com  "),
            || {
                let origins = default_cors_allowed_origins();
                assert_eq!(origins.len(), 2);
                assert_eq!(origins[0], "https://example.com");
                assert_eq!(origins[1], "https://api.example.com");
            },
        );
    }

    #[test]
    fn test_cors_allowed_origins_defaults_when_env_missing() {
        temp_env::with_var("CORS_ALLOWED_ORIGINS", None::<&str>, || {
            let origins = default_cors_allowed_origins();
            assert_eq!(origins.len(), 4);
            assert!(origins.contains(&"http://localhost:8080".to_string()));
            assert!(origins.contains(&"https://localhost:8080".to_string()));
            assert!(origins.contains(&"http://127.0.0.1:8080".to_string()));
            assert!(origins.contains(&"https://127.0.0.1:8080".to_string()));
        });
    }

    #[test]
    fn test_rpc_url_from_env_var() {
        temp_env::with_var("RPC_URL", Some("https://rpc.example.com"), || {
            assert_eq!(default_rpc_url(), "https://rpc.example.com");
        });
    }

    #[test]
    fn test_rpc_url_defaults_when_env_missing() {
        temp_env::with_var("RPC_URL", None::<&str>, || {
            assert_eq!(default_rpc_url(), "http://localhost:8545");
        });
    }

    #[test]
    fn test_readiness_check_interval_defaults() {
        assert_eq!(default_readiness_check_interval(), Duration::from_secs(60));
    }
}
