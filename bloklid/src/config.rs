use std::time::Duration;

use blokli_chain_types::ContractAddresses;
use hopr_chain_config::{ChainNetworkConfig, ProtocolsConfig};
use serde::Deserialize;

fn default_host() -> std::net::SocketAddr {
    "0.0.0.0:3064".parse().unwrap()
}

fn default_database() -> DatabaseConfig {
    DatabaseConfig::PostgreSql(PostgreSqlConfig::Url(PostgreSqlUrlConfig {
        url: "postgresql://bloklid:password@localhost:5432/bloklid".to_string(),
        max_connections: default_max_connections(),
    }))
}

fn default_rpc_url() -> String {
    "http://localhost:8545".to_string()
}

fn default_data_directory() -> String {
    "data".to_string()
}

fn default_network() -> String {
    "dufour".to_string()
}

/// PostgreSQL database configuration
///
/// Supports two formats:
/// 1. Simple URL: `url = "postgresql://user:pass@host:port/database"`
/// 2. Detailed components with individual fields
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(untagged)]
pub enum PostgreSqlConfig {
    /// Simple connection URL format
    Url(PostgreSqlUrlConfig),
    /// Detailed connection parameters
    Detailed(PostgreSqlDetailedConfig),
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PostgreSqlUrlConfig {
    pub url: String,
    #[serde(default = "default_max_connections")]
    pub max_connections: u32,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PostgreSqlDetailedConfig {
    pub host: String,
    pub port: u16,
    pub username: String,
    pub password: String,
    pub database: String,
    #[serde(default = "default_max_connections")]
    pub max_connections: u32,
}

/// SQLite database configuration
///
/// SQLite uses two separate database files to avoid write lock contention:
/// - Index database: Contains accounts, channels, announcements, node_info, chain_info
/// - Logs database: Contains log, log_status, log_topic_info tables
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SqliteConfig {
    /// Path to the index SQLite database file (accounts, channels, etc.)
    /// Use ":memory:" for in-memory database
    #[serde(default = "default_sqlite_index_path")]
    pub index_path: String,

    /// Path to the logs SQLite database file (log tables)
    /// Use ":memory:" for in-memory database
    #[serde(default = "default_sqlite_logs_path")]
    pub logs_path: String,

    #[serde(default = "default_max_connections")]
    pub max_connections: u32,
}

fn default_sqlite_index_path() -> String {
    "data/bloklid-index.db".to_string()
}

fn default_sqlite_logs_path() -> String {
    "data/bloklid-logs.db".to_string()
}

/// Database configuration supporting both PostgreSQL and SQLite
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum DatabaseConfig {
    #[serde(rename = "postgresql")]
    PostgreSql(PostgreSqlConfig),
    #[serde(rename = "sqlite")]
    Sqlite(SqliteConfig),
}

fn default_max_connections() -> u32 {
    10
}

impl DatabaseConfig {
    /// Convert database configuration to connection URL
    /// For PostgreSQL, returns the single database URL
    /// For SQLite, returns the index database URL
    pub fn to_url(&self) -> String {
        match self {
            DatabaseConfig::PostgreSql(pg_config) => match pg_config {
                PostgreSqlConfig::Url(config) => config.url.clone(),
                PostgreSqlConfig::Detailed(config) => {
                    format!(
                        "postgresql://{}:{}@{}:{}/{}",
                        config.username, config.password, config.host, config.port, config.database
                    )
                }
            },
            DatabaseConfig::Sqlite(sqlite_config) => {
                format!("sqlite://{}?mode=rwc", sqlite_config.index_path)
            }
        }
    }

    /// Get logs database URL (only applicable for SQLite)
    /// For PostgreSQL, returns None as it uses a single database
    /// For SQLite, returns the logs database URL
    pub fn to_logs_url(&self) -> Option<String> {
        match self {
            DatabaseConfig::PostgreSql(_) => None,
            DatabaseConfig::Sqlite(sqlite_config) => Some(format!("sqlite://{}?mode=rwc", sqlite_config.logs_path)),
        }
    }

    /// Get max_connections setting
    pub fn max_connections(&self) -> u32 {
        match self {
            DatabaseConfig::PostgreSql(pg_config) => match pg_config {
                PostgreSqlConfig::Url(config) => config.max_connections,
                PostgreSqlConfig::Detailed(config) => config.max_connections,
            },
            DatabaseConfig::Sqlite(sqlite_config) => sqlite_config.max_connections,
        }
    }
}

#[serde_with::serde_as]
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, smart_default::SmartDefault, validator::Validate)]
#[serde(deny_unknown_fields)]
pub struct Config {
    #[serde_as(as = "serde_with::DisplayFromStr")]
    #[default(_code = "default_host()")]
    #[serde(default = "default_host")]
    pub host: std::net::SocketAddr,

    #[default(_code = "default_database()")]
    #[serde(default = "default_database")]
    pub database: DatabaseConfig,

    #[default(_code = "default_data_directory()")]
    #[serde(default = "default_data_directory")]
    pub data_directory: String,

    #[default(_code = "default_network()")]
    #[serde(default = "default_network")]
    pub network: String,

    #[default(_code = "default_rpc_url()")]
    #[serde(default = "default_rpc_url")]
    pub rpc_url: String,

    #[serde(default)]
    pub max_rpc_requests_per_sec: u32,

    #[serde(default)]
    pub indexer: IndexerConfig,

    #[serde(default)]
    pub api: ApiConfig,

    #[serde(skip)]
    #[default(None)]
    pub chain_network: Option<ChainNetworkConfig>,

    #[serde(skip)]
    #[default(_code = "ContractAddresses::default()")]
    pub contracts: ContractAddresses,

    #[serde(default)]
    pub protocols: ProtocolsConfig,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, smart_default::SmartDefault)]
#[serde(deny_unknown_fields)]
pub struct IndexerConfig {
    #[default(true)]
    #[serde(default = "default_true")]
    pub fast_sync: bool,

    #[default(false)]
    #[serde(default = "default_false")]
    pub enable_logs_snapshot: bool,

    #[serde(default)]
    pub logs_snapshot_url: Option<String>,

    #[serde(default)]
    pub subscription: SubscriptionConfig,
}

/// Configuration for GraphQL subscription behavior
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, smart_default::SmartDefault)]
#[serde(deny_unknown_fields)]
pub struct SubscriptionConfig {
    /// Capacity of the event bus buffer for channel events
    /// Higher values prevent overflow but use more memory
    #[default(1000)]
    #[serde(default = "default_event_bus_capacity")]
    pub event_bus_capacity: usize,

    /// Capacity of the shutdown signal buffer
    /// Typically a small value is sufficient
    #[default(10)]
    #[serde(default = "default_shutdown_signal_capacity")]
    pub shutdown_signal_capacity: usize,

    /// Batch size for Phase 1 historical channel queries
    /// Higher values fetch more data per query but may increase latency
    #[default(100)]
    #[serde(default = "default_batch_size")]
    pub batch_size: usize,
}

#[serde_with::serde_as]
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, smart_default::SmartDefault)]
#[serde(deny_unknown_fields)]
pub struct ApiConfig {
    #[default(true)]
    #[serde(default = "default_true")]
    pub enabled: bool,

    #[serde_as(as = "serde_with::DisplayFromStr")]
    #[default(_code = "default_api_bind_address()")]
    #[serde(default = "default_api_bind_address")]
    pub bind_address: std::net::SocketAddr,

    #[default(true)]
    #[serde(default = "default_true")]
    pub playground_enabled: bool,

    #[serde(default)]
    pub health: HealthConfig,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, smart_default::SmartDefault)]
#[serde(deny_unknown_fields)]
pub struct HealthConfig {
    /// Maximum allowed indexer lag (in blocks) before readiness check fails
    #[default(10)]
    #[serde(default = "default_max_indexer_lag")]
    pub max_indexer_lag: u64,

    /// Timeout for health check queries (in milliseconds)
    #[default(_code = "Duration::from_millis(5000)")]
    #[serde(default = "default_health_timeout", deserialize_with = "deserialize_duration_ms")]
    pub timeout: Duration,
}

fn deserialize_duration_ms<'de, D>(deserializer: D) -> Result<Duration, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let millis = u64::deserialize(deserializer)?;
    Ok(Duration::from_millis(millis))
}

fn default_api_bind_address() -> std::net::SocketAddr {
    "0.0.0.0:8080".parse().unwrap()
}

// Helper functions for serde defaults that respect SmartDefault values
fn default_true() -> bool {
    true
}

fn default_false() -> bool {
    false
}

fn default_event_bus_capacity() -> usize {
    1000
}

fn default_shutdown_signal_capacity() -> usize {
    10
}

fn default_batch_size() -> usize {
    100
}

fn default_max_indexer_lag() -> u64 {
    10
}

fn default_health_timeout() -> Duration {
    Duration::from_millis(5000)
}

#[cfg(test)]
mod tests {
    use std::{fs, path::PathBuf};

    use super::*;

    #[test]
    fn test_strict_parsing() {
        // Test unknown top-level field
        let config = r#"
        host = "0.0.0.0:3064"
        unknown_field = "bad"
        [database]
        type = "sqlite"
        index_path = ":memory:"
        logs_path = ":memory:"
    "#;
        let res: Result<Config, _> = toml::from_str(config);
        assert!(res.is_err(), "Should fail on unknown field");

        // Test unknown nested field
        let config = r#"
        host = "0.0.0.0:3064"
        [indexer]
        fast_sync = true
        unknown_indexer_field = "bad"
        [database]
        type = "sqlite"
        index_path = ":memory:"
        logs_path = ":memory:"
    "#;
        let res: Result<Config, _> = toml::from_str(config);
        assert!(res.is_err(), "Should fail on unknown nested field");

        // Test invalid type
        let config = r#"
        host = "0.0.0.0:3064"
        max_rpc_requests_per_sec = "not_a_number"
        [database]
        type = "sqlite"
        index_path = ":memory:"
        logs_path = ":memory:"
    "#;
        let res: Result<Config, _> = toml::from_str(config);
        assert!(res.is_err(), "Should fail on invalid type");
    }

    #[test]
    fn test_sqlite_strict() {
        let config = r#"
        host = "0.0.0.0:3064"
        [database]
        type = "sqlite"
        index_path = ":memory:"
        logs_path = ":memory:"
        unknown = "bad"
    "#;
        let res: Result<Config, _> = toml::from_str(config);
        assert!(res.is_err(), "Should fail on unknown sqlite field");
    }

    #[test]
    fn test_postgres_detailed_strict() {
        let config = r#"
        host = "0.0.0.0:3064"
        [database]
        type = "postgresql"
        host = "localhost"
        port = 5432
        username = "u"
        password = "p"
        database = "d"
        unknown = "bad"
    "#;
        let res: Result<Config, _> = toml::from_str(config);
        assert!(res.is_err(), "Should fail on unknown postgres detailed field");
    }

    #[test]
    fn test_postgres_url_strict() {
        let config = r#"
        host = "0.0.0.0:3064"
        [database]
        type = "postgresql"
        url = "postgresql://..."
        unknown = "bad"
    "#;
        let res: Result<Config, _> = toml::from_str(config);
        assert!(res.is_err(), "Should fail on unknown postgres url field");
    }

    #[test]
    fn test_api_strict() {
        let config = r#"
        host = "0.0.0.0:3064"
        [api]
        enabled = true
        unknown_api_field = "bad"
        [database]
        type = "sqlite"
        index_path = ":memory:"
        logs_path = ":memory:"
    "#;
        let res: Result<Config, _> = toml::from_str(config);
        assert!(res.is_err(), "Should fail on unknown api field");
    }

    #[test]
    fn test_valid_postgres_config() {
        let config = r#"
        host = "0.0.0.0:3064"
        [database]
        type = "postgresql"
        url = "postgres://..."
    "#;
        let res: Result<Config, _> = toml::from_str(config);
        assert!(res.is_ok(), "Should pass on valid postgres config: {:?}", res.err());
    }

    #[test]
    fn test_valid_config() {
        let config = r#"
        host = "0.0.0.0:3064"
        [database]
        type = "sqlite"
        index_path = ":memory:"
        logs_path = ":memory:"
    "#;
        let res: Result<Config, _> = toml::from_str(config);
        assert!(res.is_ok(), "Should pass on valid config: {:?}", res.err());
    }

    #[test]
    fn test_load_example_config() {
        let project_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let config_path = project_root.join("example-config.toml");

        println!("Loading config from: {:?}", config_path);

        let config_content = fs::read_to_string(&config_path).expect("Failed to read example-config.toml");

        let config: Config = toml::from_str(&config_content).expect("Failed to parse example-config.toml");

        // Basic verification that values are loaded correctly
        assert_eq!(config.host.to_string(), "0.0.0.0:3064");
        assert_eq!(config.network, "dufour");

        // Check database config
        match config.database {
            DatabaseConfig::PostgreSql(pg) => {
                match pg {
                    PostgreSqlConfig::Url(_) => {} // OK
                    _ => panic!("Example config should use URL format by default"),
                }
            }
            _ => panic!("Example config should default to PostgreSQL"),
        }

        // Check indexer config
        assert!(config.indexer.fast_sync);
        assert_eq!(config.indexer.subscription.event_bus_capacity, 1000);

        // Check API config
        assert!(config.api.enabled);
        assert_eq!(config.api.bind_address.to_string(), "0.0.0.0:8080");
    }

    #[test]
    fn test_partial_indexer_config() {
        // Only set fast_sync, all other fields should use defaults
        let config = r#"
        host = "0.0.0.0:3064"
        [indexer]
        fast_sync = false
        [database]
        type = "sqlite"
        index_path = ":memory:"
        logs_path = ":memory:"
    "#;
        let res: Result<Config, _> = toml::from_str(config);
        assert!(res.is_ok(), "Should allow partial indexer config: {:?}", res.err());

        let cfg = res.unwrap();
        assert!(!cfg.indexer.fast_sync);
        assert!(!cfg.indexer.enable_logs_snapshot); // Default
        assert_eq!(cfg.indexer.subscription.event_bus_capacity, 1000); // Default
        assert_eq!(cfg.indexer.subscription.batch_size, 100); // Default
    }

    #[test]
    fn test_partial_api_config() {
        // Only set enabled, all other fields should use defaults
        let config = r#"
        host = "0.0.0.0:3064"
        [api]
        enabled = false
        [database]
        type = "sqlite"
        index_path = ":memory:"
        logs_path = ":memory:"
    "#;
        let res: Result<Config, _> = toml::from_str(config);
        assert!(res.is_ok(), "Should allow partial api config: {:?}", res.err());

        let cfg = res.unwrap();
        assert!(!cfg.api.enabled);
        assert!(cfg.api.playground_enabled); // Default
        assert_eq!(cfg.api.bind_address.to_string(), "0.0.0.0:8080"); // Default
        assert_eq!(cfg.api.health.max_indexer_lag, 10); // Default
        assert_eq!(cfg.api.health.timeout, Duration::from_millis(5000)); // Default
    }

    #[test]
    fn test_partial_subscription_config() {
        // Only set event_bus_capacity, other fields should use defaults
        let config = r#"
        host = "0.0.0.0:3064"
        [indexer.subscription]
        event_bus_capacity = 500
        [database]
        type = "sqlite"
        index_path = ":memory:"
        logs_path = ":memory:"
    "#;
        let res: Result<Config, _> = toml::from_str(config);
        assert!(res.is_ok(), "Should allow partial subscription config: {:?}", res.err());

        let cfg = res.unwrap();
        assert_eq!(cfg.indexer.subscription.event_bus_capacity, 500);
        assert_eq!(cfg.indexer.subscription.shutdown_signal_capacity, 10); // Default
        assert_eq!(cfg.indexer.subscription.batch_size, 100); // Default
    }
}
