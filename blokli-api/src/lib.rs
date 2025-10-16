//! blokli-api - GraphQL API server for HOPR blokli indexer
//!
//! This crate provides a GraphQL API server built with Axum and async-graphql,
//! supporting HTTP/2 and Server-Sent Events (SSE) for subscriptions.

pub mod config;
pub mod errors;
pub mod query;
pub mod schema;
pub mod server;
pub mod tls;
pub mod types;

use axum::serve;
use config::ApiConfig;
use errors::ApiResult;
use sea_orm::Database;
use tokio::net::TcpListener;
use tracing::info;

/// Start the API server
pub async fn start_server(config: ApiConfig) -> ApiResult<()> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "blokli_api=info,tower_http=debug".into()),
        )
        .init();

    info!("Starting blokli API server on {}", config.bind_address);
    info!("Connecting to database: {}", config.database_url);

    // Connect to database
    let db = Database::connect(&config.database_url).await?;
    info!("Database connection established");

    // Build the application
    let app = server::build_app(db).await?;

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
