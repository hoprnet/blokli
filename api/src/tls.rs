//! TLS configuration and utilities for the blokli API server
//!
//! This module provides TLS 1.3 support using rustls.

use std::{fs, sync::Arc, time::Duration};

use pem_rfc7468::{Decoder, decode_vec};
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
    let mut decoder = Decoder::new(&cert_data)
        .map_err(|e| ApiError::ConfigError(format!("Failed to parse certificate file: {}", e)))?;

    while !decoder.is_finished() {
        let label = decoder.type_label().to_string();
        let mut der_buf = vec![0u8; decoder.remaining_len()];
        let der_slice = decoder
            .decode(&mut der_buf)
            .map_err(|e| ApiError::ConfigError(format!("Failed to decode certificate: {}", e)))?;
        let der = der_slice.to_vec();

        if label == "CERTIFICATE" {
            certs.push(CertificateDer::from(der));
        }
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
    use std::io::Write;

    use base64ct::{Base64, Encoding};
    use tempfile::NamedTempFile;

    use super::*;

    fn create_pem(label: &str, data: &[u8]) -> String {
        let b64 = Base64::encode_string(data);
        format!("-----BEGIN {}-----\n{}\n-----END {}-----", label, b64, label)
    }

    #[test]
    fn test_read_cert_and_key_pkcs8() {
        let cert_data = create_pem("CERTIFICATE", b"dummy cert data");
        let key_data = create_pem("PRIVATE KEY", b"dummy key data");

        let mut cert_file = NamedTempFile::new().unwrap();
        cert_file.write_all(cert_data.as_bytes()).unwrap();

        let mut key_file = NamedTempFile::new().unwrap();
        key_file.write_all(key_data.as_bytes()).unwrap();

        let config = TlsConfig {
            cert_path: cert_file.path().to_path_buf(),
            key_path: key_file.path().to_path_buf(),
        };

        let (certs, key) = read_cert_and_key(&config).expect("Should read cert and key");
        assert_eq!(certs.len(), 1);
        match key {
            PrivateKeyDer::Pkcs8(_) => (),
            _ => panic!("Expected PKCS#8 key"),
        }
    }

    #[test]
    fn test_read_cert_and_key_pkcs1() {
        let cert_data = create_pem("CERTIFICATE", b"dummy cert data");
        let key_data = create_pem("RSA PRIVATE KEY", b"dummy key data");

        let mut cert_file = NamedTempFile::new().unwrap();
        cert_file.write_all(cert_data.as_bytes()).unwrap();

        let mut key_file = NamedTempFile::new().unwrap();
        key_file.write_all(key_data.as_bytes()).unwrap();

        let config = TlsConfig {
            cert_path: cert_file.path().to_path_buf(),
            key_path: key_file.path().to_path_buf(),
        };

        let (certs, key) = read_cert_and_key(&config).expect("Should read cert and key");
        assert_eq!(certs.len(), 1);
        match key {
            PrivateKeyDer::Pkcs1(_) => (),
            _ => panic!("Expected PKCS#1 key"),
        }
    }

    #[test]
    fn test_read_cert_and_key_multiple_certs() {
        let cert1_data = create_pem("CERTIFICATE", b"cert 1");
        let cert2_data = create_pem("CERTIFICATE", b"cert 2");
        let cert_data = format!("{}\n{}", cert1_data, cert2_data);
        let key_data = create_pem("PRIVATE KEY", b"dummy key data");

        let mut cert_file = NamedTempFile::new().unwrap();
        cert_file.write_all(cert_data.as_bytes()).unwrap();

        let mut key_file = NamedTempFile::new().unwrap();
        key_file.write_all(key_data.as_bytes()).unwrap();

        let config = TlsConfig {
            cert_path: cert_file.path().to_path_buf(),
            key_path: key_file.path().to_path_buf(),
        };

        let (certs, _key) = read_cert_and_key(&config).expect("Should read cert and key");
        assert_eq!(certs.len(), 2, "Should read 2 certificates");
    }

    #[test]
    fn test_read_committed_cert_and_key() {
        let mut cert_path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        cert_path.push("tests/fixtures/tls/cert.pem");
        let mut key_path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        key_path.push("tests/fixtures/tls/key.pem");

        let config = TlsConfig { cert_path, key_path };

        let (certs, key) = read_cert_and_key(&config).expect("Should read committed cert and key");
        assert_eq!(certs.len(), 1, "Should read 1 certificate");
        match key {
            PrivateKeyDer::Pkcs8(_) => (),
            _ => panic!("Expected PKCS#8 key from committed fixture"),
        }
    }

    #[test]
    fn test_read_cert_and_key_no_cert() {
        let key_data = create_pem("PRIVATE KEY", b"dummy key data");

        let mut cert_file = NamedTempFile::new().unwrap();
        cert_file.write_all(b"not a cert").unwrap();

        let mut key_file = NamedTempFile::new().unwrap();
        key_file.write_all(key_data.as_bytes()).unwrap();

        let config = TlsConfig {
            cert_path: cert_file.path().to_path_buf(),
            key_path: key_file.path().to_path_buf(),
        };

        let result = read_cert_and_key(&config);
        assert!(result.is_err());
    }

    #[test]
    fn test_read_cert_and_key_file_not_found() {
        let config = TlsConfig {
            cert_path: "non_existent_cert".into(),
            key_path: "non_existent_key".into(),
        };

        let result = read_cert_and_key(&config);
        assert!(result.is_err());
    }
}
