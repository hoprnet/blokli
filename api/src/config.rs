//! Configuration for the blokli API server

use std::{net::SocketAddr, path::PathBuf, time::Duration};

use blokli_chain_api::fee_recommendation::GasFeePolicy;
use blokli_chain_types::ContractAddresses;
use serde::{Deserialize, Serialize};

/// API server configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ApiConfig {
    /// Version of the running service (bloklid), reported by the `version` query and the health endpoints.
    ///
    /// Not read from configuration files; the embedding binary sets it. Defaults to the blokli-api crate version.
    #[serde(skip, default = "default_app_version")]
    pub app_version: String,

    /// Address to bind the server to
    #[serde(default = "default_bind_address")]
    pub bind_address: SocketAddr,

    /// Enable GraphQL playground
    #[serde(default = "default_playground_enabled")]
    pub playground_enabled: bool,

    /// Database URL
    #[serde(default = "default_database_url")]
    pub database_url: String,

    /// Maximum number of connections in this process's database pool
    #[serde(default = "default_max_connections")]
    pub max_connections: u32,

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

    /// Expected block time in seconds
    #[serde(default = "default_expected_block_time")]
    pub expected_block_time: u64,

    /// Multiplier applied to EIP-1559 gas estimates exposed by chainInfo
    #[serde(default = "default_gas_multiplier")]
    pub gas_multiplier: f64,

    /// Minimum `maxPriorityFeePerGas` exposed by chainInfo, in wei
    #[serde(default = "default_min_priority_fee_per_gas")]
    pub min_priority_fee_per_gas: u64,

    /// Maximum `maxPriorityFeePerGas` exposed by chainInfo, in wei
    #[serde(default = "default_max_priority_fee_per_gas")]
    pub max_priority_fee_per_gas: u64,

    /// Minimum `maxFeePerGas` exposed by chainInfo, in wei
    #[serde(default = "default_min_max_fee_per_gas")]
    pub min_max_fee_per_gas: u64,

    /// Maximum `maxFeePerGas` exposed by chainInfo, in wei
    #[serde(default = "default_max_max_fee_per_gas")]
    pub max_max_fee_per_gas: u64,

    /// Factor applied to the next-block base fee when computing `maxFeePerGas`
    #[serde(default = "default_base_fee_headroom_factor")]
    pub base_fee_headroom_factor: f64,

    #[serde(default)]
    pub sse_keepalive: SseKeepAliveConfig,

    /// Health check configuration
    #[serde(default)]
    pub health: HealthConfig,

    /// Maximum GraphQL query nesting depth
    #[serde(default = "default_max_query_depth")]
    pub max_query_depth: usize,

    /// Maximum GraphQL query complexity budget
    #[serde(default = "default_max_query_complexity")]
    pub max_query_complexity: usize,

    /// Maximum number of subscription snapshots loaded from the database at the same time
    #[serde(default = "default_max_concurrent_snapshots")]
    pub max_concurrent_snapshots: usize,

    /// How long a subscription waits for its turn to load a snapshot before it is ended
    #[serde(default = "default_snapshot_queue_timeout", with = "humantime_serde")]
    pub snapshot_queue_timeout: Duration,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SseKeepAliveConfig {
    #[serde(default = "default_sse_keepalive_enabled")]
    pub enabled: bool,

    #[serde(default = "default_sse_keepalive_interval", with = "humantime_serde")]
    pub interval: Duration,
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

impl Default for SseKeepAliveConfig {
    fn default() -> Self {
        Self {
            enabled: default_sse_keepalive_enabled(),
            interval: default_sse_keepalive_interval(),
        }
    }
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

fn default_sse_keepalive_enabled() -> bool {
    true
}

fn default_sse_keepalive_interval() -> Duration {
    Duration::from_secs(15)
}

fn default_max_query_depth() -> usize {
    8
}

fn default_max_query_complexity() -> usize {
    500
}

fn default_max_concurrent_snapshots() -> usize {
    8
}

fn default_snapshot_queue_timeout() -> Duration {
    Duration::from_secs(10)
}

fn default_max_indexer_lag() -> u64 {
    10
}

fn default_health_timeout() -> Duration {
    Duration::from_millis(5000)
}

fn default_app_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

fn default_readiness_check_interval() -> Duration {
    Duration::from_secs(60)
}

impl Default for ApiConfig {
    fn default() -> Self {
        Self {
            app_version: default_app_version(),
            bind_address: default_bind_address(),
            playground_enabled: default_playground_enabled(),
            database_url: default_database_url(),
            max_connections: default_max_connections(),
            tls: None,
            cors_allowed_origins: default_cors_allowed_origins(),
            chain_id: default_chain_id(),
            rpc_url: default_rpc_url(),
            contract_addresses: default_contract_addresses(),
            expected_block_time: default_expected_block_time(),
            gas_multiplier: default_gas_multiplier(),
            min_priority_fee_per_gas: default_min_priority_fee_per_gas(),
            max_priority_fee_per_gas: default_max_priority_fee_per_gas(),
            min_max_fee_per_gas: default_min_max_fee_per_gas(),
            max_max_fee_per_gas: default_max_max_fee_per_gas(),
            base_fee_headroom_factor: default_base_fee_headroom_factor(),
            sse_keepalive: SseKeepAliveConfig::default(),
            health: HealthConfig::default(),
            max_query_depth: default_max_query_depth(),
            max_query_complexity: default_max_query_complexity(),
            max_concurrent_snapshots: default_max_concurrent_snapshots(),
            snapshot_queue_timeout: default_snapshot_queue_timeout(),
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

fn default_max_connections() -> u32 {
    std::env::var("BLOKLI_DATABASE_MAX_CONNECTIONS")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(10)
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

fn default_expected_block_time() -> u64 {
    5
}

fn default_gas_multiplier() -> f64 {
    1.0
}

/// 0.01 gwei: Gnosis median tips are around 0.1 gwei but idle blocks often carry none, so a small floor keeps
/// transactions attractive to block builders while a 300k gas transaction pays at most 0.000003 xDAI of tip.
pub const DEFAULT_MIN_PRIORITY_FEE_PER_GAS: u64 = 10_000_000;

/// 1 gwei: Gnosis market tips reach 0.6-0.76 gwei at the median when blocks fill up, so a lower cap leaves the
/// transaction behind them. The tip is paid only on gas used: a 300k gas transaction pays at most 0.0003 xDAI of it.
pub const DEFAULT_MAX_PRIORITY_FEE_PER_GAS: u64 = 1_000_000_000;

/// 0.5 gwei: the Gnosis base fee idles at 9-20 wei but peaked at 0.23 gwei in 24 hours of blocks, and it can grow
/// about 14x within two minutes. A 5_000_000 wei floor was exceeded ~8% of the day. Nodes pay base fee plus tip,
/// not this cap; it only raises the balance reserved upfront (0.005 xDAI at a 10M gas limit).
pub const DEFAULT_MIN_MAX_FEE_PER_GAS: u64 = 500_000_000;

/// 1 gwei: 4x the largest base fee seen in a 24h Gnosis sample (0.23 gwei). It bounds the balance reserved upfront
/// (gas limit x max fee) to 0.01 xDAI at a 10M gas limit.
pub const DEFAULT_MAX_MAX_FEE_PER_GAS: u64 = 1_000_000_000;

/// 8x: the base fee grows at most 12.5% per block, so 8x covers about 17 blocks of worst-case growth, where the
/// default alloy estimate of 2x only covers about 6.
pub const DEFAULT_BASE_FEE_HEADROOM_FACTOR: f64 = 8.0;

fn default_min_priority_fee_per_gas() -> u64 {
    DEFAULT_MIN_PRIORITY_FEE_PER_GAS
}

fn default_max_priority_fee_per_gas() -> u64 {
    DEFAULT_MAX_PRIORITY_FEE_PER_GAS
}

fn default_min_max_fee_per_gas() -> u64 {
    DEFAULT_MIN_MAX_FEE_PER_GAS
}

fn default_max_max_fee_per_gas() -> u64 {
    DEFAULT_MAX_MAX_FEE_PER_GAS
}

fn default_base_fee_headroom_factor() -> f64 {
    DEFAULT_BASE_FEE_HEADROOM_FACTOR
}

impl ApiConfig {
    /// Fee policy (floors, tip cap, max fee ceiling, headroom) applied to the chainInfo EIP-1559 estimates
    pub fn gas_fee_policy(&self) -> GasFeePolicy {
        GasFeePolicy {
            min_priority_fee_per_gas: u128::from(self.min_priority_fee_per_gas),
            max_priority_fee_per_gas: u128::from(self.max_priority_fee_per_gas),
            min_max_fee_per_gas: u128::from(self.min_max_fee_per_gas),
            max_max_fee_per_gas: u128::from(self.max_max_fee_per_gas),
            base_fee_headroom_factor: self.base_fee_headroom_factor,
        }
    }

    /// Check that the gas multiplier, the headroom factor and the fee bounds are consistent.
    pub fn validate_fee_settings(&self) -> Result<(), String> {
        validate_fee_settings(self.gas_multiplier, self.gas_fee_policy())
    }
}

/// Check that the gas multiplier, the headroom factor and the fee bounds are consistent.
///
/// Shared by the daemon and the standalone server so both reject the same configurations.
pub fn validate_fee_settings(gas_multiplier: f64, policy: GasFeePolicy) -> Result<(), String> {
    if !gas_multiplier.is_finite() || gas_multiplier < 1.0 {
        return Err("api.gas_multiplier must be a finite number greater than or equal to 1".to_string());
    }

    if !policy.base_fee_headroom_factor.is_finite() || policy.base_fee_headroom_factor < 1.0 {
        return Err("api.base_fee_headroom_factor must be a finite number greater than or equal to 1".to_string());
    }

    if policy.max_priority_fee_per_gas < policy.min_priority_fee_per_gas {
        return Err(
            "api.max_priority_fee_per_gas must be greater than or equal to api.min_priority_fee_per_gas".to_string(),
        );
    }

    if policy.max_max_fee_per_gas < policy.min_max_fee_per_gas {
        return Err("api.max_max_fee_per_gas must be greater than or equal to api.min_max_fee_per_gas".to_string());
    }

    if policy.max_max_fee_per_gas < policy.max_priority_fee_per_gas {
        return Err(
            "api.max_max_fee_per_gas must be greater than or equal to api.max_priority_fee_per_gas".to_string(),
        );
    }

    if policy.min_max_fee_per_gas < policy.min_priority_fee_per_gas {
        return Err(
            "api.min_max_fee_per_gas must be greater than or equal to api.min_priority_fee_per_gas".to_string(),
        );
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sse_keepalive_defaults() {
        let config = ApiConfig::default();
        assert!(config.sse_keepalive.enabled);
        assert_eq!(config.sse_keepalive.interval, Duration::from_secs(15));
        assert_eq!(config.gas_multiplier, 1.0);
        assert_eq!(
            config.gas_fee_policy(),
            GasFeePolicy {
                min_priority_fee_per_gas: 10_000_000,
                max_priority_fee_per_gas: 1_000_000_000,
                min_max_fee_per_gas: 500_000_000,
                max_max_fee_per_gas: 1_000_000_000,
                base_fee_headroom_factor: 8.0,
            }
        );
    }

    #[test]
    fn test_default_max_priority_fee_is_one_gwei_and_fits_the_ceiling() {
        assert_eq!(DEFAULT_MAX_PRIORITY_FEE_PER_GAS, 1_000_000_000);
        let config = ApiConfig::default();
        assert_eq!(config.max_priority_fee_per_gas, 1_000_000_000);
        // The tip cap may equal the max fee ceiling.
        assert_eq!(config.max_priority_fee_per_gas, config.max_max_fee_per_gas);
    }

    #[test]
    fn test_default_fee_settings_are_valid() {
        assert_eq!(ApiConfig::default().validate_fee_settings(), Ok(()));
    }

    #[test]
    fn test_inconsistent_fee_settings_are_rejected() {
        let invalid = |edit: fn(&mut ApiConfig)| {
            let mut config = ApiConfig::default();
            edit(&mut config);
            config.validate_fee_settings().expect_err("settings should be rejected")
        };

        assert!(invalid(|c| c.gas_multiplier = 0.5).contains("api.gas_multiplier"));
        assert!(invalid(|c| c.base_fee_headroom_factor = f64::NAN).contains("api.base_fee_headroom_factor"));
        assert!(
            invalid(|c| {
                c.min_priority_fee_per_gas = 100;
                c.max_priority_fee_per_gas = 50;
            })
            .contains("api.max_priority_fee_per_gas")
        );
        assert!(
            invalid(|c| {
                c.min_max_fee_per_gas = 2_000;
                c.max_max_fee_per_gas = 1_000;
                c.max_priority_fee_per_gas = 1;
                c.min_priority_fee_per_gas = 1;
            })
            .contains("api.max_max_fee_per_gas must be greater than or equal to api.min_max_fee_per_gas")
        );
        // The standalone case from review: ceiling below the tip cap
        assert!(
            invalid(|c| {
                c.min_max_fee_per_gas = 1;
                c.max_max_fee_per_gas = 20_000_000;
                c.max_priority_fee_per_gas = 50_000_000;
            })
            .contains("api.max_max_fee_per_gas must be greater than or equal to api.max_priority_fee_per_gas")
        );
        assert!(
            invalid(|c| {
                c.min_max_fee_per_gas = 5;
                c.min_priority_fee_per_gas = 10;
            })
            .contains("api.min_max_fee_per_gas must be greater than or equal to api.min_priority_fee_per_gas")
        );
    }

    #[test]
    fn test_graphql_limits_defaults() {
        let config = ApiConfig::default();
        assert_eq!(config.max_query_depth, 8);
        assert_eq!(config.max_query_complexity, 500);
    }

    #[test]
    fn test_snapshot_limits_defaults() {
        let config = ApiConfig::default();
        assert_eq!(config.max_concurrent_snapshots, 8);
        assert_eq!(config.snapshot_queue_timeout, Duration::from_secs(10));
    }

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
    fn test_max_connections_defaults_when_env_missing() {
        temp_env::with_var("BLOKLI_DATABASE_MAX_CONNECTIONS", None::<&str>, || {
            assert_eq!(default_max_connections(), 10);
        });
    }

    #[test]
    fn test_max_connections_from_env_var() {
        temp_env::with_var("BLOKLI_DATABASE_MAX_CONNECTIONS", Some("25"), || {
            assert_eq!(default_max_connections(), 25);
        });
    }

    #[test]
    fn test_max_connections_defaults_when_env_invalid() {
        temp_env::with_var("BLOKLI_DATABASE_MAX_CONNECTIONS", Some("not-a-number"), || {
            assert_eq!(default_max_connections(), 10);
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
