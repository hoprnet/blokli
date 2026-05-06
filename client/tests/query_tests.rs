use blokli_client::{BlokliClient, CLIENT_VERSION, api::BlokliQueryClient};
use mockito::Matcher;
use serde_json::json;

use crate::common::{Body, RequestRecorder};

mod common;

#[tokio::test]
async fn query_native_balance() -> anyhow::Result<()> {
    let mut server = mockito::Server::new_async().await;

    let cli = BlokliClient::new(server.url().parse()?, Default::default());

    let recorder = RequestRecorder::default();

    let compatibility_mock = server
        .mock("POST", "/graphql")
        .match_body(Matcher::Regex("QueryCompatibility".into()))
        .with_status(200)
        .match_request(recorder.as_matcher())
        .with_header("content-type", "application/json")
        .with_body(
            json!({
              "data": {
                "compatibility": {
                  "apiVersion": "0.19.1",
                  "supportedClientVersions": format!("={CLIENT_VERSION}"),
                  "features": ["indexes_safe_events"]
                }
              }
            })
            .to_string(),
        )
        .create_async()
        .await;

    let balance_mock = server
        .mock("POST", "/graphql")
        .match_body(Matcher::Regex("QueryNativeBalance".into()))
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

    compatibility_mock.assert_async().await;
    balance_mock.assert_async().await;

    insta::assert_yaml_snapshot!(recorder.requests());

    Ok(())
}

#[tokio::test]
async fn query_token_balance() -> anyhow::Result<()> {
    let mut server = mockito::Server::new_async().await;

    let cli = BlokliClient::new(server.url().parse()?, Default::default());

    let recorder = RequestRecorder::default();

    let compatibility_mock = server
        .mock("POST", "/graphql")
        .match_body(Matcher::Regex("QueryCompatibility".into()))
        .with_status(200)
        .match_request(recorder.as_matcher())
        .with_header("content-type", "application/json")
        .with_body(
            json!({
              "data": {
                "compatibility": {
                  "apiVersion": "0.19.1",
                  "supportedClientVersions": format!("={CLIENT_VERSION}"),
                  "features": ["indexes_safe_events"]
                }
              }
            })
            .to_string(),
        )
        .create_async()
        .await;

    let balance_mock = server
        .mock("POST", "/graphql")
        .match_body(Matcher::Regex("QueryHoprBalance".into()))
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

    compatibility_mock.assert_async().await;
    balance_mock.assert_async().await;

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
                  "supportedClientVersions": "^0.24",
                  "features": ["indexes_safe_events"]
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
    assert_eq!(compatibility.features, vec!["indexes_safe_events"]);

    mock.assert_async().await;

    let requests = recorder.requests();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].method, "POST");
    assert_eq!(requests[0].path_and_query, "/graphql");
    assert_eq!(
        requests[0].body,
        Some(Body::Json(json!({
            "operationName": "QueryCompatibility",
            "query": "query QueryCompatibility {\n  compatibility {\n    apiVersion\n    supportedClientVersions\n    features\n  }\n}\n",
            "variables": null
        })))
    );

    Ok(())
}

#[tokio::test]
async fn query_safe_returns_safes_list() -> anyhow::Result<()> {
    let mut server = mockito::Server::new_async().await;
    let cli = BlokliClient::new(server.url().parse()?, Default::default());

    let mock = server
        .mock("POST", "/graphql")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(
            r#"{
              "data": {
                "safeBy": {
                  "__typename": "SafesList",
                  "safes": [
                    {
                      "address": "0x1111111111111111111111111111111111111111",
                      "chainKey": "0x3333333333333333333333333333333333333333",
                      "owners": ["0x7777777777777777777777777777777777777777"],
                      "moduleAddress": "0x2222222222222222222222222222222222222222",
                      "registeredNodes": ["0x8888888888888888888888888888888888888888"],
                      "threshold": "2"
                    }
                  ]
                }
              }
            }"#,
        )
        .create_async()
        .await;

    let safes = cli.query_safe(SafeSelector::Owner([0x77; 20])).await?;
    assert_eq!(safes.len(), 1);
    assert_eq!(safes[0].address, "0x1111111111111111111111111111111111111111");
    assert_eq!(safes[0].owners, vec!["0x7777777777777777777777777777777777777777"]);

    mock.assert_async().await;
    Ok(())
}

#[tokio::test]
async fn query_safe_returns_empty_vec_when_safe_by_is_null() -> anyhow::Result<()> {
    let mut server = mockito::Server::new_async().await;
    let cli = BlokliClient::new(server.url().parse()?, Default::default());

    let mock = server
        .mock("POST", "/graphql")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(
            r#"{
              "data": {
                "safeBy": null
              }
            }"#,
        )
        .create_async()
        .await;

    let safes = cli.query_safe(SafeSelector::Owner([0x77; 20])).await?;
    assert!(safes.is_empty());

    mock.assert_async().await;
    Ok(())
}
