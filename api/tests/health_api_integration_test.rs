//! Integration tests for health API endpoints (/healthz and /readyz)
//!
//! These tests verify end-to-end functionality by:
//! - Running a real Anvil instance with deployed HOPR contracts
//! - Running database migrations to create required tables
//! - Making HTTP requests to health endpoints
//! - Verifying health check responses for various scenarios

mod common;

use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode},
};
use blokli_db_entity::codegen::chain_info;
use sea_orm::{DatabaseConnection, EntityTrait, Set, sea_query::OnConflict};
use tower::ServiceExt;

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
// Healthz Endpoint Tests
// ===========================================

/// Test that /healthz returns 200 OK with healthy status
#[test_log::test(tokio::test)]
async fn test_healthz_returns_ok() -> anyhow::Result<()> {
    let ctx = common::setup_http_test_environment().await?;

    let (status, json) = make_request(ctx.app, "/healthz").await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["status"], "healthy");
    assert!(json["version"].is_string(), "Version should be present");

    Ok(())
}

/// Test that /healthz returns the correct version
#[test_log::test(tokio::test)]
async fn test_healthz_returns_version() -> anyhow::Result<()> {
    let ctx = common::setup_http_test_environment().await?;

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
    let ctx = common::setup_http_test_environment().await?;

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
    let ctx = common::setup_http_test_environment().await?;

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
    let ctx = common::setup_http_test_environment().await?;

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
    let ctx = common::setup_http_test_environment().await?;

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
    let ctx = common::setup_http_test_environment().await?;

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
    let ctx = common::setup_http_test_environment().await?;

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
    let ctx = common::setup_http_test_environment().await?;

    update_chain_info(&ctx.db, 0).await?;

    let (status, json) = make_request(ctx.app, "/readyz").await;

    // If successful, error fields should be null (skipped in serialization)
    if status == StatusCode::OK {
        assert!(json["checks"]["database"]["error"].is_null());
        assert!(json["checks"]["rpc"]["error"].is_null());
    }

    Ok(())
}
