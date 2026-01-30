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
    // The payload should contain "sub-transaction-updated-{uuid}-keep-alive"
    assert!(
        payload.contains("sub-transaction-updated-"),
        "expected subscription name in identifier, got: {payload}"
    );
    assert!(
        payload.contains("-keep-alive"),
        "expected keep-alive suffix in identifier, got: {payload}"
    );

    Ok(())
}

#[test_log::test(tokio::test)]
async fn test_sse_keepalive_uuid_identifier_format() -> anyhow::Result<()> {
    // Test that keep-alive includes UUID identifier in correct format
    // Format: sub-{subscription-name}-{uuid}-keep-alive
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

    let request_body = json!({
        "query": format!(
            "subscription SafeDeployed {{ safeDeployed(address: \"{}\") {{ address }} }}",
            "0x0000000000000000000000000000000000000000"
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

    // Verify format: sub-{subscription-name}-{uuid}-keep-alive
    assert!(
        payload.contains("sub-safe-deployed-"),
        "expected subscription name 'safe-deployed' in identifier, got: {payload}"
    );
    assert!(
        payload.contains("-keep-alive"),
        "expected '-keep-alive' suffix in identifier, got: {payload}"
    );

    // Verify UUID v4 format (8-4-4-4-12 hex digits)
    // UUID regex: [0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}
    let uuid_regex = regex::Regex::new(
        r"sub-safe-deployed-([0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12})-keep-alive",
    )?;
    assert!(
        uuid_regex.is_match(&payload),
        "expected valid UUID v4 format in identifier, got: {payload}"
    );

    Ok(())
}

#[test_log::test(tokio::test)]
async fn test_sse_identifier_uniqueness() -> anyhow::Result<()> {
    // Start two SSE subscriptions concurrently
    // Extract identifiers from keep-alive comments
    // Assert both UUIDs are unique (different)
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

    // Create two subscription requests
    let request1 = Request::builder()
        .method("POST")
        .uri("/graphql")
        .header("content-type", "application/json")
        .header("accept", "text/event-stream")
        .body(Body::from(
            serde_json::to_string(&json!({
                "query": format!(
                    "subscription {{ transactionUpdated(id: \"{}\") {{ id status }} }}",
                    Uuid::new_v4()
                )
            }))
            .expect("Failed to serialize JSON"),
        ))
        .expect("Failed to build request");

    let request2 = Request::builder()
        .method("POST")
        .uri("/graphql")
        .header("content-type", "application/json")
        .header("accept", "text/event-stream")
        .body(Body::from(
            serde_json::to_string(&json!({
                "query": format!(
                    "subscription {{ transactionUpdated(id: \"{}\") {{ id status }} }}",
                    Uuid::new_v4()
                )
            }))
            .expect("Failed to serialize JSON"),
        ))
        .expect("Failed to build request");

    // Need to create two separate app instances since oneshot consumes the app
    let ctx1 = common::setup_http_test_environment().await?;
    let ctx2 = common::setup_http_test_environment().await?;

    let response1 = ctx1.app.oneshot(request1).await.expect("Failed to execute request 1");
    let response2 = ctx2.app.oneshot(request2).await.expect("Failed to execute request 2");

    assert_eq!(response1.status(), StatusCode::OK);
    assert_eq!(response2.status(), StatusCode::OK);

    let mut stream1 = response1.into_body().into_data_stream();
    let mut stream2 = response2.into_body().into_data_stream();

    let frame1 = tokio::time::timeout(Duration::from_millis(200), stream1.next())
        .await
        .expect("Timed out waiting for keepalive 1")
        .expect("SSE stream 1 closed before keepalive")
        .expect("SSE stream 1 returned error");

    let frame2 = tokio::time::timeout(Duration::from_millis(200), stream2.next())
        .await
        .expect("Timed out waiting for keepalive 2")
        .expect("SSE stream 2 closed before keepalive")
        .expect("SSE stream 2 returned error");

    let payload1 = String::from_utf8_lossy(&frame1);
    let payload2 = String::from_utf8_lossy(&frame2);

    // Extract UUIDs from both payloads
    let uuid_regex = regex::Regex::new(
        r"sub-transaction-updated-([0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12})-keep-alive",
    )?;

    let uuid1 = uuid_regex
        .captures(&payload1)
        .and_then(|caps| caps.get(1))
        .map(|m| m.as_str())
        .expect("Failed to extract UUID from payload 1");

    let uuid2 = uuid_regex
        .captures(&payload2)
        .and_then(|caps| caps.get(1))
        .map(|m| m.as_str())
        .expect("Failed to extract UUID from payload 2");

    // Assert UUIDs are different
    assert_ne!(
        uuid1, uuid2,
        "SSE connection identifiers must be unique, but both were: {uuid1}"
    );

    Ok(())
}
