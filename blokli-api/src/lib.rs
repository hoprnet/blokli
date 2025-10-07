//! blokli-api - GraphQL API server for HOPR blokli indexer
//!
//! This crate provides a GraphQL API server built with Axum and async-graphql,
//! supporting HTTP/2 and Server-Sent Events (SSE) for subscriptions.

pub mod config;
pub mod errors;
pub mod schema;
pub mod server;

use axum::serve;
use config::ApiConfig;
use errors::ApiResult;
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

    // Build the application
    let app = server::build_app();

    // Create TCP listener
    let listener = TcpListener::bind(config.bind_address).await?;

    info!("GraphQL endpoint: http://{}/graphql", config.bind_address);
    if config.playground_enabled {
        info!("GraphQL Playground: http://{}/graphql", config.bind_address);
    }
    info!("GraphQL Subscriptions (SSE): http://{}/graphql/subscriptions", config.bind_address);
    info!("Health check: http://{}/health", config.bind_address);

    // Start the server with HTTP/2 support
    serve(listener, app)
        .await?;

    Ok(())
}
