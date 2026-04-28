use std::time::Duration;

use anyhow::Result;
use blokli_client::{BlokliClient, BlokliClientConfig, api::BlokliSubscriptionClient};
use futures::StreamExt;
use tokio::{io::AsyncWriteExt, net::TcpListener};
use url::Url;

#[derive(Clone, Debug, PartialEq, serde::Serialize)]
struct TicketParams {
    ticket_price: String,
    min_ticket_winning_probability: f64,
}

impl TicketParams {
    fn format_event(&self) -> String {
        let payload = serde_json::json!({
            "data": {
                "ticketParametersUpdated": {
                    "minTicketWinningProbability": self.min_ticket_winning_probability,
                    "ticketPrice": self.ticket_price,
                }
            }
        });
        format!("event: next\ndata: {payload}\n\n")
    }
}

#[tokio::test]
async fn subscribe_ticket_params_recreates_stream_without_loss_or_duplication() -> Result<()> {
    let expected_ticket_params = vec![
        TicketParams {
            ticket_price: "0.0010 wxHOPR".to_string(),
            min_ticket_winning_probability: 0.25,
        },
        TicketParams {
            ticket_price: "0.0020 wxHOPR".to_string(),
            min_ticket_winning_probability: 0.5,
        },
        TicketParams {
            ticket_price: "0.0030 wxHOPR".to_string(),
            min_ticket_winning_probability: 0.75,
        },
    ];
    let stream_batches = vec![
        expected_ticket_params[..2].to_vec(),
        expected_ticket_params[2..].to_vec(),
    ];
    let (base_url, server) = spawn_reconnecting_server(stream_batches).await?;
    let client = BlokliClient::new(
        base_url,
        BlokliClientConfig {
            auto_compatibility_check: true,
            timeout: Duration::from_secs(2),
            stream_reconnect_timeout: Duration::from_secs(2),
            subscription_read_timeout: Some(Duration::from_secs(2)),
            subscription_tcp_keepalive: Duration::from_secs(15),
            subscription_stream_restart_delay: Some(Duration::from_millis(100)),
            ..Default::default()
        },
    );

    let stream = client.subscribe_ticket_params()?;
    let updates = futures::stream::unfold(stream, |mut stream| async move {
        let update_result = tokio::time::timeout(Duration::from_secs(2), stream.next()).await;
        match update_result {
            Ok(Some(Ok(update))) => Some((
                Ok(TicketParams {
                    ticket_price: update.ticket_price.0,
                    min_ticket_winning_probability: update.min_ticket_winning_probability,
                }),
                stream,
            )),
            Ok(Some(Err(error))) => Some((Err(error.into()), stream)),
            Ok(None) | Err(_) => None,
        }
    })
    .take(10)
    .collect::<Vec<Result<TicketParams>>>()
    .await
    .into_iter()
    .collect::<Result<Vec<TicketParams>>>()?;

    assert_eq!(updates, expected_ticket_params);

    server.await??;
    Ok(())
}

#[tokio::test]
async fn subscribe_ticket_params_stays_open_beyond_non_streaming_timeout() -> Result<()> {
    let expected_ticket_params = TicketParams {
        ticket_price: "0.0010 wxHOPR".to_string(),
        min_ticket_winning_probability: 0.25,
    };
    let (base_url, server) =
        spawn_delayed_streaming_server(vec![expected_ticket_params.clone()], Duration::from_millis(250)).await?;
    let client = BlokliClient::new(
        base_url,
        BlokliClientConfig {
            auto_compatibility_check: true,
            timeout: Duration::from_millis(100),
            stream_reconnect_timeout: Duration::from_secs(2),
            subscription_read_timeout: Some(Duration::from_secs(2)),
            subscription_tcp_keepalive: Duration::from_secs(15),
            subscription_stream_restart_delay: Some(Duration::from_millis(100)),
            ..Default::default()
        },
    );

    let mut stream = client.subscribe_ticket_params()?;
    let update = tokio::time::timeout(Duration::from_secs(2), stream.next())
        .await?
        .ok_or_else(|| anyhow::anyhow!("subscription ended before delivering an event"))??;

    assert_eq!(update.ticket_price.0, expected_ticket_params.ticket_price);
    assert_eq!(
        update.min_ticket_winning_probability,
        expected_ticket_params.min_ticket_winning_probability
    );

    server.await??;
    Ok(())
}

#[tokio::test]
async fn subscribe_ticket_params_reconnects_after_read_timeout() -> Result<()> {
    let expected_ticket_params = TicketParams {
        ticket_price: "0.0010 wxHOPR".to_string(),
        min_ticket_winning_probability: 0.25,
    };
    let (base_url, server) = spawn_timed_out_then_reconnecting_server(expected_ticket_params.clone()).await?;
    let client = BlokliClient::new(
        base_url,
        BlokliClientConfig {
            auto_compatibility_check: true,
            timeout: Duration::from_secs(2),
            stream_reconnect_timeout: Duration::from_millis(250),
            subscription_read_timeout: Some(Duration::from_millis(100)),
            subscription_tcp_keepalive: Duration::from_secs(15),
            subscription_stream_restart_delay: Some(Duration::from_millis(100)),
            ..Default::default()
        },
    );

    let mut stream = client.subscribe_ticket_params()?;
    let update = tokio::time::timeout(Duration::from_secs(2), stream.next())
        .await?
        .ok_or_else(|| anyhow::anyhow!("subscription ended before reconnecting"))??;

    assert_eq!(update.ticket_price.0, expected_ticket_params.ticket_price);
    assert_eq!(
        update.min_ticket_winning_probability,
        expected_ticket_params.min_ticket_winning_probability
    );

    server.await??;
    Ok(())
}

async fn spawn_reconnecting_server(
    event_batches: Vec<Vec<TicketParams>>,
) -> Result<(Url, tokio::task::JoinHandle<Result<()>>)> {
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let base_url = Url::parse(&format!("http://{}", listener.local_addr()?))?;

    let server = tokio::spawn(async move {
        let (mut compatibility_conn, _) = listener.accept().await?;
        compatibility_conn
            .write_all(format_json_response(compatibility_response_body()).as_bytes())
            .await?;
        compatibility_conn.shutdown().await?;

        let responses = event_batches
            .iter()
            .map(|events| format_sse_response(&format_ticket_params_events(events)));

        for response in responses {
            let (mut conn, _) = listener.accept().await?;
            conn.write_all(response.as_bytes()).await?;
            conn.shutdown().await?;
        }

        Ok(())
    });

    Ok((base_url, server))
}

async fn spawn_delayed_streaming_server(
    events: Vec<TicketParams>,
    initial_delay: Duration,
) -> Result<(Url, tokio::task::JoinHandle<Result<()>>)> {
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let base_url = Url::parse(&format!("http://{}", listener.local_addr()?))?;

    let server = tokio::spawn(async move {
        let (mut compatibility_conn, _) = listener.accept().await?;
        compatibility_conn
            .write_all(format_json_response(compatibility_response_body()).as_bytes())
            .await?;
        compatibility_conn.shutdown().await?;

        let response_headers = format!(concat!(
            "HTTP/1.1 200 OK\r\n",
            "content-type: text/event-stream\r\n",
            "cache-control: no-cache\r\n",
            "connection: close\r\n",
            "\r\n",
        ),);
        let body = format_ticket_params_events(&events);
        let (mut conn, _) = listener.accept().await?;
        conn.write_all(response_headers.as_bytes()).await?;
        tokio::time::sleep(initial_delay).await;
        conn.write_all(body.as_bytes()).await?;
        conn.shutdown().await?;
        Ok(())
    });

    Ok((base_url, server))
}

async fn spawn_timed_out_then_reconnecting_server(
    event: TicketParams,
) -> Result<(Url, tokio::task::JoinHandle<Result<()>>)> {
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let base_url = Url::parse(&format!("http://{}", listener.local_addr()?))?;

    let server = tokio::spawn(async move {
        let (mut compatibility_conn, _) = listener.accept().await?;
        compatibility_conn
            .write_all(format_json_response(compatibility_response_body()).as_bytes())
            .await?;
        compatibility_conn.shutdown().await?;

        let response_headers = format!(concat!(
            "HTTP/1.1 200 OK\r\n",
            "content-type: text/event-stream\r\n",
            "cache-control: no-cache\r\n",
            "connection: close\r\n",
            "\r\n",
        ),);

        let (mut first_conn, _) = listener.accept().await?;
        first_conn.write_all(response_headers.as_bytes()).await?;
        tokio::time::sleep(Duration::from_millis(300)).await;
        first_conn.shutdown().await?;

        let (mut second_conn, _) = listener.accept().await?;
        let body = format_ticket_params_events(&[event]);
        second_conn
            .write_all(format!("{response_headers}{body}").as_bytes())
            .await?;
        second_conn.shutdown().await?;

        Ok(())
    });

    Ok((base_url, server))
}

fn format_ticket_params_events(events: &[TicketParams]) -> String {
    events
        .iter()
        .map(|event| event.format_event())
        .collect::<Vec<String>>()
        .join("")
}

fn format_sse_response(body: &str) -> String {
    format!(
        concat!(
            "HTTP/1.1 200 OK\r\n",
            "content-type: text/event-stream\r\n",
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

fn compatibility_response_body() -> &'static str {
    r#"{"data":{"compatibility":{"apiVersion":"0.19.1","supportedClientVersions":"^0.26","indexesSafeEvents":true}}}"#
}
