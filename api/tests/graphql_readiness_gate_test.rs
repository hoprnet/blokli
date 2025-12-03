//! Integration tests for GraphQL readiness gating
//!
//! These tests verify that the GraphQL API is only usable when the server
//! is ready (i.e., when /readyz returns 200 OK). The GraphQL endpoint should
//! return 503 Service Unavailable while the indexer is catching up.

use std::{sync::Arc, time::Duration};

use alloy::{
    node_bindings::AnvilInstance,
    rpc::client::ClientBuilder,
    transports::{http::ReqwestTransport, layers::RetryBackoffLayer},
};
use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode},
};
use blokli_api::{
    config::{ApiConfig, HealthConfig},
    server::build_app,
};
use blokli_chain_api::{
    rpc_adapter::RpcAdapter,
    transaction_executor::{RawTransactionExecutor, RawTransactionExecutorConfig},
    transaction_store::TransactionStore,
    transaction_validator::TransactionValidator,
};
use blokli_chain_indexer::IndexerState;
use blokli_chain_rpc::{
    client::{DefaultRetryPolicy, create_rpc_client_to_anvil},
    rpc::{RpcOperations, RpcOperationsConfig},
    transport::ReqwestClient,
};
use blokli_chain_types::{ContractAddresses, ContractInstances, utils::create_anvil};
use blokli_db_entity::codegen::chain_info;
use hopr_crypto_types::keypairs::{ChainKeypair, Keypair};
use migration::{Migrator, MigratorTrait};
use sea_orm::{Database, DatabaseConnection, EntityTrait, Set, sea_query::OnConflict};
use serde_json::json;
use tower::ServiceExt;

/// Test context containing all components needed for GraphQL readiness testing
struct TestContext {
    /// Anvil instance (must be kept alive)
    _anvil: AnvilInstance,
    /// Axum router with GraphQL endpoint
    app: Router,
    /// Database connection for test data manipulation
    db: DatabaseConnection,
    /// Chain ID from Anvil
    _chain_id: u64,
}

/// Setup test environment with Anvil, contracts, migrations, and HTTP router.
async fn setup_test_environment() -> anyhow::Result<TestContext> {
    let _ = env_logger::builder().is_test(true).try_init();

    // Start Anvil with 1-second block time for fast testing
    let expected_block_time = Duration::from_secs(1);
    let anvil = create_anvil(Some(expected_block_time));

    // Create test accounts from Anvil's deterministic keys
    let test_accounts =
        vec![ChainKeypair::from_secret(anvil.keys()[0].to_bytes().as_ref()).expect("Failed to create test account 0")];

    // Deploy HOPR contracts
    let contract_instances = {
        let client = create_rpc_client_to_anvil(&anvil, &test_accounts[0]);
        ContractInstances::deploy_for_testing(client, &test_accounts[0])
            .await
            .expect("Failed to deploy contracts")
    };

    let contract_addrs = ContractAddresses::from(&contract_instances);

    // Wait for contract deployments to be final
    tokio::time::sleep((1 + 2) * expected_block_time).await;

    // Setup in-memory SQLite database and run migrations
    let db = Database::connect("sqlite::memory:")
        .await
        .expect("Failed to create test database");

    // Run migrations to create chain_info table
    Migrator::up(&db, None).await.expect("Failed to run migrations");

    // Create RPC operations for health checks
    let transport_client = ReqwestTransport::new(anvil.endpoint_url());
    let rpc_client = ClientBuilder::default()
        .layer(RetryBackoffLayer::new_with_policy(
            2,
            100,
            100,
            DefaultRetryPolicy::default(),
        ))
        .transport(transport_client.clone(), transport_client.guess_local());

    let chain_id = 31337; // Anvil default chain ID

    let rpc_operations = Arc::new(
        RpcOperations::new(
            rpc_client.clone(),
            ReqwestClient::new(),
            RpcOperationsConfig {
                chain_id,
                contract_addrs,
                expected_block_time,
                ..Default::default()
            },
            None,
        )
        .expect("Failed to create RPC operations"),
    );

    // Create stub transaction components
    let transaction_store = Arc::new(TransactionStore::new());
    let transaction_validator = Arc::new(TransactionValidator::new());
    let rpc_adapter = Arc::new(RpcAdapter::new(
        RpcOperations::new(
            rpc_client,
            ReqwestClient::new(),
            RpcOperationsConfig {
                chain_id,
                contract_addrs,
                expected_block_time,
                ..Default::default()
            },
            None,
        )
        .expect("Failed to create RPC adapter operations"),
    ));

    let transaction_executor = Arc::new(RawTransactionExecutor::with_shared_dependencies(
        rpc_adapter,
        transaction_store.clone(),
        transaction_validator,
        RawTransactionExecutorConfig::default(),
    ));

    // Create IndexerState for subscriptions (with small buffers)
    let indexer_state = IndexerState::new(1, 1);

    // Create API config with health settings
    let api_config = ApiConfig {
        playground_enabled: false,
        chain_id,
        contract_addresses: contract_addrs,
        health: HealthConfig {
            max_indexer_lag: 10,
            timeout: std::time::Duration::from_millis(5000),
            readiness_check_interval: std::time::Duration::from_millis(100), // Fast updates for tests
        },
        ..Default::default()
    };

    // Build HTTP router
    let app = build_app(
        db.clone(),
        "test-network".to_string(),
        api_config,
        indexer_state,
        transaction_executor,
        transaction_store,
        rpc_operations,
        None, // SQLite notification manager not needed for tests
    )
    .await
    .expect("Failed to build app");

    Ok(TestContext {
        _anvil: anvil,
        app,
        db,
        _chain_id: chain_id,
    })
}

/// Helper to make HTTP POST request to GraphQL endpoint
async fn make_graphql_request(app: Router, query: &str) -> (StatusCode, serde_json::Value) {
    let request_body = json!({
        "query": query
    });

    let request = Request::builder()
        .method("POST")
        .uri("/graphql")
        .header("content-type", "application/json")
        .body(Body::from(
            serde_json::to_string(&request_body).expect("Failed to serialize JSON"),
        ))
        .expect("Failed to build request");

    let response = app.oneshot(request).await.expect("Failed to execute request");
    let status = response.status();

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("Failed to read body");

    let json: serde_json::Value = serde_json::from_slice(&body).expect("Failed to parse JSON");

    (status, json)
}

/// Update chain_info record with specified block number
async fn update_chain_info(db: &DatabaseConnection, block_number: i64) -> anyhow::Result<()> {
    let chain_info = chain_info::ActiveModel {
        id: Set(1),
        last_indexed_block: Set(block_number),
        last_indexed_tx_index: Set(Some(0)),
        last_indexed_log_index: Set(Some(0)),
        min_incoming_ticket_win_prob: Set(0.0),
        ..Default::default()
    };

    chain_info::Entity::insert(chain_info)
        .on_conflict(
            OnConflict::column(chain_info::Column::Id)
                .update_column(chain_info::Column::LastIndexedBlock)
                .to_owned(),
        )
        .exec(db)
        .await?;

    Ok(())
}

/// Delete chain_info record to simulate indexer not started
async fn delete_chain_info(db: &DatabaseConnection) -> anyhow::Result<()> {
    chain_info::Entity::delete_many().exec(db).await?;
    Ok(())
}

// ===========================================
// GraphQL Readiness Gate Tests
// ===========================================

/// Test that GraphQL returns 503 when server is not ready (no chain_info)
#[test_log::test(tokio::test)]
async fn test_graphql_returns_503_when_not_ready() -> anyhow::Result<()> {
    let ctx = setup_test_environment().await?;

    // Delete chain_info to simulate server not ready
    delete_chain_info(&ctx.db).await?;

    // Try to make a simple GraphQL query
    let query = r#"query { __typename }"#;
    let (status, json) = make_graphql_request(ctx.app, query).await;

    // Should return 503 Service Unavailable
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);

    // Should have error message
    assert!(json["errors"].is_array());
    assert_eq!(json["errors"].as_array().unwrap().len(), 1);
    assert!(
        json["errors"][0]["message"].as_str().unwrap().contains("not ready yet"),
        "Error message should mention server not being ready"
    );

    Ok(())
}

/// Test that GraphQL returns 200 when server is ready
#[test_log::test(tokio::test)]
async fn test_graphql_returns_200_when_ready() -> anyhow::Result<()> {
    let ctx = setup_test_environment().await?;

    // Set up chain_info to make server ready
    update_chain_info(&ctx.db, 0).await?;

    // Wait for periodic check to update cached state (interval is 100ms in test config)
    tokio::time::sleep(std::time::Duration::from_millis(150)).await;

    // Try to make a simple GraphQL query
    let query = r#"query { __typename }"#;
    let (status, json) = make_graphql_request(ctx.app, query).await;

    // Should return 200 OK (or at least not 503)
    assert_ne!(status, StatusCode::SERVICE_UNAVAILABLE);
    // Query should succeed (even if it returns empty data, there should be no "not ready" error)
    if json["errors"].is_array() {
        for error in json["errors"].as_array().unwrap() {
            assert!(
                !error["message"].as_str().unwrap_or("").contains("not ready yet"),
                "Should not have 'not ready' error when server is ready"
            );
        }
    }

    Ok(())
}

/// Test that GraphQL error message is clear when indexer is catching up
#[test_log::test(tokio::test)]
async fn test_graphql_error_message_mentions_indexer() -> anyhow::Result<()> {
    let ctx = setup_test_environment().await?;

    // Delete chain_info to simulate server not ready
    delete_chain_info(&ctx.db).await?;

    let query = r#"query { __typename }"#;
    let (status, json) = make_graphql_request(ctx.app, query).await;

    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    let error_message = json["errors"][0]["message"].as_str().unwrap();
    assert!(
        error_message.contains("Indexer") || error_message.contains("indexer") || error_message.contains("catching up"),
        "Error message should mention indexer or catching up"
    );

    Ok(())
}

/// Test that different query types are all blocked when not ready
#[test_log::test(tokio::test)]
async fn test_graphql_all_request_types_blocked_when_not_ready() -> anyhow::Result<()> {
    let ctx = setup_test_environment().await?;

    // Delete chain_info to simulate server not ready
    delete_chain_info(&ctx.db).await?;

    // Test various query types
    let queries = vec![
        r#"query { __typename }"#,
        r#"{ __typename }"#, // Shorthand query
        r#"query TestQuery { __typename }"#,
    ];

    for query in queries {
        let (status, _json) = make_graphql_request(ctx.app.clone(), query).await;
        assert_eq!(
            status,
            StatusCode::SERVICE_UNAVAILABLE,
            "Query '{}' should be blocked when not ready",
            query
        );
    }

    Ok(())
}

/// Test that readiness state transitions from not ready to ready
#[test_log::test(tokio::test)]
async fn test_graphql_readiness_transition() -> anyhow::Result<()> {
    let ctx = setup_test_environment().await?;

    let query = r#"query { __typename }"#;

    // First request: not ready (no chain_info)
    delete_chain_info(&ctx.db).await?;
    let (status1, _) = make_graphql_request(ctx.app.clone(), query).await;
    assert_eq!(status1, StatusCode::SERVICE_UNAVAILABLE);

    // Second request: ready (after updating chain_info)
    update_chain_info(&ctx.db, 0).await?;
    // Wait for periodic check to update cached state (interval is 100ms in test config)
    tokio::time::sleep(std::time::Duration::from_millis(150)).await;
    let (status2, json2) = make_graphql_request(ctx.app, query).await;
    assert_ne!(status2, StatusCode::SERVICE_UNAVAILABLE);

    // Verify no "not ready" error
    if json2["errors"].is_array() {
        for error in json2["errors"].as_array().unwrap() {
            assert!(!error["message"].as_str().unwrap_or("").contains("not ready yet"));
        }
    }

    Ok(())
}

/// Test that /readyz and GraphQL gating are in sync
#[test_log::test(tokio::test)]
async fn test_graphql_readiness_synced_with_readyz() -> anyhow::Result<()> {
    let ctx = setup_test_environment().await?;

    // Helper to check readyz status
    let check_readyz = async |app: &Router| {
        let request = Request::builder()
            .uri("/readyz")
            .body(Body::empty())
            .expect("Failed to build request");

        let response = app.clone().oneshot(request).await.expect("Failed to execute request");
        response.status()
    };

    // Scenario 1: Both should be unavailable when not ready
    delete_chain_info(&ctx.db).await?;
    let readyz_status = check_readyz(&ctx.app).await;
    let (graphql_status, _) = make_graphql_request(ctx.app.clone(), r#"query { __typename }"#).await;

    // Both should indicate not ready (503 for readyz, 503 for GraphQL)
    assert_eq!(
        readyz_status,
        StatusCode::SERVICE_UNAVAILABLE,
        "/readyz should return 503 when not ready"
    );
    assert_eq!(
        graphql_status,
        StatusCode::SERVICE_UNAVAILABLE,
        "GraphQL should return 503 when not ready"
    );

    // Scenario 2: Both should be available when ready
    update_chain_info(&ctx.db, 0).await?;
    let readyz_status = check_readyz(&ctx.app).await;
    let (graphql_status, _) = make_graphql_request(ctx.app, r#"query { __typename }"#).await;

    assert_eq!(readyz_status, StatusCode::OK, "/readyz should return 200 when ready");
    assert_ne!(
        graphql_status,
        StatusCode::SERVICE_UNAVAILABLE,
        "GraphQL should not return 503 when ready"
    );

    Ok(())
}
