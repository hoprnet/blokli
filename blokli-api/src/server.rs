//! Axum HTTP server configuration with GraphQL support

use std::sync::Arc;

use async_graphql::{
    EmptyMutation, Schema,
    http::{GraphQLPlaygroundConfig, playground_source},
};
use axum::{
    Json, Router,
    extract::State,
    http::{HeaderMap, StatusCode},
    response::{
        Html, IntoResponse, Response,
        sse::{Event, Sse},
    },
    routing::get,
};
use futures::stream::StreamExt;
use sea_orm::DatabaseConnection;
use serde_json::Value;
use tower_http::{
    CompressionLevel,
    compression::{CompressionLayer, predicate::SizeAbove},
    cors::CorsLayer,
    trace::TraceLayer,
};

use crate::{errors::ApiResult, query::QueryRoot, schema::build_schema, subscription::SubscriptionRoot};

/// Application state shared across handlers
#[derive(Clone)]
pub struct AppState {
    pub schema: Arc<Schema<QueryRoot, EmptyMutation, SubscriptionRoot>>,
}

/// Build the Axum application router
pub async fn build_app(db: DatabaseConnection) -> ApiResult<Router> {
    let schema = build_schema(db);
    let app_state = AppState {
        schema: Arc::new(schema),
    };

    Ok(Router::new()
        // GraphQL endpoint (queries, mutations, and SSE subscriptions)
        .route("/graphql", get(graphql_playground).post(graphql_handler))
        // Health check
        .route("/health", get(health_handler))
        .layer(CorsLayer::permissive())
        // Use zstd compression only with high quality, only for responses > 1KB
        .layer(
            CompressionLayer::new()
                .zstd(true)
                .quality(CompressionLevel::Best)
                .compress_when(SizeAbove::new(1024)),
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
        use std::pin::Pin;

        use async_stream::stream;
        use futures::stream::Stream;

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

/// GraphQL Playground UI
async fn graphql_playground() -> impl IntoResponse {
    Html(playground_source(GraphQLPlaygroundConfig::new("/graphql")))
}

/// Health check endpoint
async fn health_handler() -> impl IntoResponse {
    "ok"
}
