use blokli_client::{BlokliClient, api::BlokliQueryClient};
use serde_json::json;

use crate::common::{Body, RequestRecorder};

mod common;

#[tokio::test]
async fn query_native_balance() -> anyhow::Result<()> {
    let mut server = mockito::Server::new_async().await;

    let cli = BlokliClient::new(server.url().parse()?, Default::default());

    let recorder = RequestRecorder::default();

    let mock = server
        .mock("POST", "/graphql")
        .with_status(200)
        .match_request(recorder.as_matcher())
        .with_header("content-type", "application/json")
        .with_body(
            r#"{
              "data": {
                "nativeBalance": {
                  "__typename": "NativeBalance",
                  "balance": "1234567890"
                }
              }
            }
        "#,
        )
        .create_async()
        .await;

    let balance = cli.query_native_balance(&[1u8; 20]).await?;
    assert_eq!("1234567890", balance.balance.0);

    mock.assert_async().await;

    insta::assert_yaml_snapshot!(recorder.requests());

    Ok(())
}

#[tokio::test]
async fn query_token_balance() -> anyhow::Result<()> {
    let mut server = mockito::Server::new_async().await;

    let cli = BlokliClient::new(server.url().parse()?, Default::default());

    let recorder = RequestRecorder::default();

    let mock = server
        .mock("POST", "/graphql")
        .with_status(200)
        .match_request(recorder.as_matcher())
        .with_header("content-type", "application/json")
        .with_body(
            r#"{
              "data": {
                "hoprBalance": {
                  "__typename": "HoprBalance",
                  "balance": "1234567890"
                }
              }
            }
        "#,
        )
        .create_async()
        .await;

    let balance = cli.query_token_balance(&[1u8; 20]).await?;
    assert_eq!("1234567890", balance.balance.0);

    mock.assert_async().await;

    insta::assert_yaml_snapshot!(recorder.requests());

    Ok(())
}

#[tokio::test]
async fn query_compatibility() -> anyhow::Result<()> {
    let mut server = mockito::Server::new_async().await;

    let cli = BlokliClient::new(server.url().parse()?, Default::default());

    let recorder = RequestRecorder::default();

    let mock = server
        .mock("POST", "/graphql")
        .with_status(200)
        .match_request(recorder.as_matcher())
        .with_header("content-type", "application/json")
        .with_body(
            r#"{
              "data": {
                "compatibility": {
                  "apiVersion": "0.19.1",
                  "supportedClientVersions": "^0.24"
                }
              }
            }
        "#,
        )
        .create_async()
        .await;

    let compatibility = cli.query_compatibility().await?;
    assert_eq!(compatibility.api_version, "0.19.1");
    assert_eq!(compatibility.supported_client_versions, "^0.24");

    mock.assert_async().await;

    let requests = recorder.requests();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].method, "POST");
    assert_eq!(requests[0].path_and_query, "/graphql");
    assert_eq!(
        requests[0].body,
        Some(Body::Json(json!({
            "operationName": "QueryCompatibility",
            "query": "query QueryCompatibility {\n  compatibility {\n    apiVersion\n    supportedClientVersions\n  }\n}\n",
            "variables": null
        })))
    );

    Ok(())
}
