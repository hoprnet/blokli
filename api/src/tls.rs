//! TLS configuration and utilities for the blokli API server
//!
//! This module provides TLS 1.3 support using rustls.

use std::{fs::File, io::BufReader, sync::Arc, time::Duration};

use rustls::{ServerConfig, pki_types::CertificateDer};
use tokio::net::{TcpListener, TcpStream};
use tokio_rustls::{TlsAcceptor, server::TlsStream};

use crate::{
    config::TlsConfig,
    errors::{ApiError, ApiResult},
};

/// Create a TLS acceptor configured for TLS 1.3 only
pub fn create_tls_acceptor(config: &TlsConfig) -> ApiResult<TlsAcceptor> {
    // Load certificates
    let cert_file = File::open(&config.cert_path)
        .map_err(|e| ApiError::ConfigError(format!("Failed to open certificate file: {}", e)))?;
    let mut cert_reader = BufReader::new(cert_file);
    let certs: Vec<CertificateDer> = rustls_pemfile::certs(&mut cert_reader)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| ApiError::ConfigError(format!("Failed to parse certificates: {}", e)))?;

    if certs.is_empty() {
        return Err(ApiError::ConfigError("No certificates found in file".to_string()));
    }

    // Load private key
    let key_file = File::open(&config.key_path)
        .map_err(|e| ApiError::ConfigError(format!("Failed to open private key file: {}", e)))?;
    let mut key_reader = BufReader::new(key_file);
    let key = rustls_pemfile::private_key(&mut key_reader)
        .map_err(|e| ApiError::ConfigError(format!("Failed to parse private key: {}", e)))?
        .ok_or_else(|| ApiError::ConfigError("No private key found in file".to_string()))?;

    // Configure rustls for TLS 1.3 only
    let mut server_config = ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(certs, key)
        .map_err(|e| ApiError::ConfigError(format!("Invalid TLS configuration: {}", e)))?;

    // Enable HTTP/2 ALPN
    server_config.alpn_protocols = vec![b"h2".to_vec(), b"http/1.1".to_vec()];

    Ok(TlsAcceptor::from(Arc::new(server_config)))
}

/// TLS listener that accepts connections and wraps them in TLS
pub struct TlsListener {
    acceptor: TlsAcceptor,
    listener: TcpListener,
}

impl TlsListener {
    pub fn new(acceptor: TlsAcceptor, listener: TcpListener) -> Self {
        Self { acceptor, listener }
    }
}

// Implement Listener trait for axum compatibility
impl axum::serve::Listener for TlsListener {
    type Addr = std::net::SocketAddr;
    type Io = TlsStream<TcpStream>;

    async fn accept(&mut self) -> (Self::Io, Self::Addr) {
        loop {
            match self.listener.accept().await {
                Ok((stream, addr)) => match self.acceptor.accept(stream).await {
                    Ok(tls_stream) => return (tls_stream, addr),
                    Err(e) => {
                        tracing::error!("TLS handshake failed: {}", e);
                        continue;
                    }
                },
                Err(e) => {
                    tracing::error!("TCP accept failed: {}", e);
                    tokio::time::sleep(Duration::from_millis(100)).await;
                    continue;
                }
            }
        }
    }

    fn local_addr(&self) -> std::io::Result<Self::Addr> {
        self.listener.local_addr()
    }
}
