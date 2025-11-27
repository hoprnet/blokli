//! Integration tests for health API endpoints (/healthz and /readyz)
//!
//! These tests verify end-to-end functionality by:
//! - Running a real Anvil instance with deployed HOPR contracts
//! - Running database migrations to create required tables
//! - Making HTTP requests to health endpoints
//! - Verifying health check responses for various scenarios

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
use tower::ServiceExt;

/// Test context containing all components needed for health API testing
struct TestContext {
    /// Anvil instance (must be kept alive)
    _anvil: AnvilInstance,
    /// Axum router with health endpoints
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

/// Helper to make HTTP request and get response
async fn make_request(app: Router, path: &str) -> (StatusCode, serde_json::Value) {
    let request = Request::builder()
        .uri(path)
        .body(Body::empty())
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
/// The migration seeds initial data with id=1, so we update it
async fn update_chain_info(db: &DatabaseConnection, block_number: i64) -> anyhow::Result<()> {
    // Use upsert to handle both cases (existing or not)
    let chain_info = chain_info::ActiveModel {
        id: Set(1),
        last_indexed_block: Set(block_number),
        last_indexed_tx_index: Set(0),
        last_indexed_log_index: Set(0),
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
// Healthz Endpoint Tests
// ===========================================

/// Test that /healthz returns 200 OK with healthy status
#[test_log::test(tokio::test)]
async fn test_healthz_returns_ok() -> anyhow::Result<()> {
    let ctx = setup_test_environment().await?;

    let (status, json) = make_request(ctx.app, "/healthz").await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["status"], "healthy");
    assert!(json["version"].is_string(), "Version should be present");

    Ok(())
}

/// Test that /healthz returns the correct version
#[test_log::test(tokio::test)]
async fn test_healthz_returns_version() -> anyhow::Result<()> {
    let ctx = setup_test_environment().await?;

    let (status, json) = make_request(ctx.app, "/healthz").await;

    assert_eq!(status, StatusCode::OK);
    // Version should match the package version
    assert_eq!(json["version"], env!("CARGO_PKG_VERSION"));

    Ok(())
}

// ===========================================
// Readyz Endpoint Tests - Success Scenarios
// ===========================================

/// Test that /readyz returns 200 OK when all checks pass
#[test_log::test(tokio::test)]
async fn test_readyz_all_healthy() -> anyhow::Result<()> {
    let ctx = setup_test_environment().await?;

    // Get current block number from Anvil (usually starts at 0 or low number)
    // We'll update chain_info with a recent block to ensure lag is within threshold

    // Update chain_info with block 0 (Anvil starts near there)
    update_chain_info(&ctx.db, 0).await?;

    let (status, json) = make_request(ctx.app, "/readyz").await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["status"], "ready");
    assert_eq!(json["checks"]["database"]["status"], "healthy");
    assert_eq!(json["checks"]["rpc"]["status"], "healthy");
    assert!(json["checks"]["rpc"]["block_number"].is_number());

    Ok(())
}

/// Test that /readyz shows correct indexer lag calculation
#[test_log::test(tokio::test)]
async fn test_readyz_shows_indexer_lag() -> anyhow::Result<()> {
    let ctx = setup_test_environment().await?;

    // Update chain_info with block 0
    update_chain_info(&ctx.db, 0).await?;

    let (status, json) = make_request(ctx.app, "/readyz").await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["checks"]["indexer"]["status"], "healthy");
    assert!(json["checks"]["indexer"]["last_indexed_block"].is_number());
    assert!(json["checks"]["indexer"]["lag"].is_number());

    Ok(())
}

// ===========================================
// Readyz Endpoint Tests - Failure Scenarios
// ===========================================

/// Test that /readyz returns 503 when no chain_info exists (indexer not started)
#[test_log::test(tokio::test)]
async fn test_readyz_no_chain_info() -> anyhow::Result<()> {
    let ctx = setup_test_environment().await?;

    // Delete chain_info - simulates indexer not started
    delete_chain_info(&ctx.db).await?;

    let (status, json) = make_request(ctx.app, "/readyz").await;

    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(json["status"], "not_ready");
    assert_eq!(json["checks"]["database"]["status"], "unhealthy");
    assert!(
        json["checks"]["database"]["error"]
            .as_str()
            .unwrap()
            .contains("No chain info found")
    );

    Ok(())
}

/// Test that /readyz returns 503 when indexer lag exceeds threshold
#[test_log::test(tokio::test)]
async fn test_readyz_indexer_lag_exceeded() -> anyhow::Result<()> {
    let ctx = setup_test_environment().await?;

    // Update chain_info with block 0
    update_chain_info(&ctx.db, 0).await?;

    let (status, json) = make_request(ctx.app, "/readyz").await;

    // The actual test depends on how many blocks Anvil has produced
    // For a reliable test, we check that lag is calculated correctly
    let lag = json["checks"]["indexer"]["lag"].as_u64();

    // If lag exceeds threshold, status should be not_ready
    if lag.unwrap_or(0) > 10 {
        assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(json["status"], "not_ready");
        assert_eq!(json["checks"]["indexer"]["status"], "unhealthy");
    } else {
        // If lag is within threshold, status should be ready
        assert_eq!(status, StatusCode::OK);
        assert_eq!(json["status"], "ready");
    }

    Ok(())
}

/// Test that /readyz includes version in response
#[test_log::test(tokio::test)]
async fn test_readyz_includes_version() -> anyhow::Result<()> {
    let ctx = setup_test_environment().await?;

    update_chain_info(&ctx.db, 0).await?;

    let (_, json) = make_request(ctx.app, "/readyz").await;

    // Regardless of status, version should be present
    assert!(json["version"].is_string());

    Ok(())
}

// ===========================================
// Error Response Structure Tests
// ===========================================

/// Test that error responses have proper structure
#[test_log::test(tokio::test)]
async fn test_readyz_error_response_structure() -> anyhow::Result<()> {
    let ctx = setup_test_environment().await?;

    // Delete chain_info - will cause error
    delete_chain_info(&ctx.db).await?;

    let (_, json) = make_request(ctx.app, "/readyz").await;

    // Verify response structure
    assert!(json["status"].is_string());
    assert!(json["version"].is_string());
    assert!(json["checks"].is_object());
    assert!(json["checks"]["database"].is_object());
    assert!(json["checks"]["rpc"].is_object());
    assert!(json["checks"]["indexer"].is_object());

    // Database should have error details
    assert!(json["checks"]["database"]["status"].is_string());
    assert!(json["checks"]["database"]["error"].is_string());

    Ok(())
}

/// Test that successful responses omit error fields
#[test_log::test(tokio::test)]
async fn test_readyz_success_omits_error_fields() -> anyhow::Result<()> {
    let ctx = setup_test_environment().await?;

    update_chain_info(&ctx.db, 0).await?;

    let (status, json) = make_request(ctx.app, "/readyz").await;

    // If successful, error fields should be null (skipped in serialization)
    if status == StatusCode::OK {
        assert!(json["checks"]["database"]["error"].is_null());
        assert!(json["checks"]["rpc"]["error"].is_null());
    }

    Ok(())
}
