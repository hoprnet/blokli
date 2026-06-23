//! Integration tests for `X-Blokli-Schema-Version` header routing.
//!
//! Tests cover two scenarios:
//! - Basic routing: the correct schema is selected from the header value.
//! - Composition routing: v2 only overrides `query1`; `query2` lives in `SharedQuery` and is resolved identically
//!   regardless of the version header.

use std::{collections::HashMap, sync::Arc};

use async_graphql::{EmptyMutation, EmptySubscription, MergedObject, Object, Schema};
use axum::{
    body::Body,
    http::{Request, StatusCode, header},
};
use blokli_api::{
    config::{HealthConfig, SseKeepAliveConfig},
    readiness::ReadinessChecker,
    schema::ErasedSchema,
    server::{AppState, build_test_router},
};
use blokli_chain_rpc::{
    rpc::{RpcOperations, RpcOperationsConfig},
    transport::ReqwestClient,
};
use hopr_bindings::exports::alloy::{rpc::client::ClientBuilder, transports::http::ReqwestTransport};
use sea_orm::Database;
use tower::ServiceExt;

// ---------------------------------------------------------------------------
// Minimal test schemas
// ---------------------------------------------------------------------------

#[derive(Default)]
struct V1Query;

#[Object]
impl V1Query {
    async fn schema_version(&self) -> &'static str {
        "v1"
    }
}

#[derive(Default)]
struct V2Query;

#[Object]
impl V2Query {
    async fn schema_version(&self) -> &'static str {
        "v2"
    }

    /// Field that does not exist in v1 — querying it against v1 returns a validation error.
    async fn v2_only_field(&self) -> &'static str {
        "only-in-v2"
    }
}

// ---------------------------------------------------------------------------
// Test helpers
// ---------------------------------------------------------------------------

fn build_test_schemas() -> (HashMap<u32, Arc<dyn ErasedSchema>>, Arc<dyn ErasedSchema>) {
    let v1: Arc<dyn ErasedSchema> = Arc::new(Schema::build(V1Query, EmptyMutation, EmptySubscription).finish());
    let v2: Arc<dyn ErasedSchema> = Arc::new(Schema::build(V2Query, EmptyMutation, EmptySubscription).finish());
    let mut schemas = HashMap::new();
    schemas.insert(1u32, v1.clone());
    schemas.insert(2u32, v2);
    (schemas, v1)
}

async fn build_test_app_state() -> AppState {
    let db = Database::connect("sqlite::memory:").await.expect("in-memory SQLite");

    let transport = ReqwestTransport::new("http://localhost:8545".parse().unwrap());
    let rpc_client = ClientBuilder::default().transport(transport.clone(), transport.guess_local());
    let transport_client = ReqwestClient::new();
    let rpc_operations = Arc::new(
        RpcOperations::new(rpc_client, transport_client, RpcOperationsConfig::default(), None)
            .expect("dummy RpcOperations"),
    );

    let readiness_checker = ReadinessChecker::new(db.clone(), rpc_operations.clone(), HealthConfig::default());
    readiness_checker.force_ready().await;

    let (schemas, introspection_schema) = build_test_schemas();

    AppState {
        schemas: Arc::new(schemas),
        latest_schema_version: 2,
        introspection_schema,
        playground_enabled: false,
        db,
        rpc_operations,
        health_config: HealthConfig::default(),
        readiness_checker,
        sse_keepalive: SseKeepAliveConfig::default(),
    }
}

fn post_graphql(query: &str, version_header: Option<&str>) -> Request<Body> {
    let body = serde_json::json!({ "query": query }).to_string();
    let mut builder = Request::builder()
        .method("POST")
        .uri("/graphql")
        .header(header::CONTENT_TYPE, "application/json");
    if let Some(v) = version_header {
        builder = builder.header("x-blokli-schema-version", v);
    }
    builder.body(Body::from(body)).unwrap()
}

async fn json_body(resp: axum::response::Response) -> serde_json::Value {
    let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    serde_json::from_slice(&bytes).unwrap()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

/// No header → defaults to v1.
#[tokio::test]
async fn no_version_header_routes_to_v1() {
    let app = build_test_router(build_test_app_state().await);
    let resp = app.oneshot(post_graphql("{ schemaVersion }", None)).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = json_body(resp).await;
    assert_eq!(body["data"]["schemaVersion"], "v1");
}

/// Explicit `X-Blokli-Schema-Version: 1` → v1.
#[tokio::test]
async fn explicit_v1_header_routes_to_v1() {
    let app = build_test_router(build_test_app_state().await);
    let resp = app.oneshot(post_graphql("{ schemaVersion }", Some("1"))).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = json_body(resp).await;
    assert_eq!(body["data"]["schemaVersion"], "v1");
}

/// `X-Blokli-Schema-Version: 2` → v2.
#[tokio::test]
async fn v2_header_routes_to_v2() {
    let app = build_test_router(build_test_app_state().await);
    let resp = app.oneshot(post_graphql("{ schemaVersion }", Some("2"))).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = json_body(resp).await;
    assert_eq!(body["data"]["schemaVersion"], "v2");
}

/// A field that only exists in v2 is accessible when requesting v2 …
#[tokio::test]
async fn v2_only_field_succeeds_against_v2() {
    let app = build_test_router(build_test_app_state().await);
    let resp = app.oneshot(post_graphql("{ v2OnlyField }", Some("2"))).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = json_body(resp).await;
    assert_eq!(body["data"]["v2OnlyField"], "only-in-v2");
}

/// … but fails validation when the client omits the header (routes to v1).
#[tokio::test]
async fn v2_only_field_fails_validation_against_v1() {
    let app = build_test_router(build_test_app_state().await);
    let resp = app.oneshot(post_graphql("{ v2OnlyField }", None)).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK); // GraphQL returns 200 with errors[]
    let body = json_body(resp).await;
    assert!(body["errors"].is_array(), "expected GraphQL validation errors");
    assert!(body["data"].is_null() || body["data"]["v2OnlyField"].is_null());
}

/// An unknown version returns 400 Bad Request.
#[tokio::test]
async fn unknown_version_returns_400() {
    let app = build_test_router(build_test_app_state().await);
    let resp = app
        .oneshot(post_graphql("{ schemaVersion }", Some("99")))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    let body = json_body(resp).await;
    assert!(body["errors"][0]["message"].as_str().unwrap().contains("99"));
}

/// A non-numeric version header is treated the same as no header (defaults to v1).
#[tokio::test]
async fn malformed_version_header_defaults_to_v1() {
    let app = build_test_router(build_test_app_state().await);
    let resp = app
        .oneshot(post_graphql("{ schemaVersion }", Some("not-a-number")))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = json_body(resp).await;
    assert_eq!(body["data"]["schemaVersion"], "v1");
}

// ---------------------------------------------------------------------------
// Composition tests — MergedObject pattern
//
// v1 = MergedV1(Query1V1, SharedQuery)
// v2 = MergedV2(Query1V2, SharedQuery)   ← only query1 changed
//
// query1 must return different values per version; query2 must return the same
// value regardless of which version header the client sends.
// ---------------------------------------------------------------------------

/// Resolver for `query1` in v1.
#[derive(Default)]
struct Query1V1;

#[Object]
impl Query1V1 {
    async fn query1(&self) -> &'static str {
        "v1-result"
    }
}

/// Resolver for `query1` in v2 (breaking change: new implementation).
#[derive(Default)]
struct Query1V2;

#[Object]
impl Query1V2 {
    async fn query1(&self) -> &'static str {
        "v2-result"
    }
}

/// Shared resolver — identical across all versions.
#[derive(Default)]
struct SharedQuery;

#[Object]
impl SharedQuery {
    async fn query2(&self) -> &'static str {
        "shared-result"
    }
}

/// v1 schema: query1 (v1 impl) + query2 (shared).
#[derive(MergedObject, Default)]
struct MergedV1(Query1V1, SharedQuery);

/// v2 schema: query1 (v2 impl) + query2 (shared).
#[derive(MergedObject, Default)]
struct MergedV2(Query1V2, SharedQuery);

fn build_composed_test_schemas() -> (HashMap<u32, Arc<dyn ErasedSchema>>, Arc<dyn ErasedSchema>) {
    let v1: Arc<dyn ErasedSchema> =
        Arc::new(Schema::build(MergedV1::default(), EmptyMutation, EmptySubscription).finish());
    let v2: Arc<dyn ErasedSchema> =
        Arc::new(Schema::build(MergedV2::default(), EmptyMutation, EmptySubscription).finish());
    let mut schemas = HashMap::new();
    schemas.insert(1u32, v1.clone());
    schemas.insert(2u32, v2);
    (schemas, v1)
}

async fn build_composed_app_state() -> AppState {
    let db = Database::connect("sqlite::memory:").await.expect("in-memory SQLite");
    let transport = ReqwestTransport::new("http://localhost:8545".parse().unwrap());
    let rpc_client = ClientBuilder::default().transport(transport.clone(), transport.guess_local());
    let transport_client = ReqwestClient::new();
    let rpc_operations = Arc::new(
        RpcOperations::new(rpc_client, transport_client, RpcOperationsConfig::default(), None)
            .expect("dummy RpcOperations"),
    );
    let readiness_checker = ReadinessChecker::new(db.clone(), rpc_operations.clone(), HealthConfig::default());
    readiness_checker.force_ready().await;

    let (schemas, introspection_schema) = build_composed_test_schemas();
    AppState {
        schemas: Arc::new(schemas),
        latest_schema_version: 2,
        introspection_schema,
        playground_enabled: false,
        db,
        rpc_operations,
        health_config: HealthConfig::default(),
        readiness_checker,
        sse_keepalive: SseKeepAliveConfig::default(),
    }
}

/// query1 with header 1 → v1 implementation.
#[tokio::test]
async fn composed_query1_routes_to_v1_impl() {
    let app = build_test_router(build_composed_app_state().await);
    let resp = app.oneshot(post_graphql("{ query1 }", Some("1"))).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(json_body(resp).await["data"]["query1"], "v1-result");
}

/// query1 with header 2 → v2 implementation.
#[tokio::test]
async fn composed_query1_routes_to_v2_impl() {
    let app = build_test_router(build_composed_app_state().await);
    let resp = app.oneshot(post_graphql("{ query1 }", Some("2"))).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(json_body(resp).await["data"]["query1"], "v2-result");
}

/// query2 with header 1 → shared resolver.
#[tokio::test]
async fn composed_query2_resolves_via_shared_on_v1() {
    let app = build_test_router(build_composed_app_state().await);
    let resp = app.oneshot(post_graphql("{ query2 }", Some("1"))).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(json_body(resp).await["data"]["query2"], "shared-result");
}

/// query2 with header 2 → same shared resolver, same result.
/// This is the key invariant: unchanged queries work identically across versions.
#[tokio::test]
async fn composed_query2_resolves_via_shared_on_v2() {
    let app = build_test_router(build_composed_app_state().await);
    let resp = app.oneshot(post_graphql("{ query2 }", Some("2"))).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(json_body(resp).await["data"]["query2"], "shared-result");
}
