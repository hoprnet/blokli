//! Configuration for the blokli API server

use std::{net::SocketAddr, path::PathBuf};

use serde::{Deserialize, Serialize};

/// API server configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiConfig {
    /// Address to bind the server to
    #[serde(default = "default_bind_address")]
    pub bind_address: SocketAddr,

    /// Enable GraphQL playground
    #[serde(default = "default_playground_enabled")]
    pub playground_enabled: bool,

    /// TLS configuration
    #[serde(default)]
    pub tls: Option<TlsConfig>,
}

/// TLS configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TlsConfig {
    /// Path to TLS certificate file
    pub cert_path: PathBuf,

    /// Path to TLS private key file
    pub key_path: PathBuf,
}

impl Default for ApiConfig {
    fn default() -> Self {
        Self {
            bind_address: default_bind_address(),
            playground_enabled: default_playground_enabled(),
            tls: None,
        }
    }
}

fn default_bind_address() -> SocketAddr {
    "0.0.0.0:8080".parse().unwrap()
}

fn default_playground_enabled() -> bool {
    true
}
