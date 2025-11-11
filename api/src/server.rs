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
    http::{HeaderMap, StatusCode, header},
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
use futures::stream::{Stream, StreamExt};
use sea_orm::DatabaseConnection;
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
    config::ApiConfig, errors::ApiResult, mutation::MutationRoot, query::QueryRoot, schema::build_schema,
    subscription::SubscriptionRoot,
};

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
}

/// Build the Axum application router
pub async fn build_app(
    db: DatabaseConnection,
    network: String,
    config: ApiConfig,
    indexer_state: IndexerState,
    transaction_executor: Arc<RawTransactionExecutor<RpcAdapter<DefaultHttpRequestor>>>,
    transaction_store: Arc<TransactionStore>,
    rpc_operations: Arc<RpcOperations<ReqwestClient>>,
) -> ApiResult<Router> {
    let schema = build_schema(
        db,
        config.chain_id,
        network,
        config.contract_addresses,
        indexer_state,
        transaction_executor,
        transaction_store,
        rpc_operations,
    );

    let app_state = AppState {
        schema: Arc::new(schema),
        playground_enabled: config.playground_enabled,
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
            .allow_methods([
                axum::http::Method::GET,
                axum::http::Method::POST,
                axum::http::Method::OPTIONS,
            ])
            .allow_headers([header::CONTENT_TYPE, header::ACCEPT, header::AUTHORIZATION])
            .allow_credentials(true)
    };

    Ok(Router::new()
        // GraphQL endpoint (queries, mutations, and SSE subscriptions)
        .route("/graphql", get(graphql_playground).post(graphql_handler))
        // Health check
        .route("/health", get(health_handler))
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

/// Health check endpoint
async fn health_handler() -> impl IntoResponse {
    "ok"
}

#[cfg(test)]
mod tests {
    use axum::http::{HeaderValue, Response};

    use super::*;

    /// Helper to create a response with specific Content-Type header
    fn make_response(content_type: Option<&str>) -> Response<String> {
        let mut builder = Response::builder().status(200);

        if let Some(ct_value) = content_type {
            builder = builder.header(header::CONTENT_TYPE, HeaderValue::from_str(ct_value).unwrap());
        }

        builder.body(String::new()).unwrap()
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
