mod common;

use std::time::Duration;

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use blokli_db_entity::chain_info;
use futures::StreamExt;
use sea_orm::{EntityTrait, Set, sea_query::OnConflict};
use serde_json::json;
use tower::ServiceExt;
use uuid::Uuid;

#[test_log::test(tokio::test)]
async fn test_sse_keepalive_comments_emitted() -> anyhow::Result<()> {
    // Ensure readiness passes so the subscription returns 200.
    let ctx = common::setup_http_test_environment().await?;
    let current_block = ctx.rpc_operations.get_block_number().await?;
    let block_number = i64::try_from(current_block)?;
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
        .exec(&ctx.db)
        .await?;
    tokio::time::sleep(Duration::from_millis(150)).await;

    // Subscribe with no events and assert the keepalive comment arrives.
    let request_body = json!({
        "query": format!(
            "subscription {{ transactionUpdated(id: \"{}\") {{ id status }} }}",
            Uuid::new_v4()
        )
    });

    let request = Request::builder()
        .method("POST")
        .uri("/graphql")
        .header("content-type", "application/json")
        .header("accept", "text/event-stream")
        .body(Body::from(
            serde_json::to_string(&request_body).expect("Failed to serialize JSON"),
        ))
        .expect("Failed to build request");

    let response = ctx.app.oneshot(request).await.expect("Failed to execute request");
    assert_eq!(response.status(), StatusCode::OK);

    let mut stream = response.into_body().into_data_stream();
    let frame = tokio::time::timeout(Duration::from_millis(200), stream.next())
        .await
        .expect("Timed out waiting for keepalive")
        .expect("SSE stream closed before keepalive")
        .expect("SSE stream returned error");

    let payload = String::from_utf8_lossy(&frame);
    assert!(
        payload.contains("keepalive-test"),
        "unexpected keepalive payload: {payload}"
    );

    Ok(())
}
