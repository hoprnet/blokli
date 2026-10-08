//! blokli-api - GraphQL API server for HOPR blokli indexer
//!
//! This crate provides a GraphQL API server built with Axum and async-graphql,
//! supporting HTTP/2 and Server-Sent Events (SSE) for subscriptions.

pub mod config;
pub mod conversions;
pub mod curvy;
pub mod errors;
pub mod logging;
pub mod metrics;
pub mod mutation;
pub mod query;
pub mod query_v2;
pub mod readiness;
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
    transaction_policy::{TransactionPolicy, network_transaction_filter},
    transaction_store::TransactionStore,
};
use blokli_chain_rpc::{
    client::{DefaultRetryPolicy, MetricsLayer},
    rpc::{RpcOperations, RpcOperationsConfig},
    transport::ReqwestClient,
};
use blokli_chain_types::ContractAddresses;
use blokli_db::{
    db::{BlokliDbConfig, build_connect_options},
    utils::redact_database_url,
};
use blokli_tx::TransactionFilter;
use config::ApiConfig;
use errors::{ApiError, ApiResult};
use hopr_bindings::exports::alloy::{
    rpc::client::ClientBuilder,
    transports::{http::ReqwestTransport, layers::RetryBackoffLayer},
};
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

/// Build the transaction policy for a standalone API server.
///
/// Derives the same allow-set bloklid does, from the configured contract addresses. With none
/// configured there is nothing to derive, so the empty allow-set refuses everything: the executor
/// here is fully functional, and failing open would make it an unrestricted relay.
fn standalone_transaction_policy(contracts: &ContractAddresses) -> TransactionPolicy {
    if contracts == &ContractAddresses::default() {
        warn!("No contract addresses configured - transaction relaying is disabled");
        return TransactionPolicy::Whitelist(TransactionFilter::default());
    }

    TransactionPolicy::Whitelist(network_transaction_filter(contracts))
}

/// Start the API server
pub async fn start_server(network: String, finality: u16, config: ApiConfig) -> ApiResult<()> {
    // Initialize tracing
    logging::setup_tracing_env_like("blokli_api=info,tower_http=debug")
        .map_err(|error| ApiError::ConfigError(format!("Failed to initialize tracing: {error}")))?;

    info!("Starting blokli API server on {}", config.bind_address);
    info!("Connecting to database: {}", redact_database_url(&config.database_url));

    // Connect to database
    let db = Database::connect(build_connect_options(&config.database_url, &BlokliDbConfig::default())).await?;
    info!("Database connection established");

    // Create a default IndexerState for standalone API server
    // This is only used for subscription coordination, not for actual indexing
    // Use small buffer sizes since no events will flow through in standalone mode
    let indexer_state = blokli_chain_indexer::IndexerState::new(16, 16);

    info!("Running in standalone mode - transaction mutations are gated by the configured contract addresses");

    let transaction_store = Arc::new(TransactionStore::new());
    let transaction_policy = Arc::new(standalone_transaction_policy(&config.contract_addresses));

    // Create RPC connection for balance queries
    info!("Connecting to RPC: {}", redact_url(&config.rpc_url));
    let rpc_url = url::Url::parse(&config.rpc_url).map_err(|e| {
        ApiError::ConfigError(format!(
            "Failed to parse RPC URL '{}': {}",
            redact_url(&config.rpc_url),
            e
        ))
    })?;
    let transport_client = ReqwestTransport::new(rpc_url);
    let rpc_client = ClientBuilder::default()
        .layer(RetryBackoffLayer::new_with_policy(
            2,
            100,
            100,
            DefaultRetryPolicy::default(),
        ))
        .layer(MetricsLayer)
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
    .map_err(|e| ApiError::ConfigError(format!("Failed to create RPC operations: {}", e)))?;

    let rpc_adapter = Arc::new(RpcAdapter::new(rpc_operations.clone()));

    let transaction_executor = Arc::new(RawTransactionExecutor::with_shared_dependencies(
        rpc_adapter,
        transaction_store.clone(),
        transaction_policy,
        RawTransactionExecutorConfig::default(),
    ));

    // Build the application
    let app = server::build_app(
        server::ApiDatabases::single(db),
        network,
        config.clone(),
        config.expected_block_time,
        finality,
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
    #[cfg(feature = "telemetry")]
    info!("Metrics endpoint: {}://{}/metrics", protocol, config.bind_address);
    #[cfg(not(feature = "telemetry"))]
    info!("Metrics endpoint disabled (build without telemetry feature)");
    info!("Health check: {}://{}/healthz", protocol, config.bind_address);
    info!("Readiness check: {}://{}/readyz", protocol, config.bind_address);

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

#[cfg(test)]
mod tests {
    use blokli_chain_types::ContractAddresses;
    use blokli_tx::FilterError;
    use hopr_bindings::{
        exports::alloy::{
            consensus::{SignableTransaction, TxEip1559},
            eips::eip2718::Encodable2718,
            primitives::{Address as AlloyAddress, Bytes, TxKind, U256},
            signers::{SignerSync, local::PrivateKeySigner},
            sol_types::SolCall,
        },
        hopr_token::HoprToken::approveCall,
    };
    use hopr_types::primitive::prelude::Address;

    use crate::standalone_transaction_policy;

    const KEY: &str = "ac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80";
    const TOKEN: [u8; 20] = [0x11; 20];

    fn configured_contracts() -> ContractAddresses {
        ContractAddresses {
            token: Address::from(TOKEN),
            ..Default::default()
        }
    }

    /// A signed `approve` on the token contract, the simplest relayable HOPR operation.
    fn signed_token_approve() -> Vec<u8> {
        let mut input = approveCall::SELECTOR.to_vec();
        input.extend_from_slice(&[0u8; 64]);

        let tx = TxEip1559 {
            chain_id: 1,
            nonce: 0,
            gas_limit: 21_000,
            max_fee_per_gas: 1_000_000_000,
            max_priority_fee_per_gas: 1_000_000_000,
            to: TxKind::Call(AlloyAddress::from(TOKEN)),
            value: U256::ZERO,
            access_list: Default::default(),
            input: Bytes::from(input),
        };
        let signer: PrivateKeySigner = KEY.parse().expect("valid private key");
        let signature = signer.sign_hash_sync(&tx.signature_hash()).expect("sign tx");

        let mut raw = Vec::new();
        tx.into_signed(signature).encode_2718(&mut raw);
        raw
    }

    #[test]
    fn standalone_policy_enforces_the_network_allow_set_when_contracts_are_configured() {
        let policy = standalone_transaction_policy(&configured_contracts());
        assert!(policy.check(&signed_token_approve()).is_ok());
    }

    #[test]
    fn standalone_policy_rejects_calls_outside_the_network_allow_set() {
        let policy = standalone_transaction_policy(&configured_contracts());
        let raw = signed_token_approve();
        // Same calldata, a contract the network does not deploy.
        let unknown = ContractAddresses {
            token: Address::from([0x99u8; 20]),
            ..Default::default()
        };

        assert!(matches!(
            standalone_transaction_policy(&unknown).check(&raw),
            Err(FilterError::ContractNotAllowed { .. })
        ));
        assert!(policy.check(&raw).is_ok());
    }

    #[test]
    fn standalone_policy_relays_nothing_when_no_contracts_are_configured() {
        // Failing open here would make the standalone server an open relay for any signed
        // transaction, contract creation included.
        let policy = standalone_transaction_policy(&ContractAddresses::default());

        assert!(matches!(
            policy.check(&signed_token_approve()),
            Err(FilterError::ContractNotAllowed { .. })
        ));
    }
}
