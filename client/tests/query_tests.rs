use std::net::IpAddr;

use blokli_client::{
    BlokliClient, BlokliClientConfig, BlokliDnsOverride, CLIENT_VERSION,
    api::{BlokliQueryClient, SafeSelector},
};
use mockito::Matcher;
use serde_json::json;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
};

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
    let cfg = BlokliClientConfig {
        auto_compatibility_check: false,
        ..Default::default()
    };
    let cli = BlokliClient::new(server.url().parse()?, cfg);

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
    let cfg = BlokliClientConfig {
        auto_compatibility_check: false,
        ..Default::default()
    };
    let cli = BlokliClient::new(server.url().parse()?, cfg);

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

#[test]
fn default_config_uses_system_dns() {
    assert_eq!(BlokliClientConfig::default().dns_override, None);
}

#[tokio::test]
async fn query_uses_dns_override_without_rewriting_host() -> anyhow::Result<()> {
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let port = listener.local_addr()?.port();
    let base_url = format!("http://blokli.invalid:{port}").parse()?;
    let client = BlokliClient::new(
        base_url,
        BlokliClientConfig {
            auto_compatibility_check: false,
            dns_override: Some(BlokliDnsOverride {
                ip: IpAddr::from([127, 0, 0, 1]),
                port: None,
            }),
            ..Default::default()
        },
    );

    let server = tokio::spawn(async move {
        let (mut conn, _) = listener.accept().await?;
        let request = read_http_request_head(&mut conn).await?;
        conn.write_all(format_json_response(r#"{"data":{"version":"0.19.1"}}"#).as_bytes())
            .await?;
        conn.shutdown().await?;
        anyhow::Ok(request)
    });

    let version = client.query_version().await?;
    let request = server.await??;

    assert_eq!(version, "0.19.1");
    assert!(request.starts_with("post /graphql http/1.1\r\n"));
    assert!(request.contains(&format!("\r\nhost: blokli.invalid:{port}\r\n")));

    Ok(())
}

#[tokio::test]
async fn query_uses_dns_override_with_explicit_port() -> anyhow::Result<()> {
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let listener_port = listener.local_addr()?.port();
    let base_url = "http://blokli.invalid".parse()?;
    let client = BlokliClient::new(
        base_url,
        BlokliClientConfig {
            auto_compatibility_check: false,
            dns_override: Some(BlokliDnsOverride {
                ip: IpAddr::from([127, 0, 0, 1]),
                port: Some(listener_port),
            }),
            ..Default::default()
        },
    );

    let server = tokio::spawn(async move {
        let (mut conn, _) = listener.accept().await?;
        let request = read_http_request_head(&mut conn).await?;
        conn.write_all(format_json_response(r#"{"data":{"version":"0.19.1"}}"#).as_bytes())
            .await?;
        conn.shutdown().await?;
        anyhow::Ok(request)
    });

    let version = client.query_version().await?;
    let request = server.await??;

    assert_eq!(version, "0.19.1");
    assert!(request.starts_with("post /graphql http/1.1\r\n"));
    assert!(request.contains("\r\nhost: blokli.invalid\r\n"));

    Ok(())
}

async fn read_http_request_head(conn: &mut tokio::net::TcpStream) -> anyhow::Result<String> {
    let mut buf = Vec::new();
    loop {
        let mut chunk = [0_u8; 1024];
        let n = conn.read(&mut chunk).await?;
        if n == 0 {
            break;
        }
        buf.extend_from_slice(&chunk[..n]);
        if buf.windows(4).any(|window| window == b"\r\n\r\n") {
            break;
        }
    }

    Ok(String::from_utf8_lossy(&buf).to_ascii_lowercase())
}

fn format_json_response(body: &str) -> String {
    format!(
        concat!(
            "HTTP/1.1 200 OK\r\n",
            "content-type: application/json\r\n",
            "cache-control: no-cache\r\n",
            "connection: close\r\n",
            "content-length: {}\r\n",
            "\r\n",
            "{}",
        ),
        body.len(),
        body,
    )
}
