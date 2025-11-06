//! blokli-api - GraphQL API server for HOPR blokli indexer
//!
//! This crate provides a GraphQL API server built with Axum and async-graphql,
//! supporting HTTP/2 and Server-Sent Events (SSE) for subscriptions.

pub mod config;
pub mod conversions;
pub mod errors;
pub mod query;
pub mod schema;
pub mod server;
pub mod subscription;
pub mod tls;
pub mod validation;

use axum::serve;
use config::ApiConfig;
use errors::ApiResult;
use sea_orm::Database;
use tokio::net::TcpListener;
use tracing::info;

/// Redact credentials from a database URL for safe logging
///
/// Converts URLs like `postgres://user:pass@host/db` to `postgres://***:***@host/db`
fn redact_url(url: &str) -> String {
    if let Some(scheme_end) = url.find("://") {
        let scheme = &url[..scheme_end + 3];
        let rest = &url[scheme_end + 3..];

        // Check if there's an @ sign indicating credentials
        if let Some(at_pos) = rest.find('@') {
            let credentials = &rest[..at_pos];
            let after_at = &rest[at_pos..];

            // Redact the credentials part
            if credentials.contains(':') {
                format!("{}***:***{}", scheme, after_at)
            } else {
                format!("{}***{}", scheme, after_at)
            }
        } else {
            // No credentials, return as-is
            url.to_string()
        }
    } else {
        // Not a URL format, return as-is
        url.to_string()
    }
}

/// Start the API server
pub async fn start_server(network: String, config: ApiConfig) -> ApiResult<()> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "blokli_api=info,tower_http=debug".into()),
        )
        .init();

    info!("Starting blokli API server on {}", config.bind_address);
    info!("Connecting to database: {}", redact_url(&config.database_url));

    // Connect to database
    let db = Database::connect(&config.database_url).await?;
    info!("Database connection established");

    // Build the application
    let app = server::build_app(db, network, config.clone()).await?;

    // Create TCP listener
    let listener = TcpListener::bind(config.bind_address).await?;

    let protocol = if config.tls.is_some() { "https" } else { "http" };

    info!("GraphQL endpoint: {}://{}/graphql", protocol, config.bind_address);
    if config.playground_enabled {
        info!("GraphQL Playground: {}://{}/graphql", protocol, config.bind_address);
    }
    info!("Health check: {}://{}/health", protocol, config.bind_address);

    // Start the server with TLS if configured
    if let Some(tls_config) = config.tls {
        info!("Starting server with TLS 1.3");
        let tls_acceptor = tls::create_tls_acceptor(&tls_config)?;
        let tls_listener = tls::TlsListener::new(tls_acceptor, listener);

        serve(tls_listener, app).await?;
    } else {
        info!("Starting server without TLS (HTTP only)");
        serve(listener, app).await?;
    }

    Ok(())
}
