mod common;

use std::{sync::Arc, time::Duration};

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use blokli_db_entity::chain_info;
use sea_orm::{DatabaseConnection, EntityTrait, Set, sea_query::OnConflict};
use serde_json::json;
use tower::ServiceExt;

async fn make_graphql_request(app: axum::Router, body: serde_json::Value) -> (StatusCode, serde_json::Value) {
    let request = Request::builder()
        .method("POST")
        .uri("/graphql")
        .header("content-type", "application/json")
        .body(Body::from(
            serde_json::to_string(&body).expect("failed to serialize JSON"),
        ))
        .expect("failed to build request");

    let response = app.oneshot(request).await.expect("failed to execute request");
    let status = response.status();
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("failed to read body");
    let json = serde_json::from_slice(&body).expect("failed to parse JSON");

    (status, json)
}

async fn mark_ready(
    db: &DatabaseConnection,
    rpc_operations: &Arc<blokli_chain_rpc::rpc::RpcOperations<blokli_chain_rpc::transport::ReqwestClient>>,
) -> anyhow::Result<()> {
    let block_number = i64::try_from(rpc_operations.get_block_number().await?)?;
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

#[test_log::test(tokio::test)]
async fn graphql_accepts_query_batch() -> anyhow::Result<()> {
    let ctx = common::setup_http_test_environment().await?;
    mark_ready(&ctx.db, &ctx.rpc_operations).await?;
    tokio::time::sleep(Duration::from_millis(150)).await;

    let (status, response) = make_graphql_request(
        ctx.app,
        json!([
            {"query": "query { __typename }"},
            {"query": "query { health }"}
        ]),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert!(response.is_array());
    assert_eq!(response[0]["data"]["__typename"], "QueryRoot");
    assert_eq!(response[1]["data"]["health"], "ok");

    Ok(())
}

#[test_log::test(tokio::test)]
async fn graphql_rejects_mutation_in_batch() -> anyhow::Result<()> {
    let ctx = common::setup_http_test_environment().await?;
    mark_ready(&ctx.db, &ctx.rpc_operations).await?;
    tokio::time::sleep(Duration::from_millis(150)).await;

    let (status, response) = make_graphql_request(
        ctx.app,
        json!([
            {"query": "query { __typename }"},
            {"query": "mutation { __typename }"}
        ]),
    )
    .await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(
        response["errors"][0]["message"]
            .as_str()
            .unwrap()
            .contains("may only contain queries")
    );

    Ok(())
}
