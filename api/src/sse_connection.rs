//! Log lines for the life of one SSE subscription connection.
//!
//! A connection logs one line when it opens and one when it closes, with how long it lasted, how
//! many events it carried, how many of them were errors, and why it ended
//! (hoprnet/hopr-integration-tests#45). The close line is logged when the connection's event stream
//! is dropped, so it is written however the connection ends.

use std::{convert::Infallible, time::Instant};

use async_graphql::Response as GraphQlResponse;
use async_stream::stream;
use axum::response::sse::Event;
use futures::stream::{Stream, StreamExt};
use tracing::{debug, info};
use uuid::Uuid;

use crate::metrics;

/// Why an SSE connection closed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CloseReason {
    /// The client went away (it closed the connection, or writing to it failed) while the
    /// subscription was still running. Clients reconnect, so this is routine.
    ClientGone,
    /// The subscription stream ended on the server, including normal completion after a
    /// transaction reaches a terminal status. Logged at DEBUG unless it carried GraphQL errors.
    StreamEnded,
}

impl CloseReason {
    fn as_str(self) -> &'static str {
        match self {
            Self::ClientGone => "client_gone",
            Self::StreamEnded => "stream_ended",
        }
    }
}

/// Tracks one SSE connection and logs its close line when dropped.
pub(crate) struct SseConnectionLog {
    connection_id: Uuid,
    subscription: String,
    opened: Instant,
    events: u64,
    /// Events that carried GraphQL errors.
    errors: u64,
    stream_ended: bool,
}

impl SseConnectionLog {
    /// Logs the open line and starts tracking the connection.
    pub(crate) fn open(connection_id: Uuid, subscription: &str) -> Self {
        debug!(connection = %connection_id, subscription, "SSE connection established");
        Self {
            connection_id,
            subscription: subscription.to_owned(),
            opened: Instant::now(),
            events: 0,
            errors: 0,
            stream_ended: false,
        }
    }

    fn close_reason(&self) -> CloseReason {
        if self.stream_ended {
            CloseReason::StreamEnded
        } else {
            CloseReason::ClientGone
        }
    }
}

impl Drop for SseConnectionLog {
    fn drop(&mut self) {
        let reason = self.close_reason();
        let duration_ms = u64::try_from(self.opened.elapsed().as_millis()).unwrap_or(u64::MAX);
        // Only connections that carried GraphQL errors need an INFO summary. Stream completion
        // is routine for transaction subscriptions.
        if self.errors > 0 {
            info!(
                connection = %self.connection_id,
                subscription = %self.subscription,
                reason = reason.as_str(),
                duration_ms,
                events = self.events,
                errors = self.errors,
                "SSE connection closed"
            );
        } else {
            debug!(
                connection = %self.connection_id,
                subscription = %self.subscription,
                reason = reason.as_str(),
                duration_ms,
                events = self.events,
                errors = self.errors,
                "SSE connection closed"
            );
        }
    }
}

/// Turns a subscription's responses into SSE events, and logs the connection's close line when the
/// returned stream is dropped.
pub(crate) fn subscription_events(
    responses: impl Stream<Item = GraphQlResponse> + Send + 'static,
    connection: SseConnectionLog,
    request_type: &'static str,
) -> impl Stream<Item = Result<Event, Infallible>> + Send + 'static {
    stream! {
        // Moved in by name: the close line must be logged when this stream is dropped, not when
        // the function returns.
        let mut connection = connection;
        futures::pin_mut!(responses);
        while let Some(response) = responses.next().await {
            if !response.errors.is_empty() {
                metrics::increment_errors(request_type);
                connection.errors += 1;
            }
            let json = serde_json::to_string(&response)
                .unwrap_or_else(|_| r#"{"errors":[{"message":"Failed to serialize response"}]}"#.to_string());
            connection.events += 1;
            yield Ok::<_, Infallible>(Event::default().event("next").data(json));
        }
        connection.stream_ended = true;
    }
}

#[cfg(test)]
mod tests {
    use std::{
        io::Write,
        sync::{Arc, Mutex},
    };

    use async_graphql::ServerError;
    use futures::stream;
    use tracing_subscriber::fmt::MakeWriter;

    use super::*;

    /// Collects formatted log output so a test can read it back.
    #[derive(Clone, Default)]
    struct CapturedLogs(Arc<Mutex<Vec<u8>>>);

    impl CapturedLogs {
        fn text(&self) -> String {
            String::from_utf8_lossy(&self.0.lock().unwrap()).into_owned()
        }
    }

    impl Write for CapturedLogs {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(buf);
            Ok(buf.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    impl<'a> MakeWriter<'a> for CapturedLogs {
        type Writer = Self;

        fn make_writer(&'a self) -> Self::Writer {
            self.clone()
        }
    }

    fn capture() -> (CapturedLogs, tracing::subscriber::DefaultGuard) {
        let logs = CapturedLogs::default();
        let subscriber = tracing_subscriber::fmt()
            .with_max_level(tracing::Level::DEBUG)
            .with_ansi(false)
            .without_time()
            .with_writer(logs.clone())
            .finish();
        let guard = tracing::subscriber::set_default(subscriber);
        (logs, guard)
    }

    fn responses(count: usize) -> impl Stream<Item = GraphQlResponse> + Send + 'static {
        stream::iter((0..count).map(|_| GraphQlResponse::default()))
    }

    fn close_line(logs: &CapturedLogs) -> String {
        logs.text()
            .lines()
            .find(|line| line.contains("SSE connection closed"))
            .map(str::to_owned)
            .unwrap_or_default()
    }

    #[tokio::test]
    async fn a_client_that_goes_away_is_logged_at_debug() {
        let (logs, _guard) = capture();

        let connection = SseConnectionLog::open(Uuid::new_v4(), "channel-updated");
        let mut events = Box::pin(subscription_events(
            responses(3).chain(stream::pending()),
            connection,
            "subscription",
        ));
        let _ = events.next().await;
        drop(events);

        let line = close_line(&logs);
        assert!(line.contains("DEBUG"), "{line}");
        assert!(line.contains("reason=\"client_gone\""), "{line}");
        assert!(line.contains("events=1"), "{line}");
        assert!(line.contains("subscription=channel-updated"), "{line}");
    }

    #[tokio::test]
    async fn a_subscription_stream_that_ends_without_errors_is_logged_at_debug() {
        let (logs, _guard) = capture();

        let connection = SseConnectionLog::open(Uuid::new_v4(), "safe-deployed");
        let events: Vec<_> = subscription_events(responses(2), connection, "subscription")
            .collect()
            .await;
        assert_eq!(events.len(), 2);

        let line = close_line(&logs);
        assert!(line.contains("DEBUG"), "{line}");
        assert!(line.contains("reason=\"stream_ended\""), "{line}");
        assert!(line.contains("events=2"), "{line}");
        assert!(line.contains("errors=0"), "{line}");
    }

    #[tokio::test]
    async fn a_connection_that_carried_errors_is_logged_at_info() {
        let (logs, _guard) = capture();

        let connection = SseConnectionLog::open(Uuid::new_v4(), "channel-updated");
        let failed = GraphQlResponse::from_errors(vec![ServerError::new("resolver failed", None)]);
        let mut events = Box::pin(subscription_events(
            stream::iter([failed]).chain(stream::pending()),
            connection,
            "subscription",
        ));
        let _ = events.next().await;
        drop(events);

        let line = close_line(&logs);
        assert!(line.contains(" INFO "), "{line}");
        assert!(line.contains("reason=\"client_gone\""), "{line}");
        assert!(line.contains("errors=1"), "{line}");
    }

    #[tokio::test]
    async fn the_open_and_close_lines_carry_the_same_connection_id() {
        let (logs, _guard) = capture();

        let connection_id = Uuid::new_v4();
        drop(SseConnectionLog::open(connection_id, "health"));

        let text = logs.text();
        let open = text
            .lines()
            .find(|l| l.contains("SSE connection established"))
            .unwrap_or_default();
        assert!(open.contains(&connection_id.to_string()), "{text}");
        assert!(close_line(&logs).contains(&connection_id.to_string()), "{text}");
    }
}
