use std::{net::IpAddr, time::Duration};

use anyhow::Result;
use blokli_client::{
    BlokliClient, BlokliClientConfig, BlokliDnsOverride, CLIENT_VERSION,
    api::{BlokliSubscriptionClient, types::ChannelStatus},
};
use futures::StreamExt;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
};
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

#[tokio::test]
async fn subscribe_ticket_params_uses_dns_override() -> Result<()> {
    let expected_ticket_params = TicketParams {
        ticket_price: "0.0010 wxHOPR".to_string(),
        min_ticket_winning_probability: 0.25,
    };
    let (base_url, server) = spawn_dns_override_streaming_server(vec![expected_ticket_params.clone()]).await?;
    let expected_host = format!(
        "{}:{}",
        base_url
            .host_str()
            .ok_or_else(|| anyhow::anyhow!("missing base URL host"))?,
        base_url
            .port_or_known_default()
            .ok_or_else(|| anyhow::anyhow!("missing base URL port"))?,
    );
    let client = BlokliClient::new(
        base_url,
        BlokliClientConfig {
            dns_override: Some(BlokliDnsOverride {
                ip: IpAddr::from([127, 0, 0, 1]),
                port: None,
            }),
            auto_compatibility_check: true,
            timeout: Duration::from_secs(2),
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

    let request = server.await??;
    assert!(request.contains(&format!("\r\nhost: {expected_host}\r\n")));

    Ok(())
}

#[tokio::test]
async fn subscribe_graph_forwards_closed_channel_entries() -> Result<()> {
    let channel_id = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    let (base_url, server) = spawn_single_streaming_server(format_graph_event(channel_id, "CLOSED")).await?;
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

    let mut stream = client.subscribe_graph()?;
    let entry = tokio::time::timeout(Duration::from_secs(2), stream.next())
        .await?
        .ok_or_else(|| anyhow::anyhow!("subscription ended before delivering graph event"))??;

    assert_eq!(entry.channel.concrete_channel_id, channel_id);
    assert_eq!(entry.channel.status, ChannelStatus::Closed);
    assert_eq!(entry.source.keyid, 1);
    assert_eq!(entry.destination.keyid, 2);

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
            .write_all(format_json_response(&compatibility_response_body()).as_bytes())
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

async fn spawn_dns_override_streaming_server(
    events: Vec<TicketParams>,
) -> Result<(Url, tokio::task::JoinHandle<Result<String>>)> {
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let base_url = Url::parse(&format!(
        "http://blokli-stream.invalid:{}",
        listener.local_addr()?.port()
    ))?;

    let server = tokio::spawn(async move {
        let (mut compatibility_conn, _) = listener.accept().await?;
        compatibility_conn
            .write_all(format_json_response(&compatibility_response_body()).as_bytes())
            .await?;
        compatibility_conn.shutdown().await?;

        let body = format_ticket_params_events(&events);
        let (mut conn, _) = listener.accept().await?;
        let request = read_http_request_head(&mut conn).await?;
        conn.write_all(format_sse_response(&body).as_bytes()).await?;
        conn.shutdown().await?;
        Ok(request)
    });

    Ok((base_url, server))
}

async fn read_http_request_head(conn: &mut tokio::net::TcpStream) -> Result<String> {
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

async fn spawn_delayed_streaming_server(
    events: Vec<TicketParams>,
    initial_delay: Duration,
) -> Result<(Url, tokio::task::JoinHandle<Result<()>>)> {
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let base_url = Url::parse(&format!("http://{}", listener.local_addr()?))?;

    let server = tokio::spawn(async move {
        let (mut compatibility_conn, _) = listener.accept().await?;
        compatibility_conn
            .write_all(format_json_response(&compatibility_response_body()).as_bytes())
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
            .write_all(format_json_response(&compatibility_response_body()).as_bytes())
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

async fn spawn_single_streaming_server(body: String) -> Result<(Url, tokio::task::JoinHandle<Result<()>>)> {
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let base_url = Url::parse(&format!("http://{}", listener.local_addr()?))?;

    let server = tokio::spawn(async move {
        let (mut compatibility_conn, _) = listener.accept().await?;
        compatibility_conn
            .write_all(format_json_response(&compatibility_response_body()).as_bytes())
            .await?;
        compatibility_conn.shutdown().await?;

        let (mut conn, _) = listener.accept().await?;
        conn.write_all(format_sse_response(&body).as_bytes()).await?;
        conn.shutdown().await?;
        Ok(())
    });

    Ok((base_url, server))
}

fn format_graph_event(channel_id: &str, status: &str) -> String {
    let payload = serde_json::json!({
        "data": {
            "openedChannelGraphUpdated": {
                "channel": {
                    "balance": "0 wxHOPR",
                    "closureTime": null,
                    "concreteChannelId": channel_id,
                    "destination": 2,
                    "epoch": 1,
                    "source": 1,
                    "status": status,
                    "ticketIndex": "0",
                },
                "destination": {
                    "chainKey": "0x2222222222222222222222222222222222222222",
                    "keyid": 2,
                    "multiAddresses": [],
                    "packetKey": "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
                    "safeAddress": null,
                },
                "source": {
                    "chainKey": "0x1111111111111111111111111111111111111111",
                    "keyid": 1,
                    "multiAddresses": [],
                    "packetKey": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                    "safeAddress": null,
                },
            },
        },
    });
    format!("event: next\ndata: {payload}\n\n")
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

fn compatibility_response_body() -> String {
    format!(
        r#"{{"data":{{"compatibility":{{"apiVersion":"0.19.1","supportedClientVersions":"={CLIENT_VERSION}","features":["indexes_safe_events"]}}}}}}"#
    )
}
