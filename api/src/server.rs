//! Axum HTTP server configuration with GraphQL support

use std::{pin::Pin, sync::Arc};

use async_graphql::{
    Schema,
    http::{GraphQLPlaygroundConfig, playground_source},
};
use async_stream::stream;
use axum::{
    Json, Router,
    extract::State,
    http::{HeaderMap, Method, StatusCode, header},
    response::{
        Html, IntoResponse, Response,
        sse::{Event, Sse},
    },
    routing::get,
};
use blokli_chain_api::{
    DefaultHttpRequestor, rpc_adapter::RpcAdapter, transaction_executor::RawTransactionExecutor,
    transaction_store::TransactionStore,
};
use blokli_chain_indexer::IndexerState;
use blokli_chain_rpc::{rpc::RpcOperations, transport::ReqwestClient};
use blokli_db_entity::prelude::ChainInfo;
use futures::stream::{Stream, StreamExt};
use sea_orm::{DatabaseConnection, EntityTrait};
use serde::Serialize;
use serde_json::Value;
use tower_http::{
    CompressionLevel,
    compression::{
        CompressionLayer,
        predicate::{Predicate, SizeAbove},
    },
    cors::CorsLayer,
    trace::TraceLayer,
};

use crate::{
    config::{ApiConfig, HealthConfig},
    errors::ApiResult,
    mutation::MutationRoot,
    query::QueryRoot,
    readiness::{ReadinessChecker, ReadinessState},
    schema::build_schema,
    subscription::SubscriptionRoot,
};

/// Health check response for liveness probe
#[derive(Serialize)]
struct HealthResponse {
    status: String,
    version: &'static str,
}

/// Readiness check response
#[derive(Serialize)]
struct ReadinessResponse {
    status: String,
    version: &'static str,
    checks: ReadinessChecks,
}

/// Individual readiness checks
#[derive(Serialize)]
struct ReadinessChecks {
    database: CheckStatus,
    rpc: RpcCheckStatus,
    indexer: IndexerCheckStatus,
}

/// Generic check status
#[derive(Serialize)]
struct CheckStatus {
    status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

/// RPC check status with block number
#[derive(Serialize)]
struct RpcCheckStatus {
    status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    block_number: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

/// Indexer check status with lag information
#[derive(Serialize)]
struct IndexerCheckStatus {
    status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    last_indexed_block: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    lag: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

/// Predicate that excludes Server-Sent Events from compression
///
/// SSE requires immediate event delivery without buffering.
/// Compression would buffer responses, breaking real-time streaming.
#[derive(Clone, Copy)]
struct NotSse;

impl Predicate for NotSse {
    fn should_compress<B>(&self, response: &axum::http::Response<B>) -> bool {
        // Check Content-Type header for text/event-stream
        // HTTP headers are case-insensitive, so we need case-insensitive comparison
        let is_sse = response
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .map(|v| v.to_lowercase().contains("text/event-stream"))
            .unwrap_or(false);

        // Only compress if NOT SSE
        !is_sse
    }
}

/// Application state shared across handlers
#[derive(Clone)]
pub struct AppState {
    pub schema: Arc<Schema<QueryRoot, MutationRoot, SubscriptionRoot>>,
    pub playground_enabled: bool,
    pub db: DatabaseConnection,
    pub rpc_operations: Arc<RpcOperations<ReqwestClient>>,
    pub health_config: HealthConfig,
    pub readiness_checker: ReadinessChecker,
}

/// Build the Axum application router
#[allow(clippy::too_many_arguments)]
pub async fn build_app(
    db: DatabaseConnection,
    network: String,
    config: ApiConfig,
    expected_block_time: u64,
    indexer_state: IndexerState,
    transaction_executor: Arc<RawTransactionExecutor<RpcAdapter<DefaultHttpRequestor>>>,
    transaction_store: Arc<TransactionStore>,
    rpc_operations: Arc<RpcOperations<ReqwestClient>>,
) -> ApiResult<Router> {
    let schema = build_schema(
        db.clone(),
        config.chain_id,
        network,
        config.contract_addresses,
        expected_block_time,
        indexer_state,
        transaction_executor,
        transaction_store,
        rpc_operations.clone(),
    );

    let readiness_checker = ReadinessChecker::new(db.clone(), rpc_operations.clone(), config.health.clone());

    // Start periodic readiness updates in background
    readiness_checker.clone().start_periodic_updates();

    let app_state = AppState {
        schema: Arc::new(schema),
        playground_enabled: config.playground_enabled,
        db,
        rpc_operations,
        health_config: config.health,
        readiness_checker,
    };

    // Configure CORS based on allowed origins
    let cors_layer = if config.cors_allowed_origins.contains(&"*".to_string()) {
        // Permissive CORS for development
        CorsLayer::permissive()
    } else {
        // Restrictive CORS with specific origins
        let allowed_origins: Vec<_> = config
            .cors_allowed_origins
            .iter()
            .filter_map(|origin| origin.parse::<axum::http::HeaderValue>().ok())
            .collect();

        CorsLayer::new()
            .allow_origin(allowed_origins)
            .allow_methods([Method::GET, Method::POST, Method::OPTIONS])
            .allow_headers([header::CONTENT_TYPE, header::ACCEPT, header::AUTHORIZATION])
            .allow_credentials(true)
    };

    Ok(Router::new()
        // GraphQL endpoint (queries, mutations, and SSE subscriptions)
        .route("/graphql", get(graphql_playground).post(graphql_handler))
        // Health check endpoints for Kubernetes probes
        .route("/healthz", get(healthz_handler))
        .route("/readyz", get(readyz_handler))
        .layer(cors_layer)
        // Use zstd compression only with high quality, only for responses > 1KB
        // Exclude SSE responses to preserve real-time streaming
        .layer(
            CompressionLayer::new()
                .zstd(true)
                // Use balanced compression to not use too much CPU
                .quality(CompressionLevel::Default)
                .compress_when(
                    // Compression requires: size > 1KB AND not SSE
                    SizeAbove::new(1024).and(NotSse),
                ),
        )
        .layer(TraceLayer::new_for_http())
        .with_state(app_state))
}

/// GraphQL query/mutation/subscription handler
async fn graphql_handler(State(state): State<AppState>, headers: HeaderMap, Json(request): Json<Value>) -> Response {
    // Check if server is ready before processing GraphQL requests
    if state.readiness_checker.get().await == ReadinessState::NotReady {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({
                "errors": [{
                    "message": "GraphQL API is not ready yet. Indexer is still catching up. Please try again later."
                }]
            })),
        )
            .into_response();
    }

    // Parse the GraphQL request
    let request = match serde_json::from_value::<async_graphql::Request>(request) {
        Ok(req) => req,
        Err(e) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({
                    "errors": [{
                        "message": format!("Invalid GraphQL request: {}", e)
                    }]
                })),
            )
                .into_response();
        }
    };

    // Check if client accepts SSE (for subscriptions)
    let accepts_sse = headers
        .get("accept")
        .and_then(|v| v.to_str().ok())
        .map(|v| v.contains("text/event-stream"))
        .unwrap_or(false);

    // Check if the request is a subscription
    let is_subscription = request.query.trim_start().starts_with("subscription");

    // Handle subscription requests via SSE
    if accepts_sse && is_subscription {
        let schema = state.schema.clone();
        let sse_stream: Pin<Box<dyn Stream<Item = Result<Event, std::convert::Infallible>> + Send>> =
            Box::pin(stream! {
                let mut response_stream = schema.as_ref().execute_stream(request);
                while let Some(response) = response_stream.next().await {
                    let json = serde_json::to_string(&response)
                        .unwrap_or_else(|_| r#"{"errors":[{"message":"Failed to serialize response"}]}"#.to_string());
                    yield Ok::<_, std::convert::Infallible>(Event::default().data(json));
                }
            });

        return Sse::new(sse_stream).into_response();
    }

    // Execute regular query/mutation
    let response = state.schema.execute(request).await;

    // Serialize and return the response
    Json(serde_json::to_value(response).unwrap_or_else(|_| {
        serde_json::json!({
            "errors": [{"message": "Failed to serialize response"}]
        })
    }))
    .into_response()
}

/// GraphQL Playground UI (only enabled if playground_enabled config is true)
async fn graphql_playground(State(state): State<AppState>) -> impl IntoResponse {
    if state.playground_enabled {
        Html(playground_source(GraphQLPlaygroundConfig::new("/graphql"))).into_response()
    } else {
        (
            StatusCode::NOT_FOUND,
            "GraphQL Playground is disabled. Use POST /graphql for queries.",
        )
            .into_response()
    }
}

/// Liveness probe endpoint - minimal check that process is alive
async fn healthz_handler() -> impl IntoResponse {
    Json(HealthResponse {
        status: "healthy".to_string(),
        version: env!("CARGO_PKG_VERSION"),
    })
}

/// Readiness probe endpoint - comprehensive check for service readiness
async fn readyz_handler(State(state): State<AppState>) -> impl IntoResponse {
    // Perform an immediate out-of-band readiness check and update the cached state
    state.readiness_checker.check_and_update().await;

    let mut all_healthy = true;
    let mut checks = ReadinessChecks {
        database: CheckStatus {
            status: "healthy".to_string(),
            error: None,
        },
        rpc: RpcCheckStatus {
            status: "healthy".to_string(),
            block_number: None,
            error: None,
        },
        indexer: IndexerCheckStatus {
            status: "healthy".to_string(),
            last_indexed_block: None,
            lag: None,
            error: None,
        },
    };

    // 1. Check database connectivity by querying chain_info
    let db_result = ChainInfo::find().one(&state.db).await;
    let indexed_block = match db_result {
        Ok(Some(info)) => Some(info.last_indexed_block),
        Ok(None) => {
            // No chain info yet - indexer hasn't started
            all_healthy = false;
            checks.database = CheckStatus {
                status: "unhealthy".to_string(),
                error: Some("No chain info found - indexer may not have started".to_string()),
            };
            None
        }
        Err(e) => {
            all_healthy = false;
            checks.database = CheckStatus {
                status: "unhealthy".to_string(),
                error: Some(e.to_string()),
            };
            None
        }
    };

    // 2. Check RPC connectivity and get current block
    let rpc_block = match state.rpc_operations.get_block_number().await {
        Ok(block) => {
            checks.rpc.block_number = Some(block);
            Some(block)
        }
        Err(e) => {
            all_healthy = false;
            checks.rpc = RpcCheckStatus {
                status: "unhealthy".to_string(),
                block_number: None,
                error: Some(e.to_string()),
            };
            None
        }
    };

    // 3. Check indexer lag
    if let (Some(indexed), Some(rpc_block)) = (indexed_block, rpc_block) {
        let indexed_u64 = u64::try_from(indexed).unwrap_or(0);
        let lag = rpc_block.saturating_sub(indexed_u64);
        checks.indexer.last_indexed_block = Some(indexed);
        checks.indexer.lag = Some(lag);

        if lag > state.health_config.max_indexer_lag {
            all_healthy = false;
            checks.indexer.status = "unhealthy".to_string();
            checks.indexer.error = Some(format!(
                "indexer lag exceeds threshold ({} > {} blocks)",
                lag, state.health_config.max_indexer_lag
            ));
        }
    } else if indexed_block.is_some() {
        // We have indexed block but no RPC block - already marked RPC as unhealthy
        checks.indexer.last_indexed_block = indexed_block;
    }

    let response = ReadinessResponse {
        status: if all_healthy { "ready" } else { "not_ready" }.to_string(),
        version: env!("CARGO_PKG_VERSION"),
        checks,
    };

    if all_healthy {
        (StatusCode::OK, Json(response)).into_response()
    } else {
        (StatusCode::SERVICE_UNAVAILABLE, Json(response)).into_response()
    }
}

#[cfg(test)]
mod tests {
    use axum::http::{HeaderValue, Response, StatusCode};

    use super::*;

    /// Helper to create a response with specific Content-Type header
    fn make_response(content_type: Option<&str>) -> Response<String> {
        let mut builder = Response::builder().status(200);

        if let Some(ct_value) = content_type {
            builder = builder.header(header::CONTENT_TYPE, HeaderValue::from_str(ct_value).unwrap());
        }

        builder.body(String::new()).unwrap()
    }

    // Unit tests for HealthResponse serialization
    #[test]
    fn test_health_response_serialization() {
        let response = HealthResponse {
            status: "healthy".to_string(),
            version: "1.0.0",
        };
        let json = serde_json::to_value(&response).unwrap();
        assert_eq!(json["status"], "healthy");
        assert_eq!(json["version"], "1.0.0");
    }

    #[test]
    fn test_readiness_response_all_healthy() {
        let checks = ReadinessChecks {
            database: CheckStatus {
                status: "healthy".to_string(),
                error: None,
            },
            rpc: RpcCheckStatus {
                status: "healthy".to_string(),
                block_number: Some(12345),
                error: None,
            },
            indexer: IndexerCheckStatus {
                status: "healthy".to_string(),
                last_indexed_block: Some(12340),
                lag: Some(5),
                error: None,
            },
        };

        let response = ReadinessResponse {
            status: "ready".to_string(),
            version: "1.0.0",
            checks,
        };

        let json = serde_json::to_value(&response).unwrap();
        assert_eq!(json["status"], "ready");
        assert_eq!(json["checks"]["database"]["status"], "healthy");
        assert_eq!(json["checks"]["rpc"]["block_number"], 12345);
        assert_eq!(json["checks"]["indexer"]["lag"], 5);
        // Verify error fields are not present when None
        assert!(json["checks"]["database"]["error"].is_null());
    }

    #[test]
    fn test_readiness_response_database_failure() {
        let checks = ReadinessChecks {
            database: CheckStatus {
                status: "unhealthy".to_string(),
                error: Some("Connection refused".to_string()),
            },
            rpc: RpcCheckStatus {
                status: "healthy".to_string(),
                block_number: Some(12345),
                error: None,
            },
            indexer: IndexerCheckStatus {
                status: "healthy".to_string(),
                last_indexed_block: None,
                lag: None,
                error: None,
            },
        };

        let response = ReadinessResponse {
            status: "not_ready".to_string(),
            version: "1.0.0",
            checks,
        };

        let json = serde_json::to_value(&response).unwrap();
        assert_eq!(json["status"], "not_ready");
        assert_eq!(json["checks"]["database"]["status"], "unhealthy");
        assert_eq!(json["checks"]["database"]["error"], "Connection refused");
    }

    #[test]
    fn test_readiness_response_rpc_failure() {
        let checks = ReadinessChecks {
            database: CheckStatus {
                status: "healthy".to_string(),
                error: None,
            },
            rpc: RpcCheckStatus {
                status: "unhealthy".to_string(),
                block_number: None,
                error: Some("RPC endpoint unreachable".to_string()),
            },
            indexer: IndexerCheckStatus {
                status: "healthy".to_string(),
                last_indexed_block: Some(12340),
                lag: None,
                error: None,
            },
        };

        let response = ReadinessResponse {
            status: "not_ready".to_string(),
            version: "1.0.0",
            checks,
        };

        let json = serde_json::to_value(&response).unwrap();
        assert_eq!(json["status"], "not_ready");
        assert_eq!(json["checks"]["rpc"]["status"], "unhealthy");
        assert!(json["checks"]["rpc"]["block_number"].is_null());
        assert_eq!(json["checks"]["rpc"]["error"], "RPC endpoint unreachable");
    }

    #[test]
    fn test_readiness_response_indexer_lag_exceeded() {
        let checks = ReadinessChecks {
            database: CheckStatus {
                status: "healthy".to_string(),
                error: None,
            },
            rpc: RpcCheckStatus {
                status: "healthy".to_string(),
                block_number: Some(12345),
                error: None,
            },
            indexer: IndexerCheckStatus {
                status: "unhealthy".to_string(),
                last_indexed_block: Some(12300),
                lag: Some(45),
                error: Some("indexer lag exceeds threshold (45 > 10 blocks)".to_string()),
            },
        };

        let response = ReadinessResponse {
            status: "not_ready".to_string(),
            version: "1.0.0",
            checks,
        };

        let json = serde_json::to_value(&response).unwrap();
        assert_eq!(json["status"], "not_ready");
        assert_eq!(json["checks"]["indexer"]["status"], "unhealthy");
        assert_eq!(json["checks"]["indexer"]["lag"], 45);
        assert!(
            json["checks"]["indexer"]["error"]
                .as_str()
                .unwrap()
                .contains("exceeds threshold")
        );
    }

    #[test]
    fn test_check_status_skips_none_error() {
        let status = CheckStatus {
            status: "healthy".to_string(),
            error: None,
        };

        let json = serde_json::to_value(&status).unwrap();
        // skip_serializing_if should exclude the error field
        assert!(!json.as_object().unwrap().contains_key("error"));
    }

    #[test]
    fn test_rpc_check_status_skips_none_fields() {
        let status = RpcCheckStatus {
            status: "unhealthy".to_string(),
            block_number: None,
            error: Some("error".to_string()),
        };

        let json = serde_json::to_value(&status).unwrap();
        let obj = json.as_object().unwrap();
        // skip_serializing_if should exclude None fields
        assert!(!obj.contains_key("block_number"));
        assert!(obj.contains_key("error"));
    }

    #[test]
    fn test_indexer_check_status_skips_none_fields() {
        let status = IndexerCheckStatus {
            status: "healthy".to_string(),
            last_indexed_block: Some(100),
            lag: None,
            error: None,
        };

        let json = serde_json::to_value(&status).unwrap();
        let obj = json.as_object().unwrap();
        assert!(obj.contains_key("last_indexed_block"));
        assert!(!obj.contains_key("lag"));
        assert!(!obj.contains_key("error"));
    }

    // Test healthz handler directly
    #[tokio::test]
    async fn test_healthz_handler_returns_healthy() {
        let response = healthz_handler().await;
        let response = response.into_response();

        assert_eq!(response.status(), StatusCode::OK);

        // Extract and verify body
        let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

        assert_eq!(json["status"], "healthy");
        assert!(json["version"].is_string());
    }

    // Test that healthz handler uses correct CARGO_PKG_VERSION
    #[tokio::test]
    async fn test_healthz_handler_version() {
        let response = healthz_handler().await;
        let response = response.into_response();

        let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

        // Version should match the package version
        assert_eq!(json["version"], env!("CARGO_PKG_VERSION"));
    }

    #[test]
    fn test_not_sse_predicate_rejects_sse_responses() {
        // SSE response should NOT be compressed
        let response = make_response(Some("text/event-stream"));
        let predicate = NotSse;
        assert!(
            !predicate.should_compress(&response),
            "SSE responses should not be compressed"
        );
    }

    #[test]
    fn test_not_sse_predicate_allows_json_responses() {
        // Regular JSON response should be compressed
        let response = make_response(Some("application/json"));
        let predicate = NotSse;
        assert!(
            predicate.should_compress(&response),
            "JSON responses should be compressed"
        );
    }

    #[test]
    fn test_not_sse_predicate_allows_no_content_type_header() {
        // Response without Content-Type header should be compressed
        let response = make_response(None);
        let predicate = NotSse;
        assert!(
            predicate.should_compress(&response),
            "Responses without Content-Type header should be compressed"
        );
    }

    #[test]
    fn test_not_sse_predicate_handles_multiple_content_type_values() {
        // Response with multiple Content-Type values including SSE should NOT be compressed
        let response = make_response(Some("application/json, text/event-stream"));
        let predicate = NotSse;
        assert!(
            !predicate.should_compress(&response),
            "Responses with SSE content type should not be compressed"
        );
    }

    #[test]
    fn test_not_sse_predicate_case_insensitive() {
        // Content-Type header with different casing should be handled correctly
        // HTTP headers are case-insensitive, so we use .to_lowercase()
        let response = make_response(Some("text/Event-Stream"));
        let predicate = NotSse;
        assert!(
            !predicate.should_compress(&response),
            "Case-insensitive check - text/Event-Stream should match text/event-stream"
        );
    }

    #[test]
    fn test_not_sse_predicate_partial_match() {
        // Ensure we're doing substring match, not exact match
        let response = make_response(Some("text/event-stream; charset=utf-8"));
        let predicate = NotSse;
        assert!(
            !predicate.should_compress(&response),
            "SSE with charset should not be compressed"
        );
    }

    #[test]
    fn test_not_sse_predicate_invalid_header_value() {
        // Response with no Content-Type header should be compressed (fallback)
        let response = Response::builder().status(200).body(String::new()).unwrap();

        // The predicate handles .to_str().ok() returning None → unwrap_or(false) → compress
        let predicate = NotSse;
        assert!(
            predicate.should_compress(&response),
            "Missing Content-Type header should default to compress"
        );
    }
}
