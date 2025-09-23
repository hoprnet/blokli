use blokli_chain_types::ContractAddresses;
use hopr_chain_config::{ChainNetworkConfig, ProtocolsConfig};

fn default_host() -> std::net::SocketAddr {
    "0.0.0.0:3064".parse().unwrap()
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

fn default_network() -> String {
    "dufour".to_string()
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
    #[default(0)]
    pub start_block_number: u64,

    #[default(true)]
    pub fast_sync: bool,

    #[default(false)]
    pub enable_logs_snapshot: bool,

    #[serde(default)]
    pub logs_snapshot_url: Option<String>,
}
