use blokli_chain_types::ContractAddresses;
use hopr_chain_config::{ChainNetworkConfig, ProtocolsConfig};

fn default_host() -> std::net::SocketAddr {
    "0.0.0.0:3064".parse().unwrap()
}

fn default_database() -> DatabaseConfig {
    DatabaseConfig::PostgreSql(PostgreSqlConfig::Url {
        url: "postgresql://bloklid:password@localhost:5432/bloklid".to_string(),
    })
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
    Url { url: String },
    /// Detailed connection parameters
    Detailed {
        host: String,
        port: u16,
        username: String,
        password: String,
        database: String,
        #[serde(default = "default_max_connections")]
        max_connections: u32,
    },
}

/// SQLite database configuration
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SqliteConfig {
    /// Path to the SQLite database file
    /// Use ":memory:" for in-memory database
    pub path: String,
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
}

fn default_max_connections() -> u32 {
    10
}

impl DatabaseConfig {
    /// Convert database configuration to connection URL
    pub fn to_url(&self) -> String {
        match self {
            DatabaseConfig::PostgreSql(pg_config) => match pg_config {
                PostgreSqlConfig::Url { url } => url.clone(),
                PostgreSqlConfig::Detailed {
                    host,
                    port,
                    username,
                    password,
                    database,
                    ..
                } => {
                    format!("postgresql://{}:{}@{}:{}/{}", username, password, host, port, database)
                }
            },
            DatabaseConfig::Sqlite(sqlite_config) => {
                format!("sqlite://{}?mode=rwc", sqlite_config.path)
            }
        }
    }

    /// Get max_connections setting
    pub fn max_connections(&self) -> u32 {
        match self {
            DatabaseConfig::PostgreSql(pg_config) => match pg_config {
                PostgreSqlConfig::Url { .. } => default_max_connections(),
                PostgreSqlConfig::Detailed { max_connections, .. } => *max_connections,
            },
            DatabaseConfig::Sqlite(sqlite_config) => sqlite_config.max_connections,
        }
    }
}

#[serde_with::serde_as]
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, smart_default::SmartDefault, validator::Validate)]
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
pub struct IndexerConfig {
    #[default(true)]
    pub fast_sync: bool,

    #[default(false)]
    pub enable_logs_snapshot: bool,

    #[serde(default)]
    pub logs_snapshot_url: Option<String>,
}

#[serde_with::serde_as]
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, smart_default::SmartDefault)]
pub struct ApiConfig {
    #[default(true)]
    pub enabled: bool,

    #[serde_as(as = "serde_with::DisplayFromStr")]
    #[default(_code = "default_api_bind_address()")]
    #[serde(default = "default_api_bind_address")]
    pub bind_address: std::net::SocketAddr,

    #[default(true)]
    pub playground_enabled: bool,
}

fn default_api_bind_address() -> std::net::SocketAddr {
    "0.0.0.0:8080".parse().unwrap()
}
