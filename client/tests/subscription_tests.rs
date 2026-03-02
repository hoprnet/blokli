use std::time::Duration;

use anyhow::{Context, Result};
use blokli_client::{BlokliClient, BlokliClientConfig, api::BlokliSubscriptionClient};
use futures::StreamExt;
use tokio::{io::AsyncWriteExt, net::TcpListener};
use url::Url;

#[derive(Clone, Debug, PartialEq, serde::Serialize)]
struct TicketParamsSnapshot {
    ticket_price: String,
    min_ticket_winning_probability: f64,
}

impl TicketParamsSnapshot {
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
        TicketParamsSnapshot {
            ticket_price: "0.0010 wxHOPR".to_string(),
            min_ticket_winning_probability: 0.25,
        },
        TicketParamsSnapshot {
            ticket_price: "0.0020 wxHOPR".to_string(),
            min_ticket_winning_probability: 0.5,
        },
        TicketParamsSnapshot {
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
            timeout: Duration::from_secs(2),
            stream_reconnect_timeout: Duration::from_secs(2),
            subscription_stream_restart_delay: Duration::from_millis(100),
        },
    );

    let mut stream = client.subscribe_ticket_params()?;
    let mut updates = Vec::new();
    while updates.len() < 3 {
        let update = tokio::time::timeout(Duration::from_secs(10), stream.next())
            .await
            .context("timed out waiting for ticket params update")?
            .context("subscription stream ended unexpectedly")??;

        updates.push(TicketParamsSnapshot {
            ticket_price: update.ticket_price.0,
            min_ticket_winning_probability: update.min_ticket_winning_probability,
        });
    }

    assert_eq!(updates, expected_ticket_params);

    server.await??;
    Ok(())
}

async fn spawn_reconnecting_server(
    event_batches: Vec<Vec<TicketParamsSnapshot>>,
) -> Result<(Url, tokio::task::JoinHandle<Result<()>>)> {
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let base_url = Url::parse(&format!("http://{}", listener.local_addr()?))?;

    let server = tokio::spawn(async move {
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

fn format_ticket_params_events(events: &[TicketParamsSnapshot]) -> String {
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
