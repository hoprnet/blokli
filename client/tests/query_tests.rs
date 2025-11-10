use crate::common::RequestRecorder;
use blokli_client::BlokliClient;
use blokli_client::api::BlokliQueryClient;

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
