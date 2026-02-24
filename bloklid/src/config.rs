use std::time::Duration;

use blokli_chain_indexer::utils::redact_url;
use blokli_chain_types::{ChainConfig, ContractAddresses};

use crate::network::Network;

/// Redacts username and password from database URLs while keeping host, port, and database visible
///
/// # Examples
/// ```
/// # use bloklid::config::redact_database_url;
/// assert_eq!(
///     redact_database_url("postgres://user:pass@localhost:5432/mydb"),
///     "postgres://REDACTED:REDACTED@localhost:5432/mydb"
/// );
/// assert_eq!(
///     redact_database_url("postgresql://localhost:5432/mydb"),
///     "postgresql://localhost:5432/mydb"
/// );
/// ```
pub fn redact_database_url(url: &str) -> String {
    // Parse the URL to extract components
    if let Some(scheme_end) = url.find("://") {
        let scheme = &url[..scheme_end + 3];
        let rest = &url[scheme_end + 3..];

        // Check if there's an @ sign indicating credentials
        if let Some(at_pos) = rest.find('@') {
            let after_at = &rest[at_pos..];
            // Redact credentials but keep everything else
            format!("{}REDACTED:REDACTED{}", scheme, after_at)
        } else {
            // No credentials, return as-is
            url.to_string()
        }
    } else {
        // Not a URL format, return as-is
        url.to_string()
    }
}

fn default_rpc_url() -> String {
    "http://localhost:8545".to_string()
}

fn default_data_directory() -> String {
    "data".to_string()
}

fn default_network() -> Network {
    Network::default()
}

fn default_max_block_range() -> u32 {
    10000
}

/// PostgreSQL database configuration
///
/// Supports two formats:
/// 1. Simple URL: `url = "postgresql://user:pass@host:port/database"`
/// 2. Detailed components with individual fields (host, port, username, password, database)
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PostgreSqlConfig {
    /// Connection URL (Option 1: simple URL format)
    #[serde(default)]
    pub url: Option<String>,
    /// Host (Option 2: detailed configuration)
    #[serde(default)]
    pub host: Option<String>,
    /// Port (Option 2: detailed configuration)
    #[serde(default)]
    pub port: Option<u16>,
    /// Username (Option 2: detailed configuration)
    #[serde(default)]
    pub username: Option<String>,
    /// Password (Option 2: detailed configuration)
    #[serde(default)]
    pub password: Option<String>,
    /// Database (Option 2: detailed configuration)
    #[serde(default)]
    pub database: Option<String>,
    /// Maximum number of connections
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

/// In-memory database configuration
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InMemoryConfig {
    #[serde(default = "default_max_connections")]
    pub max_connections: u32,
}

/// Database configuration supporting both PostgreSQL and SQLite
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum DatabaseConfig {
    #[serde(rename = "postgresql")]
    PostgreSql(PostgreSqlConfig),
    #[serde(rename = "sqlite")]
    Sqlite(SqliteConfig),
    #[serde(rename = "in-memory")]
    InMemory(InMemoryConfig),
}

fn default_max_connections() -> u32 {
    10
}

impl DatabaseConfig {
    /// Convert database configuration to connection URL
    /// Constructs a database connection URL.
    ///
    /// For PostgreSQL:
    /// - If `url` is provided in config, returns it directly
    /// - Otherwise, constructs URL from individual fields with the following defaults:
    ///   - `host`: "localhost" if not specified
    ///   - `port`: 5432 if not specified
    ///   - `username`: empty string if not specified
    ///   - `password`: empty string if not specified
    ///   - `database`: empty string if not specified
    ///
    /// For SQLite, returns the path with read-write mode enabled.
    ///
    /// # Warning
    ///
    /// When using detailed PostgreSQL configuration fields instead of a full URL,
    /// ensure all required connection parameters are provided. Missing parameters
    /// will use defaults which may result in connection failures.
    pub fn to_url(&self) -> String {
        match self {
            DatabaseConfig::PostgreSql(pg_config) => {
                // If URL is provided, use it directly
                if let Some(url) = &pg_config.url {
                    return url.clone();
                }

                // Otherwise, construct from detailed fields
                let username = pg_config.username.as_deref().unwrap_or("");
                let password = pg_config.password.as_deref().unwrap_or("");
                let host = pg_config.host.as_deref().unwrap_or("localhost");
                let port = pg_config.port.unwrap_or(5432);
                let database = pg_config.database.as_deref().unwrap_or("");
                format!("postgresql://{}:{}@{}:{}/{}", username, password, host, port, database)
            }
            DatabaseConfig::Sqlite(sqlite_config) => {
                format!("sqlite://{}?mode=rwc", sqlite_config.index_path)
            }
            DatabaseConfig::InMemory(_) => "sqlite::memory:".to_string(),
        }
    }

    /// Get logs database URL (only applicable for SQLite)
    /// For PostgreSQL, returns None as it uses a single database
    /// For SQLite, returns the logs database URL
    pub fn to_logs_url(&self) -> Option<String> {
        match self {
            DatabaseConfig::PostgreSql(_) => None,
            DatabaseConfig::Sqlite(sqlite_config) => Some(format!("sqlite://{}?mode=rwc", sqlite_config.logs_path)),
            DatabaseConfig::InMemory(_) => Some("sqlite::memory:".to_string()),
        }
    }

    /// Get max_connections setting
    pub fn max_connections(&self) -> u32 {
        match self {
            DatabaseConfig::PostgreSql(pg_config) => pg_config.max_connections,
            DatabaseConfig::Sqlite(sqlite_config) => sqlite_config.max_connections,
            DatabaseConfig::InMemory(in_memory_config) => in_memory_config.max_connections,
        }
    }

    /// Inform if the db is stored in memory
    pub fn is_in_memory(&self) -> bool {
        matches!(self, DatabaseConfig::InMemory(_))
    }

    /// Display the database configuration with sensitive data redacted
    pub fn display_redacted(&self) -> String {
        match self {
            DatabaseConfig::PostgreSql(pg_config) => {
                let url_display = if let Some(url) = &pg_config.url {
                    format!("url={}", redact_database_url(url))
                } else {
                    let host = pg_config.host.as_deref().unwrap_or("localhost");
                    let port = pg_config.port.unwrap_or(5432);
                    let user = if pg_config.username.is_some() {
                        "configured".to_string()
                    } else {
                        "REDACTED".to_string()
                    };
                    let pass = pg_config
                        .password
                        .as_ref()
                        .map(|_| "REDACTED".to_string())
                        .unwrap_or("(none)".to_string());
                    let db = pg_config.database.as_deref().unwrap_or("bloklid");
                    format!(
                        "host={}, port={}, user={}, password={}, database={}",
                        host, port, user, pass, db
                    )
                };
                format!(
                    "PostgreSQL: {}, max_connections={}",
                    url_display, pg_config.max_connections
                )
            }
            DatabaseConfig::Sqlite(sqlite_config) => {
                format!(
                    "SQLite: index_path={}, logs_path={}, max_connections={}",
                    sqlite_config.index_path, sqlite_config.logs_path, sqlite_config.max_connections
                )
            }
            DatabaseConfig::InMemory(in_memory_config) => {
                format!(
                    "In-Memory Database: max_connections={}",
                    in_memory_config.max_connections
                )
            }
        }
    }
}

#[serde_with::serde_as]
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, smart_default::SmartDefault, validator::Validate)]
#[serde(deny_unknown_fields)]
pub struct Config {
    #[serde(default)]
    pub database: Option<DatabaseConfig>,

    #[default(_code = "default_data_directory()")]
    #[serde(default = "default_data_directory")]
    pub data_directory: String,

    #[default(_code = "default_network()")]
    #[serde(default = "default_network")]
    pub network: Network,

    #[default(_code = "default_rpc_url()")]
    #[serde(default = "default_rpc_url")]
    pub rpc_url: String,

    #[serde(default)]
    pub max_rpc_requests_per_sec: u32,

    #[validate(range(min = 1))]
    #[default(10000)]
    #[serde(default = "default_max_block_range")]
    pub max_block_range: u32,

    #[serde(default)]
    pub indexer: IndexerConfig,

    #[serde(default)]
    pub api: ApiConfig,

    #[serde(default, rename = "contracts")]
    pub contracts_override: Option<ContractAddresses>,

    #[serde(skip)]
    #[default(None)]
    pub chain_network: Option<ChainConfig>,

    #[serde(skip)]
    #[default(_code = "ContractAddresses::default()")]
    pub contracts: ContractAddresses,
}

impl Config {
    /// Display the configuration with sensitive data redacted
    pub fn display_redacted(&self) -> String {
        let mut output = String::new();
        output.push_str("Configuration:\n");
        output.push_str(&format!("  network: {}\n", self.network));
        output.push_str(&format!("  rpc_url: {}\n", redact_url(&self.rpc_url)));
        output.push_str(&format!("  data_directory: {}\n", self.data_directory));
        output.push_str(&format!(
            "  max_rpc_requests_per_sec: {}\n",
            self.max_rpc_requests_per_sec
        ));
        output.push_str(&format!("  max_block_range: {}\n", self.max_block_range));

        if let Some(db_config) = &self.database {
            output.push_str(&format!("  database: {}\n", db_config.display_redacted()));
        } else {
            output.push_str("  database: (not configured)\n");
        }

        output.push_str(&format!("  indexer.fast_sync: {}\n", self.indexer.fast_sync));
        output.push_str(&format!(
            "  indexer.enable_logs_snapshot: {}\n",
            self.indexer.enable_logs_snapshot
        ));

        if let Some(snapshot_url) = &self.indexer.logs_snapshot_url {
            output.push_str(&format!("  indexer.logs_snapshot_url: {}\n", redact_url(snapshot_url)));
        }

        output.push_str(&format!("  api.enabled: {}\n", self.api.enabled));
        output.push_str(&format!("  api.bind_address: {}\n", self.api.bind_address));
        output.push_str(&format!("  api.playground_enabled: {}\n", self.api.playground_enabled));
        output.push_str(&format!("  api.gas_multiplier: {}\n", self.api.gas_multiplier));
        output.push_str(&format!(
            "  api.sse_keepalive.enabled: {}\n",
            self.api.sse_keepalive.enabled
        ));
        output.push_str(&format!(
            "  api.sse_keepalive.interval: {:?}\n",
            self.api.sse_keepalive.interval
        ));
        output.push_str(&format!("  api.sse_keepalive.text: {}\n", self.api.sse_keepalive.text));
        output.push_str(&format!(
            "  api.health.max_indexer_lag: {}\n",
            self.api.health.max_indexer_lag
        ));
        output.push_str(&format!("  api.health.timeout: {:?}\n", self.api.health.timeout));
        output.push_str(&format!(
            "  api.health.readiness_check_interval: {:?}\n",
            self.api.health.readiness_check_interval
        ));

        output
    }
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

    #[default(1.0)]
    #[serde(default = "default_api_gas_multiplier")]
    pub gas_multiplier: f64,

    #[serde(default)]
    pub sse_keepalive: SseKeepAliveConfig,

    #[serde(default)]
    pub health: HealthConfig,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, smart_default::SmartDefault)]
#[serde(deny_unknown_fields)]
pub struct SseKeepAliveConfig {
    #[default(true)]
    #[serde(default = "default_sse_keepalive_enabled")]
    pub enabled: bool,

    #[default(_code = "Duration::from_secs(15)")]
    #[serde(default = "default_sse_keepalive_interval", with = "humantime_serde")]
    pub interval: Duration,

    #[default(_code = "default_sse_keepalive_text()")]
    #[serde(default = "default_sse_keepalive_text")]
    pub text: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, smart_default::SmartDefault)]
#[serde(deny_unknown_fields)]
pub struct HealthConfig {
    /// Maximum allowed indexer lag (in blocks) before readiness check fails
    #[default(10)]
    #[serde(default = "default_max_indexer_lag")]
    pub max_indexer_lag: u64,

    /// Timeout for health check queries
    #[default(_code = "Duration::from_millis(5000)")]
    #[serde(default = "default_health_timeout", with = "humantime_serde")]
    pub timeout: Duration,

    /// Interval for periodic readiness checks
    #[default(_code = "Duration::from_secs(60)")]
    #[serde(default = "default_readiness_check_interval", with = "humantime_serde")]
    pub readiness_check_interval: Duration,
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

fn default_sse_keepalive_enabled() -> bool {
    true
}

fn default_api_gas_multiplier() -> f64 {
    1.0
}

fn default_sse_keepalive_interval() -> Duration {
    Duration::from_secs(15)
}

fn default_sse_keepalive_text() -> String {
    "keep-alive".to_string()
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

#[cfg(test)]
mod tests {
    use std::{fs, path::PathBuf};

    use super::*;

    #[test]
    fn test_strict_parsing() {
        // Test unknown top-level field
        let config = r#"
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
         [database]
         type = "sqlite"
         index_path = ":memory:"
         logs_path = ":memory:"
     "#;
        let res: Result<Config, _> = toml::from_str(config);
        assert!(res.is_ok(), "Should pass on valid config: {:?}", res.err());
    }

    #[test]
    fn test_api_sse_keepalive_defaults() {
        let config = r#"
         [database]
         type = "sqlite"
         index_path = ":memory:"
         logs_path = ":memory:"
     "#;
        let cfg: Config = toml::from_str(config).expect("Failed to parse config");
        assert!(cfg.api.sse_keepalive.enabled);
        assert_eq!(cfg.api.sse_keepalive.interval, Duration::from_secs(15));
        assert_eq!(cfg.api.sse_keepalive.text, "keep-alive");
    }

    #[test]
    fn test_api_sse_keepalive_override() {
        let config = r#"
         [api.sse_keepalive]
         enabled = false
         interval = "5s"
         text = "ping"
         [database]
         type = "sqlite"
         index_path = ":memory:"
         logs_path = ":memory:"
     "#;
        let cfg: Config = toml::from_str(config).expect("Failed to parse config");
        assert!(!cfg.api.sse_keepalive.enabled);
        assert_eq!(cfg.api.sse_keepalive.interval, Duration::from_secs(5));
        assert_eq!(cfg.api.sse_keepalive.text, "ping");
    }

    #[test]
    fn test_config_without_database_section() {
        let config = r#"
         network = "rotsee"
         rpc_url = "http://localhost:8545"
     "#;
        let res: Result<Config, _> = toml::from_str(config);
        assert!(
            res.is_ok(),
            "Should allow config without database section: {:?}",
            res.err()
        );

        let cfg = res.unwrap();
        assert!(cfg.database.is_none(), "database should be None when not specified");
    }

    #[test]
    fn test_load_example_config() {
        let project_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let config_path = project_root.join("example-config.toml");

        println!("Loading config from: {:?}", config_path);

        let config_content = fs::read_to_string(&config_path).expect("Failed to read example-config.toml");

        let config: Config = toml::from_str(&config_content).expect("Failed to parse example-config.toml");

        // Basic verification that values are loaded correctly
        assert_eq!(config.network, Network::Rotsee);

        // Check database config (should be present in example-config.toml)
        match &config.database {
            Some(DatabaseConfig::PostgreSql(pg)) => {
                // Should have a URL configured
                assert!(pg.url.is_some(), "Example config should have URL configured");
            }
            _ => panic!("Example config should have PostgreSQL database section"),
        }

        // Check indexer config
        assert!(config.indexer.fast_sync);
        assert_eq!(config.indexer.subscription.event_bus_capacity, 1000);

        // Check API config
        assert!(config.api.enabled);
        assert_eq!(config.api.bind_address.to_string(), "0.0.0.0:8080");
        assert_eq!(config.api.gas_multiplier, 1.0);
    }

    #[test]
    fn test_partial_indexer_config() {
        // Only set fast_sync, all other fields should use defaults
        let config = r#"
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
        assert_eq!(cfg.api.gas_multiplier, 1.0); // Default
        assert_eq!(cfg.api.health.max_indexer_lag, 10); // Default
        assert_eq!(cfg.api.health.timeout, Duration::from_millis(5000)); // Default
        assert!(cfg.database.is_some()); // Database was provided
    }

    #[test]
    fn test_api_gas_multiplier_override() {
        let config = r#"
         [api]
         gas_multiplier = 1.5
         [database]
         type = "sqlite"
         index_path = ":memory:"
         logs_path = ":memory:"
     "#;
        let cfg: Config = toml::from_str(config).expect("Failed to parse config");
        assert_eq!(cfg.api.gas_multiplier, 1.5);
    }

    #[test]
    fn test_partial_subscription_config() {
        // Only set event_bus_capacity, other fields should use defaults
        let config = r#"
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

    #[test]
    fn test_redact_database_url_with_credentials() {
        // URL with username and password should redact only credentials
        let url = "postgres://user:password@localhost:5432/mydb";
        let redacted = redact_database_url(url);
        assert_eq!(redacted, "postgres://REDACTED:REDACTED@localhost:5432/mydb");
    }

    #[test]
    fn test_redact_database_url_without_credentials() {
        // URL without credentials should remain unchanged
        let url = "postgresql://localhost:5432/mydb";
        let redacted = redact_database_url(url);
        assert_eq!(redacted, "postgresql://localhost:5432/mydb");
    }

    #[test]
    fn test_redact_database_url_with_port() {
        // URL with custom port
        let url = "postgres://admin:secret@db.example.com:9876/production";
        let redacted = redact_database_url(url);
        assert_eq!(redacted, "postgres://REDACTED:REDACTED@db.example.com:9876/production");
    }

    #[test]
    fn test_redact_non_url_string() {
        // Non-URL string should remain unchanged for database URLs
        let not_url = "just-a-string";
        assert_eq!(redact_database_url(not_url), not_url);
    }
}
