//! WebSocket `newHeads` head source, support detection and the background head driver.
//!
//! HTTP remains the transport for all RPC requests. The WebSocket connection is only used to
//! receive `newHeads` notifications, each of which triggers a regular HTTP tip refresh. The
//! subscription is optional: deployments without a usable WebSocket endpoint keep polling. The
//! driver probes the subscription at startup, follows `newHeads` while it is healthy and falls
//! back to polling with exponential-backoff reconnects.

use std::{
    fmt::{self, Formatter},
    future::{Future, pending},
    pin::Pin,
    result::Result as StdResult,
    sync::{Arc, Weak},
    time::Duration,
};

use async_trait::async_trait;
use futures::{StreamExt, stream::BoxStream};
use hopr_bindings::exports::alloy::{
    network::Ethereum,
    primitives::B256,
    providers::{Provider, RootProvider, WsConnect},
    rpc::client::RpcClient,
    transports::{RpcError as AlloyRpcError, TransportErrorKind},
};
#[cfg(all(feature = "telemetry", not(test)))]
use hopr_types::telemetry::SimpleCounter;
use tokio::time::{Instant as TokioInstant, MissedTickBehavior, sleep, sleep_until, timeout_at};
use tracing::{debug, error, info, warn};
use url::Url;

use crate::tip::{HeadSource, TipSource, TrackerRuntime};

/// A header received through the `newHeads` subscription.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HeadHeader {
    /// Block number of the header.
    pub number: u64,
    /// Hash of the block header.
    pub hash: B256,
}

/// Configuration of the WebSocket head source and the polling fallback.
#[derive(Clone)]
pub struct HeadSourceConfig {
    /// Chain id the endpoint must serve.
    pub chain_id: u64,
    /// Interval between polling refreshes.
    pub poll_interval: Duration,
    /// Expected block time, used for probe and silent-socket timeouts.
    pub expected_block_time: Duration,
    /// Effective WebSocket URL, or `None` to poll only.
    pub ws_url: Option<Url>,
    /// Whether [`HeadSourceConfig::ws_url`] was derived from the HTTP RPC URL.
    pub ws_url_derived: bool,
}

impl fmt::Debug for HeadSourceConfig {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_struct("HeadSourceConfig")
            .field("chain_id", &self.chain_id)
            .field("poll_interval", &self.poll_interval)
            .field("expected_block_time", &self.expected_block_time)
            .field("ws_url", &self.ws_url.as_ref().map(redact_ws_url))
            .field("ws_url_derived", &self.ws_url_derived)
            .finish()
    }
}

/// Errors produced while opening or reading a `newHeads` subscription.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum HeadError {
    /// The endpoint does not support `newHeads` subscriptions.
    #[error("head subscription is not supported: {reason}")]
    Unsupported {
        /// Error text reported by the endpoint or the transport.
        reason: String,
        /// JSON-RPC error code, when the endpoint returned an error response.
        code: Option<i64>,
    },
    /// The connection failed or closed, or the endpoint stopped sending headers.
    #[error("head subscription connection failed: {reason}")]
    Connection {
        /// Error text reported by the endpoint or the transport.
        reason: String,
        /// JSON-RPC error code, when the endpoint returned an error response.
        code: Option<i64>,
    },
}

impl HeadError {
    /// Creates an unsupported-subscription error.
    pub fn unsupported(reason: impl Into<String>, code: Option<i64>) -> Self {
        Self::Unsupported {
            reason: reason.into(),
            code,
        }
    }

    /// Creates a connection error.
    pub fn connection(reason: impl Into<String>, code: Option<i64>) -> Self {
        Self::Connection {
            reason: reason.into(),
            code,
        }
    }

    /// Returns `true` if the error means the endpoint cannot serve `newHeads` at all.
    pub fn is_unsupported(&self) -> bool {
        matches!(self, Self::Unsupported { .. })
    }

    /// Returns the JSON-RPC error code, when the endpoint returned an error response.
    pub fn code(&self) -> Option<i64> {
        match self {
            Self::Unsupported { code, .. } | Self::Connection { code, .. } => *code,
        }
    }
}

/// An open `newHeads` subscription.
pub struct HeadSubscription {
    /// Chain id reported by the WebSocket endpoint.
    pub chain_id: u64,
    /// Stream of subsequent headers.
    pub headers: BoxStream<'static, StdResult<HeadHeader, HeadError>>,
}

impl fmt::Debug for HeadSubscription {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_struct("HeadSubscription")
            .field("chain_id", &self.chain_id)
            .finish_non_exhaustive()
    }
}

/// Opens `newHeads` subscriptions.
#[async_trait]
pub trait HeadSubscriber: Send + Sync {
    /// Connects to the endpoint and subscribes to `newHeads`.
    async fn subscribe(&self) -> StdResult<HeadSubscription, HeadError>;
}

/// Errors produced when deriving a WebSocket URL from an HTTP RPC URL.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum HeadUrlError {
    /// The source URL does not use the `http` or `https` scheme.
    #[error("cannot derive a WebSocket URL from scheme '{0}'")]
    UnsupportedScheme(String),
}

/// Derives the WebSocket URL corresponding to an HTTP RPC URL.
///
/// `http` becomes `ws` and `https` becomes `wss`. Host, port, path, query and userinfo are kept.
pub fn derive_ws_url(rpc_url: &Url) -> StdResult<Url, HeadUrlError> {
    let scheme = match rpc_url.scheme() {
        "http" => "ws",
        "https" => "wss",
        other => return Err(HeadUrlError::UnsupportedScheme(other.to_string())),
    };

    let mut ws_url = rpc_url.clone();
    ws_url
        .set_scheme(scheme)
        .map_err(|_| HeadUrlError::UnsupportedScheme(rpc_url.scheme().to_string()))?;
    Ok(ws_url)
}

/// Resolves the effective WebSocket URL for the head source.
///
/// Returns the URL and whether it was derived from `rpc_url`. An explicit `rpc_ws_url` wins
/// over the derived URL, and disabling the subscription forces polling.
pub fn resolve_head_ws_url(
    rpc_url: &Url,
    rpc_ws_url: Option<&Url>,
    head_subscription: bool,
) -> StdResult<Option<(Url, bool)>, HeadUrlError> {
    if !head_subscription {
        return Ok(None);
    }

    match rpc_ws_url {
        Some(url) => Ok(Some((url.clone(), false))),
        None => derive_ws_url(rpc_url).map(|url| Some((url, true))),
    }
}

/// Classifies a JSON-RPC error as either unsupported or connection-related.
///
/// Only a JSON-RPC error response can mean that the endpoint does not support `newHeads`.
/// Transport-level failures, HTTP errors and socket events are always transient. The
/// classification only looks at the error code and message so it can be tested without a network.
pub fn classify_head_error(is_error_response: bool, code: Option<i64>, message: &str) -> HeadError {
    let message = message.to_ascii_lowercase();
    // Only an explicit statement that the notification method or subscriptions are missing is
    // terminal. Generic availability wording such as "service is not available" describes a
    // temporary endpoint failure and must stay retryable.
    let unsupported = is_error_response
        && (code == Some(-32601)
            || [
                "method not found",
                "notifications not supported",
                "subscriptions are not supported",
                "subscriptions not supported",
                "subscription not supported",
                "does not exist",
            ]
            .iter()
            .any(|phrase| message.contains(phrase)));

    if unsupported {
        HeadError::unsupported(message, code)
    } else {
        HeadError::connection(message, code)
    }
}

fn classify_alloy_error(error: &AlloyRpcError<TransportErrorKind>) -> HeadError {
    match error.as_error_resp() {
        Some(payload) => classify_head_error(true, Some(payload.code), payload.message.as_ref()),
        None => classify_head_error(false, None, error.to_string().as_str()),
    }
}

/// Formats a WebSocket URL for logs without userinfo, path or query.
pub fn redact_ws_url(url: &Url) -> String {
    let host = url.host_str().unwrap_or("<unknown>");
    match url.port() {
        Some(port) => format!("{}://{host}:{port}", url.scheme()),
        None => format!("{}://{host}", url.scheme()),
    }
}

/// Production [`HeadSubscriber`] backed by an alloy WebSocket connection.
#[derive(Debug, Clone)]
pub struct WsHeadSubscriber {
    url: Url,
}

impl WsHeadSubscriber {
    /// Creates a subscriber for the given WebSocket URL.
    pub fn new(url: Url) -> Self {
        Self { url }
    }
}

#[async_trait]
impl HeadSubscriber for WsHeadSubscriber {
    async fn subscribe(&self) -> StdResult<HeadSubscription, HeadError> {
        let client = RpcClient::connect_pubsub(WsConnect::new(self.url.as_str().to_string()))
            .await
            .map_err(|error| classify_alloy_error(&error))?;
        let provider = RootProvider::<Ethereum>::new(client);

        let chain_id = provider
            .get_chain_id()
            .await
            .map_err(|error| classify_alloy_error(&error))?;
        let subscription = provider
            .subscribe_blocks()
            .await
            .map_err(|error| classify_alloy_error(&error))?;
        let headers = subscription
            .into_stream()
            .map(move |header| {
                // Keep the provider and its WebSocket client alive for as long as the stream is read.
                let _provider = &provider;
                Ok(HeadHeader {
                    number: header.inner.number,
                    hash: header.hash,
                })
            })
            .boxed();

        Ok(HeadSubscription { chain_id, headers })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum ProbeFailure {
    Unsupported { code: Option<i64> },
    Connection { code: Option<i64> },
    ChainIdMismatch { expected: u64, actual: u64 },
    BlockHashMismatch { number: u64, expected: B256, actual: B256 },
}

impl From<HeadError> for ProbeFailure {
    fn from(error: HeadError) -> Self {
        let code = error.code();
        match error {
            HeadError::Unsupported { .. } => Self::Unsupported { code },
            HeadError::Connection { .. } => Self::Connection { code },
        }
    }
}

struct ProbeSuccess {
    subscription: HeadSubscription,
    first_header: HeadHeader,
}

async fn probe_head_source(
    subscriber: Arc<dyn HeadSubscriber>,
    source: Arc<dyn TipSource>,
    expected_chain_id: u64,
    timeout: Duration,
) -> StdResult<ProbeSuccess, ProbeFailure> {
    let deadline = TokioInstant::now() + timeout;

    let mut subscription = match timeout_at(deadline, subscriber.subscribe()).await {
        Err(_) => {
            return Err(ProbeFailure::Connection { code: None });
        }
        Ok(Err(error)) => return Err(error.into()),
        Ok(Ok(subscription)) => subscription,
    };

    if subscription.chain_id != expected_chain_id {
        return Err(ProbeFailure::ChainIdMismatch {
            expected: expected_chain_id,
            actual: subscription.chain_id,
        });
    }

    let first_header = match timeout_at(deadline, subscription.headers.next()).await {
        Err(_) => {
            return Err(ProbeFailure::Connection { code: None });
        }
        Ok(None) => {
            return Err(ProbeFailure::Connection { code: None });
        }
        Ok(Some(Err(error))) => return Err(error.into()),
        Ok(Some(Ok(header))) => header,
    };

    let expected_hash = source
        .block_hash(first_header.number)
        .await
        .map_err(|_| ProbeFailure::Connection { code: None })?;

    if expected_hash != first_header.hash {
        return Err(ProbeFailure::BlockHashMismatch {
            number: first_header.number,
            expected: expected_hash,
            actual: first_header.hash,
        });
    }

    Ok(ProbeSuccess {
        subscription,
        first_header,
    })
}

fn next_reconnect_backoff(current: Duration) -> Duration {
    current.saturating_mul(2).min(Duration::from_secs(60))
}

fn log_probe_failure(failure: &ProbeFailure, ws_url: &Url, ws_url_derived: bool) {
    let url = redact_ws_url(ws_url);
    let hint = if ws_url_derived {
        " set rpc_ws_url to use a different WebSocket endpoint"
    } else {
        ""
    };

    match failure {
        ProbeFailure::Unsupported { code } => {
            warn!(url = %url, code = ?code, category = "unsupported", "WebSocket head source is not supported, using polling{}", hint);
        }
        ProbeFailure::Connection { code } => {
            warn!(url = %url, code = ?code, category = "connection", "WebSocket head source is unavailable, using polling{}", hint);
        }
        ProbeFailure::ChainIdMismatch { expected, actual } => {
            error!(url = %url, expected_chain_id = expected, actual_chain_id = actual, "WebSocket head source serves a different chain, using polling");
        }
        ProbeFailure::BlockHashMismatch {
            number,
            expected,
            actual,
        } => {
            error!(url = %url, block = number, expected_hash = %expected, actual_hash = %actual, "WebSocket head source serves a different chain, using polling");
        }
    }
}

async fn leave_websocket(
    runtime: &TrackerRuntime,
    head: &HeadSourceConfig,
    reason: &str,
    timeout_secs: Option<u64>,
    backoff: &mut Duration,
) -> TokioInstant {
    if let Some(timeout_secs) = timeout_secs {
        warn!(timeout_secs, "{reason}");
    } else {
        warn!("{reason}");
    }

    runtime.set_active_source(HeadSource::Polling);
    if let Some(url) = head.ws_url.as_ref() {
        info!(source = %HeadSource::Polling, url = %redact_ws_url(url), "chain head source switched");
    }
    let _ = runtime.refresh_from(HeadSource::Polling).await;

    let reconnect_at = TokioInstant::now() + *backoff;
    *backoff = next_reconnect_backoff(*backoff);
    reconnect_at
}

async fn next_websocket_header(
    headers: &mut Option<BoxStream<'static, StdResult<HeadHeader, HeadError>>>,
) -> Option<StdResult<HeadHeader, HeadError>> {
    match headers {
        Some(headers) => headers.next().await,
        None => pending().await,
    }
}

async fn refresh_from_header(runtime: &TrackerRuntime, head: &HeadSourceConfig, header: HeadHeader) {
    let needs_follow_up = match runtime.refresh_from(HeadSource::WebSocket).await {
        Ok(tip) => tip.latest < header.number,
        Err(error) => {
            debug!(%error, "background chain tip refresh failed");
            true
        }
    };

    if needs_follow_up {
        sleep(head.poll_interval).await;
        if let Err(error) = runtime.refresh_from(HeadSource::WebSocket).await {
            debug!(%error, "background chain tip refresh retry failed");
        }
    }
}

pub(crate) async fn run_head_driver(
    runtime: Weak<TrackerRuntime>,
    head: HeadSourceConfig,
    subscriber: Option<Arc<dyn HeadSubscriber>>,
) {
    enum State {
        Polling {
            reconnect_at: Option<TokioInstant>,
        },
        Probing {
            future: Pin<Box<dyn Future<Output = StdResult<ProbeSuccess, ProbeFailure>> + Send>>,
        },
        WebSocket {
            headers: BoxStream<'static, StdResult<HeadHeader, HeadError>>,
            last_header: TokioInstant,
        },
    }

    let probe_timeout = head.expected_block_time.saturating_mul(3);
    let ws_url_display = head.ws_url.as_ref().map(redact_ws_url);
    info!(
        source = %HeadSource::Polling,
        websocket = ?ws_url_display,
        "chain head source selected"
    );

    let Some(initial_runtime) = runtime.upgrade() else {
        return;
    };
    initial_runtime.set_active_source(HeadSource::Polling);

    let mut state = match (subscriber.as_ref(), head.ws_url.as_ref()) {
        (Some(subscriber), Some(_)) => State::Probing {
            future: Box::pin(probe_head_source(
                subscriber.clone(),
                initial_runtime.tip_source(),
                head.chain_id,
                probe_timeout,
            )),
        },
        _ => State::Polling { reconnect_at: None },
    };
    drop(initial_runtime);

    let mut interval = tokio::time::interval(head.poll_interval);
    interval.set_missed_tick_behavior(MissedTickBehavior::Delay);
    let mut backoff = Duration::from_secs(1);
    let mut ever_connected = false;

    loop {
        let Some(runtime) = runtime.upgrade() else {
            break;
        };

        match state {
            State::Polling { reconnect_at } => {
                let reconnect = async {
                    match reconnect_at {
                        Some(deadline) => sleep_until(deadline).await,
                        None => pending::<()>().await,
                    }
                };

                tokio::select! {
                    _ = interval.tick() => {
                        if let Err(error) = runtime.refresh_from(HeadSource::Polling).await {
                            debug!(%error, "background chain tip refresh failed");
                        }
                    }
                    _ = reconnect => {
                        if let (Some(subscriber), Some(_)) = (subscriber.as_ref(), head.ws_url.as_ref()) {
                            state = State::Probing {
                                future: Box::pin(probe_head_source(
                                    subscriber.clone(),
                                    runtime.tip_source(),
                                    head.chain_id,
                                    probe_timeout,
                                )),
                            };
                        } else {
                            state = State::Polling { reconnect_at: None };
                        }
                    }
                }
            }
            State::Probing { mut future } => {
                tokio::select! {
                    _ = interval.tick() => {
                        if let Err(error) = runtime.refresh_from(HeadSource::Polling).await {
                            debug!(%error, "background chain tip refresh failed");
                        }
                        state = State::Probing { future };
                    }
                    outcome = &mut future => {
                        match outcome {
                            Ok(ProbeSuccess {
                                subscription,
                                first_header,
                            }) => {
                                let first_header_received = TokioInstant::now();
                                runtime.set_active_source(HeadSource::WebSocket);
                                refresh_from_header(&runtime, &head, first_header).await;
                                if ever_connected {
                                    runtime.record_ws_reconnect();
                                }
                                ever_connected = true;
                                backoff = Duration::from_secs(1);
                                if let Some(url) = head.ws_url.as_ref() {
                                    info!(source = %HeadSource::WebSocket, url = %redact_ws_url(url), "chain head source switched");
                                }
                                state = State::WebSocket {
                                    headers: subscription.headers,
                                    last_header: first_header_received,
                                };
                            }
                            Err(failure) => {
                                let is_unsupported = matches!(failure, ProbeFailure::Unsupported { .. });
                                let is_mismatch = matches!(
                                    failure,
                                    ProbeFailure::ChainIdMismatch { .. } | ProbeFailure::BlockHashMismatch { .. }
                                );
                                if let Some(url) = head.ws_url.as_ref() {
                                    log_probe_failure(&failure, url, head.ws_url_derived);
                                }

                                if is_mismatch || is_unsupported {
                                    state = State::Polling { reconnect_at: None };
                                } else {
                                    // A connection failure is retried with backoff, including when the
                                    // endpoint has never accepted a subscription. The HTTP side may be
                                    // behind the WebSocket side, so the probe can fail before the first
                                    // header's block is available over HTTP.
                                    let reconnect_at = TokioInstant::now() + backoff;
                                    backoff = next_reconnect_backoff(backoff);
                                    state = State::Polling {
                                        reconnect_at: Some(reconnect_at),
                                    };
                                }
                            }
                        }
                    }
                }
            }
            State::WebSocket {
                headers: stream,
                last_header,
            } => {
                let mut headers = Some(stream);
                let silent_deadline = last_header + probe_timeout;

                tokio::select! {
                    header = next_websocket_header(&mut headers) => {
                        match header {
                            Some(Ok(header)) => {
                                // The silence deadline starts when the last header arrives. Headers
                                // keep being read while a refresh is in flight, so a refresh that
                                // retries for longer than the silence window does not look like a
                                // silent socket. A header that arrives during the refresh moves the
                                // deadline and becomes the next refresh; only the newest one runs.
                                let mut last_header_received = TokioInstant::now();
                                let mut next_refresh = Some(header);

                                enum RefreshLoopOutcome {
                                    Refreshed,
                                    Leave {
                                        reason: String,
                                        timeout_secs: Option<u64>,
                                    },
                                }

                                let next_state = 'refresh: loop {
                                    let Some(header) = next_refresh.take() else {
                                        break 'refresh State::WebSocket {
                                            headers: headers
                                                .take()
                                                .unwrap_or_else(|| futures::stream::empty().boxed()),
                                            last_header: last_header_received,
                                        };
                                    };

                                    let outcome = {
                                        let refresh = refresh_from_header(&runtime, &head, header);
                                        tokio::pin!(refresh);

                                        loop {
                                            let silent_deadline = last_header_received + probe_timeout;

                                            tokio::select! {
                                                _ = &mut refresh => break RefreshLoopOutcome::Refreshed,
                                                _ = sleep_until(silent_deadline) => {
                                                    break RefreshLoopOutcome::Leave {
                                                        reason: "no new head received on the WebSocket while refreshing, switching to polling".to_string(),
                                                        timeout_secs: Some(probe_timeout.as_secs()),
                                                    };
                                                }
                                                header = next_websocket_header(&mut headers) => {
                                                    match header {
                                                        Some(Ok(header)) => {
                                                            last_header_received = TokioInstant::now();
                                                            next_refresh = Some(header);
                                                        }
                                                        Some(Err(error)) => {
                                                            break RefreshLoopOutcome::Leave {
                                                                reason: format!("WebSocket head source failed, switching to polling: {error}"),
                                                                timeout_secs: None,
                                                            };
                                                        }
                                                        None => {
                                                            break RefreshLoopOutcome::Leave {
                                                                reason: "WebSocket head source closed, switching to polling".to_string(),
                                                                timeout_secs: None,
                                                            };
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    };

                                    match outcome {
                                        RefreshLoopOutcome::Refreshed => {}
                                        RefreshLoopOutcome::Leave {
                                            reason,
                                            timeout_secs,
                                        } => {
                                            let reconnect_at = leave_websocket(
                                                &runtime,
                                                &head,
                                                &reason,
                                                timeout_secs,
                                                &mut backoff,
                                            )
                                            .await;
                                            break 'refresh State::Polling {
                                                reconnect_at: Some(reconnect_at),
                                            };
                                        }
                                    }
                                };

                                state = next_state;
                            }
                            Some(Err(error)) => {
                                let reconnect_at = leave_websocket(
                                    &runtime,
                                    &head,
                                    &format!("WebSocket head source failed, switching to polling: {error}"),
                                    None,
                                    &mut backoff,
                                )
                                .await;
                                state = State::Polling { reconnect_at: Some(reconnect_at) };
                            }
                            None => {
                                let reconnect_at = leave_websocket(
                                    &runtime,
                                    &head,
                                    "WebSocket head source closed, switching to polling",
                                    None,
                                    &mut backoff,
                                )
                                .await;
                                state = State::Polling { reconnect_at: Some(reconnect_at) };
                            }
                        }
                    }
                    _ = sleep_until(silent_deadline) => {
                        let reconnect_at = leave_websocket(
                            &runtime,
                            &head,
                            "no new head received on the WebSocket, switching to polling",
                            Some(probe_timeout.as_secs()),
                            &mut backoff,
                        )
                        .await;
                        state = State::Polling { reconnect_at: Some(reconnect_at) };
                    }
                }
            }
        }
    }
}

#[cfg(all(feature = "telemetry", not(test)))]
lazy_static::lazy_static! {
    static ref METRIC_HEAD_WS_RECONNECTS_TOTAL: Option<SimpleCounter> =
        SimpleCounter::new(
            "blokli_head_ws_reconnects_total",
            "Successful WebSocket head source reconnects",
        ).inspect_err(|error| error!(%error, metric = "blokli_head_ws_reconnects_total", "failed to register telemetry metric")).ok();
}

pub(crate) fn record_ws_reconnect_metric() {
    #[cfg(all(feature = "telemetry", not(test)))]
    if let Some(metric) = METRIC_HEAD_WS_RECONNECTS_TOTAL.as_ref() {
        metric.increment();
    }
}

#[cfg(test)]
mod tests {
    use std::{
        collections::VecDeque,
        future::pending,
        sync::{
            Arc, Mutex,
            atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering},
        },
        time::Duration,
    };

    use async_trait::async_trait;
    use futures::StreamExt;
    use hopr_bindings::exports::alloy::primitives::B256;
    use tokio::sync::mpsc;
    use url::Url;

    use super::{
        HeadError, HeadHeader, HeadSourceConfig, HeadSubscriber, HeadSubscription, StdResult, classify_head_error,
        derive_ws_url, next_reconnect_backoff, redact_ws_url, resolve_head_ws_url,
    };
    use crate::{
        errors::{Result, RpcError},
        tip::{ChainTipTracker, ConfirmationConfig, ConfirmationMode, HeadSource, TipObservation, TipSource},
    };

    fn observation(latest: u64, safe: Option<u64>, finalized: Option<u64>) -> TipObservation {
        TipObservation {
            latest,
            latest_timestamp: Some(1_700_000_000 + latest),
            safe,
            finalized,
        }
    }

    fn config(mode: ConfirmationMode) -> ConfirmationConfig {
        ConfirmationConfig {
            mode,
            depth: 3,
            max_safe_gap: 32,
        }
    }

    #[test]
    fn test_derive_ws_url_from_http() {
        let url = Url::parse("http://localhost:8545/rpc?key=secret").expect("URL should parse");
        let derived = derive_ws_url(&url).expect("http URL should derive");
        assert_eq!(derived.as_str(), "ws://localhost:8545/rpc?key=secret");
    }

    #[test]
    fn test_derive_ws_url_from_https_keeps_host_port_path_and_query() {
        let url = Url::parse("https://rpc.example.com:8443/v1/abc?token=xyz").expect("URL should parse");
        let derived = derive_ws_url(&url).expect("https URL should derive");
        assert_eq!(derived.as_str(), "wss://rpc.example.com:8443/v1/abc?token=xyz");
    }

    #[test]
    fn test_derive_ws_url_rejects_other_schemes() {
        for raw in ["ftp://example.com", "ws://example.com"] {
            let url = Url::parse(raw).expect("URL should parse");
            let error = derive_ws_url(&url).expect_err("non-HTTP URL must be rejected");
            assert!(error.to_string().contains("scheme"));
        }
    }

    #[test]
    fn test_resolve_head_ws_url_uses_override_or_derived_url() {
        let rpc_url = Url::parse("https://rpc.example.com/v1?token=abc").expect("URL should parse");
        let override_url = Url::parse("wss://ws.example.com/wss").expect("URL should parse");

        let derived = resolve_head_ws_url(&rpc_url, None, true).expect("URL should resolve");
        assert_eq!(
            derived,
            Some((
                Url::parse("wss://rpc.example.com/v1?token=abc").expect("URL should parse"),
                true
            ))
        );

        let overridden = resolve_head_ws_url(&rpc_url, Some(&override_url), true).expect("URL should resolve");
        assert_eq!(overridden, Some((override_url, false)));

        let disabled = resolve_head_ws_url(
            &rpc_url,
            Some(&Url::parse("wss://ws.example.com").expect("URL should parse")),
            false,
        )
        .expect("URL should resolve");
        assert_eq!(disabled, None);
    }

    #[test]
    fn test_classify_head_error_unsupported_error_responses() {
        for (code, message) in [
            (Some(-32601), "Method not found"),
            (None, "notifications not supported"),
            (None, "subscriptions are not supported by this endpoint"),
            (None, "subscriptions not supported"),
            (None, "subscription not supported"),
            (Some(-32000), "method eth_subscribe does not exist"),
        ] {
            assert!(
                classify_head_error(true, code, message).is_unsupported(),
                "{code:?} {message} should be classified as unsupported"
            );
        }
    }

    #[test]
    fn test_classify_head_error_only_error_responses_can_be_unsupported() {
        for message in [
            "method not found",
            "notifications not supported",
            "subscriptions are not supported",
            "does not exist",
            "is not available",
        ] {
            assert_eq!(
                classify_head_error(false, Some(-32601), message),
                HeadError::connection(message, Some(-32601))
            );
        }
    }

    #[test]
    fn test_classify_head_error_keeps_a_temporary_availability_error_retryable() {
        for message in [
            "service is not available",
            "the subscription is not available",
            "upstream not available",
        ] {
            let error = classify_head_error(true, Some(-32000), message);
            assert_eq!(error, HeadError::connection(message, Some(-32000)));
        }
    }

    #[test]
    fn test_classify_head_error_preserves_the_json_rpc_code() {
        let error = classify_head_error(true, Some(-32601), "Method not found");
        assert_eq!(error.code(), Some(-32601));

        let transport_error = classify_head_error(false, None, "socket closed");
        assert_eq!(transport_error.code(), None);
    }

    #[test]
    fn test_probe_failure_keeps_only_the_error_category_and_code() {
        let failure = super::ProbeFailure::from(HeadError::connection(
            "wss://rpc.example.com/?token=secret is not reachable",
            Some(-32000),
        ));

        assert!(
            !format!("{failure:?}").contains("secret"),
            "the probe failure must not carry the endpoint's raw error text"
        );
        assert_eq!(
            failure,
            super::ProbeFailure::Connection { code: Some(-32000) },
            "the JSON-RPC code must survive for the warning"
        );
    }

    #[test]
    fn test_head_source_config_debug_redacts_the_websocket_url() {
        let config = HeadSourceConfig {
            chain_id: 100,
            poll_interval: Duration::from_secs(1),
            expected_block_time: Duration::from_secs(5),
            ws_url: Some(
                Url::parse("wss://user:pass@rpc.example.com:8443/secret/path?token=xyz").expect("URL should parse"),
            ),
            ws_url_derived: true,
        };

        let debug = format!("{config:?}");
        assert!(debug.contains("wss://rpc.example.com:8443"), "{debug}");
        for secret in ["user", "pass", "secret", "xyz"] {
            assert!(!debug.contains(secret), "{debug}");
        }
    }

    #[test]
    fn test_classify_head_error_connection_errors() {
        for (code, message) in [
            (Some(-32000), "connection reset"),
            (None, "websocket closed"),
            (None, "handshake failed"),
            (Some(-32603), "internal error"),
            (None, "unsupported protocol version"),
            (None, "tls: unsupported certificate"),
            (None, "failed to send eth_subscribe: connection reset"),
        ] {
            let error = classify_head_error(true, code, message);
            assert_eq!(error, HeadError::connection(message, code));
        }
    }

    #[test]
    fn test_redact_ws_url_hides_userinfo_path_and_query() {
        let url = Url::parse("wss://user:pass@rpc.example.com:8443/secret/path?token=xyz").expect("URL should parse");
        assert_eq!(redact_ws_url(&url), "wss://rpc.example.com:8443");
    }

    #[test]
    fn test_next_reconnect_backoff_doubles_and_caps_at_one_minute() {
        assert_eq!(next_reconnect_backoff(Duration::from_secs(1)), Duration::from_secs(2));
        assert_eq!(next_reconnect_backoff(Duration::from_secs(30)), Duration::from_secs(60));
        assert_eq!(next_reconnect_backoff(Duration::from_secs(60)), Duration::from_secs(60));
        assert_eq!(
            next_reconnect_backoff(Duration::from_secs(600)),
            Duration::from_secs(60)
        );
    }

    #[test]
    fn test_head_header_uses_header_hash() {
        let header = super::HeadHeader {
            number: 42,
            hash: B256::repeat_byte(0x11),
        };
        assert_eq!(header.number, 42);
        assert_eq!(header.hash, B256::repeat_byte(0x11));
    }

    #[derive(Clone)]
    enum MockSubscription {
        Headers {
            chain_id: u64,
            headers: Vec<HeadHeader>,
            keep_open: bool,
        },
        RepeatHeader {
            chain_id: u64,
            header: HeadHeader,
            interval: Duration,
        },
        Pending {
            chain_id: u64,
        },
        Error(HeadError),
    }

    #[derive(Clone, Default)]
    struct MockHeadSubscriber {
        scripts: Arc<Mutex<VecDeque<MockSubscription>>>,
        calls: Arc<AtomicUsize>,
    }

    impl MockHeadSubscriber {
        fn new(scripts: Vec<MockSubscription>) -> Self {
            Self {
                scripts: Arc::new(Mutex::new(scripts.into())),
                calls: Arc::new(AtomicUsize::new(0)),
            }
        }

        fn calls(&self) -> usize {
            self.calls.load(Ordering::SeqCst)
        }
    }

    #[async_trait]
    impl HeadSubscriber for MockHeadSubscriber {
        async fn subscribe(&self) -> StdResult<HeadSubscription, HeadError> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            let script = self
                .scripts
                .lock()
                .expect("scripts lock should not be poisoned")
                .pop_front()
                .unwrap_or_else(|| MockSubscription::Error(HeadError::connection("no subscription script left", None)));

            match script {
                MockSubscription::Error(error) => Err(error),
                MockSubscription::Pending { chain_id } => Ok(HeadSubscription {
                    chain_id,
                    headers: futures::stream::pending().boxed(),
                }),
                MockSubscription::Headers {
                    chain_id,
                    headers,
                    keep_open,
                } => {
                    let mut stream = futures::stream::iter(headers.into_iter().map(Ok)).boxed();
                    if keep_open {
                        stream = stream.chain(futures::stream::pending()).boxed();
                    }
                    Ok(HeadSubscription {
                        chain_id,
                        headers: stream,
                    })
                }
                MockSubscription::RepeatHeader {
                    chain_id,
                    header,
                    interval,
                } => {
                    let headers = async_stream::stream! {
                        loop {
                            yield Ok(header);
                            tokio::time::sleep(interval).await;
                        }
                    }
                    .boxed();
                    Ok(HeadSubscription { chain_id, headers })
                }
            }
        }
    }

    #[derive(Clone)]
    struct DriverTipSource {
        calls: Arc<AtomicUsize>,
        latest: Arc<AtomicU64>,
        hash: B256,
    }

    impl DriverTipSource {
        fn new(hash: B256) -> Self {
            Self {
                calls: Arc::new(AtomicUsize::new(0)),
                latest: Arc::new(AtomicU64::new(100)),
                hash,
            }
        }

        fn calls(&self) -> usize {
            self.calls.load(Ordering::SeqCst)
        }
    }

    #[async_trait]
    impl TipSource for DriverTipSource {
        async fn observe(&self) -> Result<TipObservation> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            let latest = self.latest.fetch_add(1, Ordering::SeqCst) + 1;
            Ok(observation(latest, None, None))
        }

        async fn block_hash(&self, _block: u64) -> Result<B256> {
            Ok(self.hash)
        }
    }

    #[derive(Clone)]
    struct FailToggleTipSource {
        calls: Arc<AtomicUsize>,
        latest: Arc<AtomicU64>,
        fail: Arc<AtomicBool>,
        hash: B256,
    }

    impl FailToggleTipSource {
        fn new(hash: B256) -> Self {
            Self {
                calls: Arc::new(AtomicUsize::new(0)),
                latest: Arc::new(AtomicU64::new(100)),
                fail: Arc::new(AtomicBool::new(false)),
                hash,
            }
        }

        fn calls(&self) -> usize {
            self.calls.load(Ordering::SeqCst)
        }

        fn set_fail(&self, fail: bool) {
            self.fail.store(fail, Ordering::SeqCst);
        }
    }

    #[async_trait]
    impl TipSource for FailToggleTipSource {
        async fn observe(&self) -> Result<TipObservation> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            if self.fail.load(Ordering::SeqCst) {
                return Err(RpcError::Other("rpc failed".to_string()));
            }
            let latest = self.latest.fetch_add(1, Ordering::SeqCst) + 1;
            Ok(observation(latest, None, None))
        }

        async fn block_hash(&self, _block: u64) -> Result<B256> {
            Ok(self.hash)
        }
    }

    /// Fails the first block hash lookup, as an HTTP backend that has not received the block yet.
    #[derive(Clone)]
    struct FailOnceHashTipSource {
        hash: B256,
        hash_calls: Arc<AtomicUsize>,
        latest: Arc<AtomicU64>,
    }

    impl FailOnceHashTipSource {
        fn new(hash: B256) -> Self {
            Self {
                hash,
                hash_calls: Arc::new(AtomicUsize::new(0)),
                latest: Arc::new(AtomicU64::new(100)),
            }
        }

        fn hash_calls(&self) -> usize {
            self.hash_calls.load(Ordering::SeqCst)
        }
    }

    #[async_trait]
    impl TipSource for FailOnceHashTipSource {
        async fn observe(&self) -> Result<TipObservation> {
            let latest = self.latest.fetch_add(1, Ordering::SeqCst) + 1;
            Ok(observation(latest, None, None))
        }

        async fn block_hash(&self, _block: u64) -> Result<B256> {
            if self.hash_calls.fetch_add(1, Ordering::SeqCst) == 0 {
                return Err(RpcError::Other("block 50 is not available".to_string()));
            }
            Ok(self.hash)
        }
    }

    fn head_config(ws_url: Option<&str>, chain_id: u64) -> HeadSourceConfig {
        HeadSourceConfig {
            chain_id,
            poll_interval: Duration::from_secs(1),
            expected_block_time: Duration::from_secs(1),
            ws_url: ws_url.map(|url| Url::parse(url).expect("test URL should parse")),
            ws_url_derived: false,
        }
    }

    fn driver_tracker<S>(source: DriverTipSource, subscriber: Option<S>, head: HeadSourceConfig) -> ChainTipTracker
    where
        S: HeadSubscriber + 'static,
    {
        ChainTipTracker::with_head_source(
            Arc::new(source),
            config(ConfirmationMode::Depth),
            head,
            subscriber.map(|subscriber| Arc::new(subscriber) as Arc<dyn HeadSubscriber>),
        )
    }

    fn header(number: u64, byte: u8) -> HeadHeader {
        HeadHeader {
            number,
            hash: B256::repeat_byte(byte),
        }
    }

    async fn advance(duration: Duration) {
        let step = Duration::from_millis(100);
        let mut remaining = duration;
        while remaining > step {
            tokio::time::advance(step).await;
            tokio::task::yield_now().await;
            remaining -= step;
        }
        if !remaining.is_zero() {
            tokio::time::advance(remaining).await;
            tokio::task::yield_now().await;
        }
    }

    #[tokio::test(start_paused = true)]
    async fn test_head_driver_without_websocket_never_calls_subscriber() {
        let subscriber = MockHeadSubscriber::new(vec![MockSubscription::Pending { chain_id: 100 }]);
        let source = DriverTipSource::new(B256::ZERO);
        let tracker = driver_tracker(source, Some(subscriber.clone()), head_config(None, 100));

        tracker.ensure_started();
        advance(Duration::from_secs(5)).await;

        assert_eq!(subscriber.calls(), 0);
        assert_eq!(tracker.current().map(|tip| tip.source), Some(HeadSource::Polling));
    }

    #[tokio::test(start_paused = true)]
    async fn test_head_driver_keeps_retrying_after_a_connection_failure() {
        let subscriber = MockHeadSubscriber::new(vec![MockSubscription::Error(HeadError::connection(
            "handshake failed",
            None,
        ))]);
        let source = DriverTipSource::new(B256::ZERO);
        let tracker = driver_tracker(
            source.clone(),
            Some(subscriber.clone()),
            head_config(Some("ws://localhost:8546"), 100),
        );

        tracker.ensure_started();
        advance(Duration::from_secs(5)).await;

        assert!(subscriber.calls() >= 2, "a connection failure must be retried");
        assert!(source.calls() > 0, "polling must keep publishing tips");
        assert_eq!(tracker.current().map(|tip| tip.source), Some(HeadSource::Polling));

        let calls = subscriber.calls();
        advance(Duration::from_secs(30)).await;
        assert!(subscriber.calls() > calls, "the retries must continue with backoff");
    }

    #[tokio::test(start_paused = true)]
    async fn test_head_driver_retries_the_probe_after_a_connection_failure_before_ever_connecting() {
        let subscriber = MockHeadSubscriber::new(vec![
            MockSubscription::Error(HeadError::connection("handshake failed", None)),
            MockSubscription::Headers {
                chain_id: 100,
                headers: vec![header(50, 0x11)],
                keep_open: true,
            },
        ]);
        let source = DriverTipSource::new(B256::repeat_byte(0x11));
        let tracker = driver_tracker(
            source,
            Some(subscriber.clone()),
            head_config(Some("ws://localhost:8546"), 100),
        );

        tracker.ensure_started();
        advance(Duration::from_millis(1_500)).await;

        assert_eq!(subscriber.calls(), 2, "the startup probe must be retried");
        assert_eq!(tracker.current().map(|tip| tip.source), Some(HeadSource::WebSocket));
    }

    #[tokio::test(start_paused = true)]
    async fn test_head_driver_retries_the_probe_when_the_block_hash_is_not_available_yet() {
        let subscriber = MockHeadSubscriber::new(vec![
            MockSubscription::Headers {
                chain_id: 100,
                headers: vec![header(50, 0x11)],
                keep_open: true,
            },
            MockSubscription::Headers {
                chain_id: 100,
                headers: vec![header(51, 0x11)],
                keep_open: true,
            },
        ]);
        let source = FailOnceHashTipSource::new(B256::repeat_byte(0x11));
        let tracker = ChainTipTracker::with_head_source(
            Arc::new(source.clone()),
            config(ConfirmationMode::Depth),
            head_config(Some("ws://localhost:8546"), 100),
            Some(Arc::new(subscriber)),
        );

        tracker.ensure_started();
        advance(Duration::from_millis(1_500)).await;

        assert_eq!(source.hash_calls(), 2, "the hash lookup must be retried");
        assert_eq!(tracker.current().map(|tip| tip.source), Some(HeadSource::WebSocket));
    }

    #[tokio::test(start_paused = true)]
    async fn test_head_driver_disables_websocket_when_subscription_is_unsupported() {
        let subscriber = MockHeadSubscriber::new(vec![MockSubscription::Error(HeadError::unsupported(
            "method not found",
            Some(-32601),
        ))]);
        let source = DriverTipSource::new(B256::ZERO);
        let tracker = driver_tracker(
            source.clone(),
            Some(subscriber.clone()),
            head_config(Some("ws://localhost:8546"), 100),
        );

        tracker.ensure_started();
        advance(Duration::from_secs(5)).await;

        assert_eq!(subscriber.calls(), 1);
        assert!(source.calls() > 0);
        assert_eq!(tracker.current().map(|tip| tip.source), Some(HeadSource::Polling));

        advance(Duration::from_secs(30)).await;
        assert_eq!(subscriber.calls(), 1);
    }

    #[tokio::test(start_paused = true)]
    async fn test_head_driver_retries_the_probe_when_no_header_arrives() {
        let subscriber = MockHeadSubscriber::new(vec![MockSubscription::Pending { chain_id: 100 }]);
        let source = DriverTipSource::new(B256::ZERO);
        let tracker = driver_tracker(
            source.clone(),
            Some(subscriber.clone()),
            head_config(Some("ws://localhost:8546"), 100),
        );

        tracker.ensure_started();
        advance(Duration::from_secs(6)).await;

        assert!(subscriber.calls() >= 2, "a probe timeout must be retried");
        assert!(source.calls() > 1, "polling must run while the probe waits");
        assert_eq!(tracker.current().map(|tip| tip.source), Some(HeadSource::Polling));
    }

    #[tokio::test(start_paused = true)]
    async fn test_head_driver_disables_websocket_on_chain_id_mismatch() {
        let subscriber = MockHeadSubscriber::new(vec![MockSubscription::Headers {
            chain_id: 1,
            headers: vec![header(50, 0x11)],
            keep_open: true,
        }]);
        let source = DriverTipSource::new(B256::repeat_byte(0x11));
        let tracker = driver_tracker(
            source.clone(),
            Some(subscriber.clone()),
            head_config(Some("ws://localhost:8546"), 100),
        );

        tracker.ensure_started();
        advance(Duration::from_secs(5)).await;

        assert_eq!(subscriber.calls(), 1);
        assert_eq!(tracker.current().map(|tip| tip.source), Some(HeadSource::Polling));
        assert!(source.calls() > 0);

        advance(Duration::from_secs(30)).await;
        assert_eq!(subscriber.calls(), 1);
    }

    #[tokio::test(start_paused = true)]
    async fn test_head_driver_disables_websocket_on_block_hash_mismatch() {
        let subscriber = MockHeadSubscriber::new(vec![MockSubscription::Headers {
            chain_id: 100,
            headers: vec![header(50, 0x11)],
            keep_open: true,
        }]);
        let source = DriverTipSource::new(B256::repeat_byte(0x22));
        let tracker = driver_tracker(
            source.clone(),
            Some(subscriber.clone()),
            head_config(Some("ws://localhost:8546"), 100),
        );

        tracker.ensure_started();
        advance(Duration::from_secs(5)).await;

        assert_eq!(subscriber.calls(), 1);
        assert_eq!(tracker.current().map(|tip| tip.source), Some(HeadSource::Polling));
        assert!(source.calls() > 0);

        advance(Duration::from_secs(30)).await;
        assert_eq!(subscriber.calls(), 1);
    }

    #[tokio::test(start_paused = true)]
    async fn test_head_driver_uses_websocket_and_refreshes_on_each_header() {
        let subscriber = MockHeadSubscriber::new(vec![MockSubscription::Headers {
            chain_id: 100,
            headers: vec![header(50, 0x11), header(51, 0x12)],
            keep_open: true,
        }]);
        let source = DriverTipSource::new(B256::repeat_byte(0x11));
        let tracker = driver_tracker(
            source.clone(),
            Some(subscriber.clone()),
            head_config(Some("ws://localhost:8546"), 100),
        );

        tracker.ensure_started();
        advance(Duration::from_millis(10)).await;

        assert_eq!(subscriber.calls(), 1);
        assert_eq!(tracker.current().map(|tip| tip.source), Some(HeadSource::WebSocket));
        assert!(
            source.calls() >= 2,
            "the first header and the following header must trigger HTTP refreshes without waiting for the poll \
             interval"
        );
    }

    #[tokio::test(start_paused = true)]
    async fn test_head_driver_refreshes_for_a_second_header_without_a_poll_tick() {
        let (tx, rx) = mpsc::unbounded_channel::<StdResult<HeadHeader, HeadError>>();
        tx.send(Ok(header(50, 0x11)))
            .expect("the probe header should be queued");
        let subscriber = ChannelHeadSubscriber {
            chain_id: 100,
            receiver: Arc::new(Mutex::new(Some(rx))),
        };
        let source = DriverTipSource::new(B256::repeat_byte(0x11));
        let tracker = driver_tracker(
            source.clone(),
            Some(subscriber),
            head_config(Some("ws://localhost:8546"), 100),
        );

        tracker.ensure_started();
        advance(Duration::from_millis(10)).await;
        assert_eq!(tracker.current().map(|tip| tip.source), Some(HeadSource::WebSocket));

        let calls_after_activation = source.calls();
        tx.send(Ok(header(51, 0x12)))
            .expect("the second header should be delivered");
        advance(Duration::from_millis(10)).await;

        assert_eq!(
            source.calls(),
            calls_after_activation + 1,
            "the second header must trigger its own refresh before the next polling tick"
        );
    }

    #[tokio::test(start_paused = true)]
    async fn test_manual_refresh_keeps_the_active_websocket_source() {
        let subscriber = MockHeadSubscriber::new(vec![MockSubscription::Headers {
            chain_id: 100,
            headers: vec![header(50, 0x11)],
            keep_open: true,
        }]);
        let source = DriverTipSource::new(B256::repeat_byte(0x11));
        let tracker = driver_tracker(source, Some(subscriber), head_config(Some("ws://localhost:8546"), 100));

        tracker.ensure_started();
        advance(Duration::from_millis(10)).await;
        assert_eq!(tracker.current().map(|tip| tip.source), Some(HeadSource::WebSocket));

        let tip = tracker.refresh().await.expect("manual refresh should succeed");
        assert_eq!(tip.source, HeadSource::WebSocket);
    }

    #[tokio::test(start_paused = true)]
    async fn test_head_driver_retries_once_when_websocket_head_is_below_the_header() {
        let subscriber = MockHeadSubscriber::new(vec![MockSubscription::Headers {
            chain_id: 100,
            headers: vec![header(50, 0x11), header(1_000, 0x12)],
            keep_open: true,
        }]);
        let source = DriverTipSource::new(B256::repeat_byte(0x11));
        let tracker = driver_tracker(
            source.clone(),
            Some(subscriber),
            head_config(Some("ws://localhost:8546"), 100),
        );

        tracker.ensure_started();
        advance(Duration::from_millis(10)).await;
        assert_eq!(tracker.current().map(|tip| tip.source), Some(HeadSource::WebSocket));

        let calls_after_header = source.calls();
        advance(Duration::from_secs(1)).await;
        assert_eq!(
            source.calls(),
            calls_after_header + 1,
            "a header above the HTTP head must trigger one follow-up refresh"
        );

        advance(Duration::from_millis(500)).await;
        assert_eq!(
            source.calls(),
            calls_after_header + 1,
            "the follow-up refresh must not start a loop"
        );
    }

    #[tokio::test(start_paused = true)]
    async fn test_head_driver_retries_once_when_websocket_refresh_fails() {
        let (tx, rx) = mpsc::unbounded_channel::<StdResult<HeadHeader, HeadError>>();
        tx.send(Ok(header(50, 0x11)))
            .expect("header should be queued for the probe");
        let subscriber = ChannelHeadSubscriber {
            chain_id: 100,
            receiver: Arc::new(Mutex::new(Some(rx))),
        };
        let source = FailToggleTipSource::new(B256::repeat_byte(0x11));
        let tracker = ChainTipTracker::with_head_source(
            Arc::new(source.clone()),
            config(ConfirmationMode::Depth),
            head_config(Some("ws://localhost:8546"), 100),
            Some(Arc::new(subscriber)),
        );

        tracker.ensure_started();
        advance(Duration::from_millis(10)).await;
        assert_eq!(tracker.current().map(|tip| tip.source), Some(HeadSource::WebSocket));

        source.set_fail(true);
        tx.send(Ok(header(51, 0x11)))
            .expect("header should trigger a failing refresh");
        advance(Duration::from_millis(10)).await;

        let calls_after_header = source.calls();
        advance(Duration::from_secs(1)).await;
        assert_eq!(
            source.calls(),
            calls_after_header + 1,
            "a failed header refresh must trigger one follow-up refresh"
        );

        advance(Duration::from_millis(500)).await;
        assert_eq!(
            source.calls(),
            calls_after_header + 1,
            "the follow-up refresh must not start a loop"
        );
    }

    #[tokio::test(start_paused = true)]
    async fn test_head_driver_reconnects_after_socket_drop() {
        let subscriber = MockHeadSubscriber::new(vec![
            MockSubscription::Headers {
                chain_id: 100,
                headers: vec![header(50, 0x11)],
                keep_open: false,
            },
            MockSubscription::RepeatHeader {
                chain_id: 100,
                header: header(51, 0x11),
                interval: Duration::from_millis(500),
            },
        ]);
        let source = DriverTipSource::new(B256::repeat_byte(0x11));
        let tracker = driver_tracker(
            source,
            Some(subscriber.clone()),
            head_config(Some("ws://localhost:8546"), 100),
        );

        tracker.ensure_started();
        advance(Duration::from_millis(10)).await;
        assert_eq!(subscriber.calls(), 1);
        assert_eq!(tracker.current().map(|tip| tip.source), Some(HeadSource::Polling));

        advance(Duration::from_secs(5)).await;

        assert_eq!(subscriber.calls(), 2);
        assert_eq!(tracker.ws_reconnect_count(), 1);
        assert_eq!(tracker.current().map(|tip| tip.source), Some(HeadSource::WebSocket));
    }

    #[tokio::test(start_paused = true)]
    async fn test_head_driver_falls_back_to_polling_on_silent_socket() {
        let subscriber = MockHeadSubscriber::new(vec![MockSubscription::Headers {
            chain_id: 100,
            headers: vec![header(50, 0x11)],
            keep_open: true,
        }]);
        let source = DriverTipSource::new(B256::repeat_byte(0x11));
        let tracker = driver_tracker(
            source,
            Some(subscriber.clone()),
            head_config(Some("ws://localhost:8546"), 100),
        );

        tracker.ensure_started();
        advance(Duration::from_millis(10)).await;
        assert_eq!(tracker.current().map(|tip| tip.source), Some(HeadSource::WebSocket));

        advance(Duration::from_secs(3)).await;

        assert_eq!(tracker.current().map(|tip| tip.source), Some(HeadSource::Polling));
    }

    #[tokio::test(start_paused = true)]
    async fn test_head_driver_silence_timer_keeps_running_during_a_long_refresh() {
        let (tx, rx) = mpsc::unbounded_channel::<StdResult<HeadHeader, HeadError>>();
        tx.send(Ok(header(50, 0x11)))
            .expect("the probe header should be queued");
        let subscriber = ChannelHeadSubscriber {
            chain_id: 100,
            receiver: Arc::new(Mutex::new(Some(rx))),
        };
        let source = DriverTipSource::new(B256::repeat_byte(0x11));
        let tracker = ChainTipTracker::with_head_source(
            Arc::new(source),
            config(ConfirmationMode::Depth),
            HeadSourceConfig {
                chain_id: 100,
                poll_interval: Duration::from_secs(30),
                expected_block_time: Duration::from_secs(1),
                ws_url: Some(Url::parse("ws://localhost:8546").expect("test URL should parse")),
                ws_url_derived: false,
            },
            Some(Arc::new(subscriber)),
        );

        tracker.ensure_started();
        advance(Duration::from_millis(10)).await;
        assert_eq!(tracker.current().map(|tip| tip.source), Some(HeadSource::WebSocket));

        // A header far above the HTTP head makes the refresh sleep for the poll interval. The
        // silence timer must still expire within the expected block time window.
        tx.send(Ok(header(1_000_000, 0x12)))
            .expect("the high header should be delivered");
        advance(Duration::from_secs(4)).await;

        assert_eq!(
            tracker.current().map(|tip| tip.source),
            Some(HeadSource::Polling),
            "the silent socket must fall back while the refresh is still in flight"
        );
    }

    #[tokio::test(start_paused = true)]
    async fn test_head_driver_stays_on_websocket_while_headers_arrive_during_a_long_refresh() {
        let subscriber = MockHeadSubscriber::new(vec![MockSubscription::RepeatHeader {
            chain_id: 100,
            header: header(1_000_000, 0x11),
            interval: Duration::from_secs(1),
        }]);
        let source = DriverTipSource::new(B256::repeat_byte(0x11));
        let tracker = ChainTipTracker::with_head_source(
            Arc::new(source),
            config(ConfirmationMode::Depth),
            HeadSourceConfig {
                chain_id: 100,
                poll_interval: Duration::from_secs(30),
                expected_block_time: Duration::from_secs(1),
                ws_url: Some(Url::parse("ws://localhost:8546").expect("test URL should parse")),
                ws_url_derived: false,
            },
            Some(Arc::new(subscriber)),
        );

        tracker.ensure_started();
        advance(Duration::from_secs(6)).await;

        assert_eq!(
            tracker.current().map(|tip| tip.source),
            Some(HeadSource::WebSocket),
            "a socket that keeps delivering headers must not look silent during a long refresh"
        );
    }

    #[tokio::test(start_paused = true)]
    async fn test_head_driver_stops_reconnecting_when_subscription_becomes_unsupported() {
        let subscriber = MockHeadSubscriber::new(vec![
            MockSubscription::Headers {
                chain_id: 100,
                headers: vec![header(50, 0x11)],
                keep_open: false,
            },
            MockSubscription::Error(HeadError::unsupported("method not found", Some(-32601))),
        ]);
        let source = DriverTipSource::new(B256::repeat_byte(0x11));
        let tracker = driver_tracker(
            source,
            Some(subscriber.clone()),
            head_config(Some("ws://localhost:8546"), 100),
        );

        tracker.ensure_started();
        advance(Duration::from_millis(10)).await;
        assert_eq!(subscriber.calls(), 1);

        advance(Duration::from_secs(2)).await;
        assert_eq!(subscriber.calls(), 2);
        assert_eq!(tracker.current().map(|tip| tip.source), Some(HeadSource::Polling));

        advance(Duration::from_secs(120)).await;
        assert_eq!(subscriber.calls(), 2, "unsupported reconnects must not be retried");
    }

    #[tokio::test(start_paused = true)]
    async fn test_head_driver_does_not_publish_header_errors_as_tips() {
        let (tx, rx) = mpsc::unbounded_channel::<StdResult<HeadHeader, HeadError>>();
        tx.send(Ok(header(50, 0x11)))
            .expect("header should be queued for the probe");
        let subscriber = ChannelHeadSubscriber {
            chain_id: 100,
            receiver: Arc::new(Mutex::new(Some(rx))),
        };
        let source = DriverTipSource::new(B256::repeat_byte(0x11));
        let tracker = driver_tracker(source, Some(subscriber), head_config(Some("ws://localhost:8546"), 100));

        tracker.ensure_started();
        advance(Duration::from_millis(10)).await;
        assert_eq!(tracker.current().map(|tip| tip.source), Some(HeadSource::WebSocket));

        tx.send(Err(HeadError::connection("socket closed", None)))
            .expect("header error should be delivered");
        advance(Duration::from_millis(10)).await;

        assert_eq!(tracker.current().map(|tip| tip.source), Some(HeadSource::Polling));
    }

    type HeaderReceiver = mpsc::UnboundedReceiver<StdResult<HeadHeader, HeadError>>;

    #[derive(Clone)]
    struct ChannelHeadSubscriber {
        chain_id: u64,
        receiver: Arc<Mutex<Option<HeaderReceiver>>>,
    }

    #[async_trait]
    impl HeadSubscriber for ChannelHeadSubscriber {
        async fn subscribe(&self) -> StdResult<HeadSubscription, HeadError> {
            let receiver = self
                .receiver
                .lock()
                .expect("receiver lock should not be poisoned")
                .take()
                .ok_or_else(|| HeadError::connection("subscription already opened", None))?;
            let headers = futures::stream::unfold(receiver, |mut receiver| async move {
                receiver.recv().await.map(|item| (item, receiver))
            })
            .boxed();
            Ok(HeadSubscription {
                chain_id: self.chain_id,
                headers,
            })
        }
    }

    struct PendingObserveTipSource {
        calls: Arc<AtomicUsize>,
        hash: B256,
        block_from_call: Arc<AtomicUsize>,
    }

    impl PendingObserveTipSource {
        fn new(hash: B256, block_from_call: Arc<AtomicUsize>) -> Self {
            Self {
                calls: Arc::new(AtomicUsize::new(0)),
                hash,
                block_from_call,
            }
        }

        fn calls(&self) -> usize {
            self.calls.load(Ordering::SeqCst)
        }
    }

    #[async_trait]
    impl TipSource for PendingObserveTipSource {
        async fn observe(&self) -> Result<TipObservation> {
            let call_number = self.calls.fetch_add(1, Ordering::SeqCst) + 1;
            let block_from_call = self.block_from_call.load(Ordering::SeqCst);
            if call_number == block_from_call {
                pending::<()>().await;
            }
            Ok(observation(100 + call_number as u64, None, None))
        }

        async fn block_hash(&self, _block: u64) -> Result<B256> {
            Ok(self.hash)
        }
    }

    #[tokio::test(start_paused = true)]
    async fn test_head_driver_falls_back_while_the_observation_is_pending() {
        let (tx, rx) = mpsc::unbounded_channel::<StdResult<HeadHeader, HeadError>>();
        tx.send(Ok(header(50, 0x11)))
            .expect("the probe header should be queued");
        let subscriber = ChannelHeadSubscriber {
            chain_id: 100,
            receiver: Arc::new(Mutex::new(Some(rx))),
        };

        let block_from_call = Arc::new(AtomicUsize::new(usize::MAX));
        let source = Arc::new(PendingObserveTipSource::new(
            B256::repeat_byte(0x11),
            block_from_call.clone(),
        ));
        let tracker = ChainTipTracker::with_head_source(
            source.clone(),
            config(ConfirmationMode::Depth),
            HeadSourceConfig {
                chain_id: 100,
                poll_interval: Duration::from_secs(30),
                expected_block_time: Duration::from_secs(1),
                ws_url: Some(Url::parse("ws://localhost:8546").expect("test URL should parse")),
                ws_url_derived: false,
            },
            Some(Arc::new(subscriber)),
        );

        tracker.ensure_started();
        advance(Duration::from_millis(10)).await;
        assert_eq!(tracker.current().map(|tip| tip.source), Some(HeadSource::WebSocket));

        let n = source.calls();
        block_from_call.store(n + 1, Ordering::SeqCst);

        tx.send(Ok(header(101, 0x12)))
            .expect("the next header should be delivered");
        advance(Duration::from_secs(4)).await;

        assert_eq!(
            tracker.current().map(|tip| tip.source),
            Some(HeadSource::Polling),
            "the fallback must complete while the observation is pending"
        );
    }
}
