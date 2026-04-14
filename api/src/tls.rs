//! TLS configuration and utilities for the blokli API server
//!
//! This module provides TLS 1.3 support using rustls.

use std::{fs, sync::Arc, time::Duration};

use pem_rfc7468::decode_vec;
use rustls::{
    ServerConfig,
    pki_types::{CertificateDer, PrivateKeyDer},
};
use tokio::net::{TcpListener, TcpStream};
use tokio_rustls::{TlsAcceptor, server::TlsStream};

use crate::{
    config::TlsConfig,
    errors::{ApiError, ApiResult},
};

/// Create a TLS acceptor configured for TLS 1.3 only
pub fn create_tls_acceptor(config: &TlsConfig) -> ApiResult<TlsAcceptor> {
    // Read certs and private key
    let (certs, key) = read_cert_and_key(config)?;

    // Configure rustls for TLS 1.3 only
    let mut server_config = ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(certs, key)
        .map_err(|e| ApiError::ConfigError(format!("Invalid TLS configuration: {}", e)))?;

    // Enable HTTP/2 ALPN
    server_config.alpn_protocols = vec![b"h2".to_vec(), b"http/1.1".to_vec()];

    Ok(TlsAcceptor::from(Arc::new(server_config)))
}

/// Reads certificates based on provided config, and extracts private key.
fn read_cert_and_key(config: &TlsConfig) -> ApiResult<(Vec<CertificateDer<'static>>, PrivateKeyDer<'static>)> {
    // Load certificates
    let cert_data = fs::read(&config.cert_path)
        .map_err(|e| ApiError::ConfigError(format!("Failed to read certificate file: {}", e)))?;

    let mut certs = Vec::new();
    let cert_str = std::str::from_utf8(&cert_data)
        .map_err(|e| ApiError::ConfigError(format!("Certificate file is not valid UTF-8: {}", e)))?;
    const CERT_BEGIN: &str = "-----BEGIN CERTIFICATE-----";
    const CERT_END: &str = "-----END CERTIFICATE-----";
    let mut rest = cert_str;

    while let Some(begin_idx) = rest.find(CERT_BEGIN) {
        let after_begin = &rest[begin_idx..];
        let end_idx = after_begin
            .find(CERT_END)
            .ok_or_else(|| ApiError::ConfigError("Unterminated certificate PEM block".to_string()))?;

        let pem_block = &after_begin[..end_idx + CERT_END.len()];
        let (label, der) = decode_vec(pem_block.as_bytes())
            .map_err(|e| ApiError::ConfigError(format!("Failed to decode certificate: {}", e)))?;

        if label == "CERTIFICATE" {
            certs.push(CertificateDer::from(der));
        }

        rest = &after_begin[end_idx + CERT_END.len()..];
    }

    if certs.is_empty() {
        return Err(ApiError::ConfigError("No certificates found in file".to_string()));
    }

    // Load Private Key
    let key_data =
        fs::read(&config.key_path).map_err(|e| ApiError::ConfigError(format!("Failed to read key file: {}", e)))?;

    let (label, key_der) =
        decode_vec(&key_data).map_err(|e| ApiError::ConfigError(format!("Key parse error: {}", e)))?;

    let key = match label {
        "PRIVATE KEY" => PrivateKeyDer::Pkcs8(key_der.into()),
        "RSA PRIVATE KEY" => PrivateKeyDer::Pkcs1(key_der.into()),
        "EC PRIVATE KEY" => PrivateKeyDer::Sec1(key_der.into()),
        _ => return Err(ApiError::ConfigError(format!("Unsupported key label: {}", label))),
    };

    Ok((certs, key))
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

#[cfg(test)]
mod tests {
    use std::{io::Write, path::PathBuf};

    use anyhow::Context;
    use base64ct::{Base64, Encoding};
    use tempfile::NamedTempFile;

    use super::*;

    fn create_pem(label: &str, data: &[u8]) -> String {
        let b64 = Base64::encode_string(data);
        format!("-----BEGIN {}-----\n{}\n-----END {}-----", label, b64, label)
    }

    #[test]
    fn test_read_cert_and_key_pkcs8() -> anyhow::Result<()> {
        let cert_data = create_pem("CERTIFICATE", b"dummy cert data");
        let key_data = create_pem("PRIVATE KEY", b"dummy key data");

        let mut cert_file = NamedTempFile::new().context("Failed to create temp cert file")?;
        cert_file
            .write_all(cert_data.as_bytes())
            .context("Failed to write to temp cert file")?;

        let mut key_file = NamedTempFile::new().context("Failed to create temp key file")?;
        key_file
            .write_all(key_data.as_bytes())
            .context("Failed to write to temp key file")?;

        let config = TlsConfig {
            cert_path: cert_file.path().to_path_buf(),
            key_path: key_file.path().to_path_buf(),
        };

        let (certs, key) = read_cert_and_key(&config).map_err(|e| anyhow::anyhow!("{:?}", e))?;
        let certs_len: usize = certs.len();
        assert_eq!(certs_len, 1);
        match key {
            PrivateKeyDer::Pkcs8(_) => (),
            _ => panic!("Expected PKCS#8 key"),
        }
        Ok(())
    }

    #[test]
    fn test_read_cert_and_key_pkcs1() -> anyhow::Result<()> {
        let cert_data = create_pem("CERTIFICATE", b"dummy cert data");
        let key_data = create_pem("RSA PRIVATE KEY", b"dummy key data");

        let mut cert_file = NamedTempFile::new().context("Failed to create temp cert file")?;
        cert_file
            .write_all(cert_data.as_bytes())
            .context("Failed to write to temp cert file")?;

        let mut key_file = NamedTempFile::new().context("Failed to create temp key file")?;
        key_file
            .write_all(key_data.as_bytes())
            .context("Failed to write to temp key file")?;

        let config = TlsConfig {
            cert_path: cert_file.path().to_path_buf(),
            key_path: key_file.path().to_path_buf(),
        };

        let (certs, key) = read_cert_and_key(&config).map_err(|e| anyhow::anyhow!("{:?}", e))?;
        let certs_len: usize = certs.len();
        assert_eq!(certs_len, 1);
        match key {
            PrivateKeyDer::Pkcs1(_) => (),
            _ => panic!("Expected PKCS#1 key"),
        }
        Ok(())
    }

    #[test]
    fn test_read_cert_and_key_multiple_certs() -> anyhow::Result<()> {
        let cert1_data = create_pem("CERTIFICATE", b"cert 1");
        let cert2_data = create_pem("CERTIFICATE", b"cert 2");
        let cert_data = format!("{}\n{}", cert1_data, cert2_data);
        let key_data = create_pem("PRIVATE KEY", b"dummy key data");

        let mut cert_file = NamedTempFile::new().context("Failed to create temp cert file")?;
        cert_file
            .write_all(cert_data.as_bytes())
            .context("Failed to write to temp cert file")?;

        let mut key_file = NamedTempFile::new().context("Failed to create temp key file")?;
        key_file
            .write_all(key_data.as_bytes())
            .context("Failed to write to temp key file")?;

        let config = TlsConfig {
            cert_path: cert_file.path().to_path_buf(),
            key_path: key_file.path().to_path_buf(),
        };

        let (certs, _key) = read_cert_and_key(&config).map_err(|e| anyhow::anyhow!("{:?}", e))?;
        let certs_len: usize = certs.len();
        assert_eq!(certs_len, 2, "Should read 2 certificates");
        Ok(())
    }

    #[test]
    fn test_read_committed_cert_and_key() -> anyhow::Result<()> {
        let mut cert_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        cert_path.push("tests/fixtures/tls/cert.pem");

        // We use a temporary key for testing with a committed cert
        let key_data = create_pem("PRIVATE KEY", b"dummy key data");
        let mut key_file = NamedTempFile::new().context("Failed to create temp key file")?;
        key_file
            .write_all(key_data.as_bytes())
            .context("Failed to write to temp key file")?;

        let config = TlsConfig {
            cert_path,
            key_path: key_file.path().to_path_buf(),
        };

        let (certs, _key) = read_cert_and_key(&config).map_err(|e| anyhow::anyhow!("{:?}", e))?;
        let certs_len: usize = certs.len();
        assert_eq!(certs_len, 1, "Should read 1 certificate");
        Ok(())
    }

    #[test]
    fn test_read_cert_and_key_no_cert() -> anyhow::Result<()> {
        let key_data = create_pem("PRIVATE KEY", b"dummy key data");

        let mut cert_file = NamedTempFile::new().context("Failed to create temp cert file")?;
        cert_file
            .write_all(b"not a cert")
            .context("Failed to write to temp cert file")?;

        let mut key_file = NamedTempFile::new().context("Failed to create temp key file")?;
        key_file
            .write_all(key_data.as_bytes())
            .context("Failed to write to temp key file")?;

        let config = TlsConfig {
            cert_path: cert_file.path().to_path_buf(),
            key_path: key_file.path().to_path_buf(),
        };

        let result = read_cert_and_key(&config);
        assert!(result.is_err());
        Ok(())
    }

    #[test]
    fn test_read_cert_and_key_file_not_found() -> anyhow::Result<()> {
        let config = TlsConfig {
            cert_path: "non_existent_cert".into(),
            key_path: "non_existent_key".into(),
        };

        let result = read_cert_and_key(&config);
        assert!(result.is_err());
        Ok(())
    }
}
