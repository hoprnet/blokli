//! Configuration for the blokli API server

use serde::{Deserialize, Serialize};
use std::net::SocketAddr;

/// API server configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiConfig {
    /// Address to bind the server to
    #[serde(default = "default_bind_address")]
    pub bind_address: SocketAddr,

    /// Enable GraphQL playground
    #[serde(default = "default_playground_enabled")]
    pub playground_enabled: bool,
}

impl Default for ApiConfig {
    fn default() -> Self {
        Self {
            bind_address: default_bind_address(),
            playground_enabled: default_playground_enabled(),
        }
    }
}

fn default_bind_address() -> SocketAddr {
    "0.0.0.0:8080".parse().unwrap()
}

fn default_playground_enabled() -> bool {
    true
}
