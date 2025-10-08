use blokli_chain_types::ContractAddresses;
use hopr_chain_config::{ChainNetworkConfig, ProtocolsConfig};

fn default_host() -> std::net::SocketAddr {
    "0.0.0.0:3064".parse().unwrap()
}

fn default_database() -> DatabaseConfig {
    DatabaseConfig::Url {
        url: "postgresql://bloklid:password@localhost:5432/bloklid".to_string(),
    }
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
pub enum DatabaseConfig {
    /// Simple connection URL format
    Url {
        url: String,
    },
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

fn default_max_connections() -> u32 {
    10
}

impl DatabaseConfig {
    /// Convert database configuration to PostgreSQL connection URL
    pub fn to_url(&self) -> String {
        match self {
            DatabaseConfig::Url { url } => url.clone(),
            DatabaseConfig::Detailed {
                host,
                port,
                username,
                password,
                database,
                ..
            } => {
                format!("postgresql://{}:{}@{}:{}/{}", username, password, host, port, database)
            }
        }
    }

    /// Get max_connections setting (if specified in Detailed format)
    pub fn max_connections(&self) -> u32 {
        match self {
            DatabaseConfig::Url { .. } => default_max_connections(),
            DatabaseConfig::Detailed { max_connections, .. } => *max_connections,
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
