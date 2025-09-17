pub const DEFAULT_HOST: &str = "0.0.0.0:3064";

fn default_host() -> std::net::SocketAddr {
    DEFAULT_HOST.parse().unwrap()
}

fn default_db_path() -> String {
    "data/bloklid.db".to_string()
}

fn default_rpc_url() -> String {
    "http://localhost:8545".to_string()
}

fn default_data_directory() -> String {
    "data".to_string()
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, smart_default::SmartDefault)]
pub struct ChainConfig {
    #[default(100)]
    pub chain_id: u32,

    #[default(5000)]
    pub block_time: u64,

    #[default(_code = "default_rpc_url()")]
    #[serde(default = "default_rpc_url")]
    pub default_provider: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, smart_default::SmartDefault)]
pub struct ChainNetworkConfig {
    #[serde(default)]
    pub chain: ChainConfig,

    #[serde(default)]
    pub max_requests_per_sec: Option<u32>,

    #[default(5000)]
    pub tx_polling_interval: u64,

    #[default(12)]
    pub confirmations: u32,

    #[default(10000)]
    pub max_block_range: u64,
}

impl From<ChainNetworkConfig> for blokli_chain_api::ChainNetworkConfig {
    fn from(config: ChainNetworkConfig) -> Self {
        hopr_chain_config::ChainNetworkConfig {
            chain: blokli_chain_api::ChainConfig {
                chain_id: config.chain.chain_id,
                block_time: config.chain.block_time,
                default_provider: config.chain.default_provider,
            },
            max_requests_per_sec: config.max_requests_per_sec,
            tx_polling_interval: config.tx_polling_interval,
            confirmations: config.confirmations,
            max_block_range: config.max_block_range,
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

    #[default(_code = "default_db_path()")]
    #[serde(default = "default_db_path")]
    pub database_path: String,

    #[default(_code = "default_data_directory()")]
    #[serde(default = "default_data_directory")]
    pub data_directory: String,

    #[default(_code = "default_rpc_url()")]
    #[serde(default = "default_rpc_url")]
    pub rpc_url: String,

    #[serde(default)]
    pub indexer: IndexerConfig,

    #[serde(default)]
    pub chain: ChainNetworkConfig,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, smart_default::SmartDefault)]
pub struct IndexerConfig {
    #[default(0)]
    pub start_block_number: u64,

    #[default(true)]
    pub fast_sync: bool,

    #[default(false)]
    pub enable_logs_snapshot: bool,

    #[serde(default)]
    pub logs_snapshot_url: Option<String>,
}
