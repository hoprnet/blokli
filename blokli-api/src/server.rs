//! Axum HTTP server configuration with GraphQL support

use std::sync::Arc;

use async_graphql::{
    dynamic::Schema,
    http::{GraphQLPlaygroundConfig, playground_source},
};
use axum::{
    Json, Router,
    extract::State,
    http::StatusCode,
    response::{Html, IntoResponse, Response},
    routing::get,
};
use sea_orm::DatabaseConnection;
use serde_json::Value;
use tower_http::{
    CompressionLevel,
    compression::{CompressionLayer, predicate::SizeAbove},
    cors::CorsLayer,
    trace::TraceLayer,
};

use crate::{errors::ApiResult, schema::build_schema};

/// Application state shared across handlers
#[derive(Clone)]
pub struct AppState {
    pub schema: Arc<Schema>,
}

/// Build the Axum application router
pub async fn build_app(db: DatabaseConnection) -> ApiResult<Router> {
    let schema = build_schema(db)?;
    let app_state = AppState {
        schema: Arc::new(schema),
    };

    Ok(Router::new()
        // GraphQL endpoint
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

/// GraphQL query/mutation handler
async fn graphql_handler(State(state): State<AppState>, Json(request): Json<Value>) -> Response {
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

    // Execute the request (schema.execute accepts Into<DynamicRequest>)
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
