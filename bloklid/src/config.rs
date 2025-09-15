pub const DEFAULT_HOST: &str = "0.0.0.0:3064";

fn default_host() -> std::net::SocketAddr {
    DEFAULT_HOST.parse().unwrap()
}

#[serde_with::serde_as]
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, smart_default::SmartDefault, validator::Validate)]
pub struct Config {
    #[serde_as(as = "serde_with::DisplayFromStr")]
    #[default(_code = "default_host()")]
    #[serde(default = "default_host")]
    pub host: std::net::SocketAddr,
}
