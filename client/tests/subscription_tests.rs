use std::time::Duration;

use anyhow::{Context, Result, anyhow};
use blokli_client::{BlokliClient, BlokliClientConfig, api::BlokliSubscriptionClient};
use futures::StreamExt;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
};
use url::Url;

const MINIMUM_TICKET_PRICE: &str = "0.0005 wxHOPR";
const MINIMUM_WINNING_PROBABILITY: f64 = 0.01;

#[tokio::test]
/// Tests that the `subscribe_ticket_params` method can recover from a transient disconnect in the subscription stream.
/// It pawns a local TCP server that simulates a flaky SSE endpoint for the `ticketParametersUpdated` subscription. The
/// server first accepts a connection and immediately closes it, simulating a transient disconnect. It then accepts a
/// second connection and sends a valid SSE response with ticket parameters.
async fn subscribe_ticket_params_recovers_after_transient_disconnect() -> Result<()> {
    let (base_url, server) = spawn_flaky_ticket_params_server().await?;
    let client = BlokliClient::new(
        base_url,
        BlokliClientConfig {
            timeout: Duration::from_secs(2),
            stream_reconnect_timeout: Duration::from_secs(2),
            subscription_stream_restart_delay: Duration::from_millis(200),
        },
    );

    let mut stream = client.subscribe_ticket_params()?;
    let update = tokio::time::timeout(Duration::from_secs(10), stream.next())
        .await
        .context("timed out waiting for ticket params update")?
        .context("subscription stream ended unexpectedly")??;

    assert_eq!(update.ticket_price.0, MINIMUM_TICKET_PRICE);
    assert_eq!(update.min_ticket_winning_probability, MINIMUM_WINNING_PROBABILITY);

    server.await??;
    Ok(())
}

async fn spawn_flaky_ticket_params_server() -> Result<(Url, tokio::task::JoinHandle<Result<()>>)> {
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let addr = listener.local_addr()?;
    let base_url = Url::parse(&format!("http://{addr}"))?;

    let server = tokio::spawn(async move {
        let first_response = concat!(
            "HTTP/1.1 200 OK\r\n",
            "content-type: text/event-stream\r\n",
            "cache-control: no-cache\r\n",
            "connection: close\r\n",
            "content-length: 0\r\n",
            "\r\n",
        );

        let second_payload = serde_json::json!({
            "data": {
                "ticketParametersUpdated": {
                    "minTicketWinningProbability": MINIMUM_WINNING_PROBABILITY,
                    "ticketPrice": MINIMUM_TICKET_PRICE,
                },
            },
        });
        let second_body = format!("event: next\ndata: {second_payload}\n\n");
        let second_response = format!(
            concat!(
                "HTTP/1.1 200 OK\r\n",
                "content-type: text/event-stream\r\n",
                "cache-control: no-cache\r\n",
                "connection: close\r\n",
                "content-length: {}\r\n",
                "\r\n",
                "{}",
            ),
            second_body.len(),
            second_body,
        );

        let (mut first_conn, _) = listener.accept().await?;
        read_request_headers(&mut first_conn).await?;
        first_conn.write_all(first_response.as_bytes()).await?;
        first_conn.shutdown().await?;

        let (mut second_conn, _) = listener.accept().await?;
        read_request_headers(&mut second_conn).await?;
        second_conn.write_all(second_response.as_bytes()).await?;
        second_conn.shutdown().await?;

        Ok(())
    });

    Ok((base_url, server))
}

async fn read_request_headers(stream: &mut TcpStream) -> Result<()> {
    let mut data = Vec::new();
    let mut chunk = [0u8; 1024];

    loop {
        let bytes = stream.read(&mut chunk).await?;
        if bytes == 0 {
            return Err(anyhow!("connection closed before complete request headers"));
        }
        data.extend_from_slice(&chunk[..bytes]);
        if data.windows(4).any(|window| window == b"\r\n\r\n") {
            return Ok(());
        }
    }
}
