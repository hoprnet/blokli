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
