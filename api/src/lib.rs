//! blokli-api - GraphQL API server for HOPR blokli indexer
//!
//! This crate provides a GraphQL API server built with Axum and async-graphql,
//! supporting HTTP/2 and Server-Sent Events (SSE) for subscriptions.

pub mod config;
pub mod conversions;
pub mod errors;
pub mod mutation;
pub mod query;
pub mod schema;
pub mod server;
pub mod subscription;
pub mod tls;
pub mod validation;

use std::sync::Arc;

use axum::serve;
use blokli_chain_api::{
    rpc_adapter::RpcAdapter,
    transaction_executor::{RawTransactionExecutor, RawTransactionExecutorConfig},
    transaction_store::TransactionStore,
    transaction_validator::TransactionValidator,
};
use blokli_chain_rpc::{
    client::DefaultRetryPolicy,
    rpc::{RpcOperations, RpcOperationsConfig},
    transport::ReqwestClient,
};
use config::ApiConfig;
use errors::{ApiError, ApiResult};
use sea_orm::Database;
use tokio::net::TcpListener;
use tracing::{info, warn};

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

    // Create a default IndexerState for standalone API server
    // This is only used for subscription coordination, not for actual indexing
    // Use small buffer sizes since no events will flow through in standalone mode
    let indexer_state = blokli_chain_indexer::IndexerState::new(16, 16);

    // Create stub transaction components for standalone mode
    // These are required for the GraphQL schema but won't be used since mutations
    // are typically called through bloklid, not standalone API
    warn!("Running in standalone mode - transaction mutations will not work without bloklid");

    let transaction_store = Arc::new(TransactionStore::new());
    let transaction_validator = Arc::new(TransactionValidator::new());

    // Create RPC connection for balance queries
    info!("Connecting to RPC: {}", redact_url(&config.rpc_url));
    let rpc_url = url::Url::parse(&config.rpc_url)
        .map_err(|e| ApiError::ConfigError(format!("Failed to parse RPC URL '{}': {}", config.rpc_url, e)))?;
    let transport_client = alloy::transports::http::ReqwestTransport::new(rpc_url);
    let rpc_client = alloy::rpc::client::ClientBuilder::default()
        .layer(alloy::transports::layers::RetryBackoffLayer::new_with_policy(
            2,
            100,
            100,
            DefaultRetryPolicy::default(),
        ))
        .transport(transport_client.clone(), transport_client.guess_local());

    let rpc_operations = RpcOperations::new(
        rpc_client.clone(),
        ReqwestClient::new(),
        RpcOperationsConfig {
            chain_id: config.chain_id,
            contract_addrs: config.contract_addresses,
            ..Default::default()
        },
        None,
    )
    .expect("Failed to create RPC operations");

    let rpc_adapter = Arc::new(RpcAdapter::new(rpc_operations.clone()));

    let transaction_executor = Arc::new(RawTransactionExecutor::with_shared_dependencies(
        rpc_adapter,
        transaction_store.clone(),
        transaction_validator,
        RawTransactionExecutorConfig::default(),
    ));

    // Build the application
    let app = server::build_app(
        db,
        network,
        config.clone(),
        indexer_state,
        transaction_executor,
        transaction_store,
        Arc::new(rpc_operations),
    )
    .await?;

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
