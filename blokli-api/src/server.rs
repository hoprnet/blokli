//! Axum HTTP server configuration with GraphQL support

use async_graphql::http::{GraphQLPlaygroundConfig, playground_source};
use async_graphql_axum::{GraphQLRequest, GraphQLResponse};
use axum::{
    extract::State,
    response::{Html, IntoResponse, sse::{Event, KeepAlive, Sse}},
    routing::get,
    Router,
};
use futures::stream::Stream;
use std::sync::Arc;
use tower_http::{
    cors::CorsLayer,
    trace::TraceLayer,
    compression::CompressionLayer,
};

use crate::schema::{build_schema, MutationRoot, QueryRoot, SubscriptionRoot};

/// Application state shared across handlers
#[derive(Clone)]
pub struct AppState {
    pub schema: Arc<async_graphql::Schema<QueryRoot, MutationRoot, SubscriptionRoot>>,
}

/// Build the Axum application router
pub fn build_app() -> Router {
    let schema = build_schema();
    let app_state = AppState { schema: Arc::new(schema) };

    Router::new()
        // GraphQL endpoint
        .route("/graphql", get(graphql_playground).post(graphql_handler))
        // GraphQL subscriptions via SSE
        .route("/graphql/subscriptions", get(graphql_subscription_handler))
        // Health check
        .route("/health", get(health_handler))
        .layer(CorsLayer::permissive())
        .layer(CompressionLayer::new())
        .layer(TraceLayer::new_for_http())
        .with_state(app_state)
}

/// GraphQL query/mutation handler
async fn graphql_handler(
    State(state): State<AppState>,
    req: GraphQLRequest,
) -> GraphQLResponse {
    state.schema.execute(req.into_inner()).await.into()
}

/// GraphQL Playground UI
async fn graphql_playground() -> impl IntoResponse {
    Html(playground_source(
        GraphQLPlaygroundConfig::new("/graphql")
            .subscription_endpoint("/graphql/subscriptions")
    ))
}

/// GraphQL subscription handler using SSE
async fn graphql_subscription_handler(
    State(state): State<AppState>,
    req: GraphQLRequest,
) -> Sse<impl Stream<Item = Result<Event, std::io::Error>>> {
    let schema = Arc::clone(&state.schema);
    let stream = async_stream::stream! {
        let mut response_stream = schema.execute_stream(req.into_inner());
        while let Some(response) = futures::StreamExt::next(&mut response_stream).await {
            match Event::default().json_data(response) {
                Ok(event) => yield Ok(event),
                Err(e) => yield Err(std::io::Error::new(std::io::ErrorKind::Other, e)),
            }
        }
    };

    Sse::new(stream).keep_alive(KeepAlive::default())
}

/// Health check endpoint
async fn health_handler() -> impl IntoResponse {
    "ok"
}
