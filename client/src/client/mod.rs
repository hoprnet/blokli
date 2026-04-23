mod queries;
mod subscriptions;
#[cfg(feature = "testing")]
mod testing;
mod transactions;

use std::{fmt::Debug, future::Future, time::Duration};

use cynic::GraphQlResponse;
use eventsource_client::{Client, ReconnectOptionsBuilder, SSE};
use futures::{StreamExt, TryFutureExt, TryStreamExt};
use launchdarkly_sdk_transport::{ByteStream, HttpTransport, ResponseFuture, TransportError};
use reqwest::redirect::Policy as RedirectPolicy;
#[cfg(feature = "testing")]
pub use testing::{
    BlokliTestClient, BlokliTestState, BlokliTestStateMutator, BlokliTestStateSnapshot, NopStateMutator,
};

use crate::{
    CLIENT_VERSION,
    api::{BlokliQueryClient, VERSION},
    errors::{BlokliClientError, ErrorKind},
};

const MIN_RECONNECTION_DELAY: Duration = Duration::from_millis(1);

/// Configuration for the [`BlokliClient`].
#[derive(Clone, Debug, PartialEq, Eq, smart_default::SmartDefault)]
pub struct BlokliClientConfig {
    /// General timeout for non-streaming requests and SSE connection establishment.
    #[default(Duration::from_secs(10))]
    pub timeout: Duration,
    /// Reconnection timeout for SSE streams.
    #[default(Duration::from_secs(30))]
    pub stream_reconnect_timeout: Duration,
    /// Per-read timeout for SSE streams. If `None`, established streams may stay open indefinitely.
    #[default(Some(Duration::from_secs(60)))]
    pub subscription_read_timeout: Option<Duration>,
    /// TCP keepalive interval for SSE streams.
    #[default(Duration::from_secs(15))]
    pub subscription_tcp_keepalive: Duration,
    /// Delay before recreating a completed SSE stream. If `None`, completed streams will not be recreated.
    #[default(Some(Duration::from_secs(1)))]
    pub subscription_stream_restart_delay: Option<Duration>,
}

/// Internal state for managing GraphQL subscription streams.
struct SubscriptionStreamState {
    graphql_url: url::Url,
    query: String,
    cfg: BlokliClientConfig,
    reqwest_client: reqwest::Client,
    stream: Option<eventsource_client::BoxStream<eventsource_client::Result<SSE>>>,
}

impl SubscriptionStreamState {
    fn new(
        graphql_url: url::Url,
        query: String,
        config: BlokliClientConfig,
        reqwest_client: reqwest::Client,
    ) -> Result<Self, BlokliClientError> {
        let mut instance = Self {
            graphql_url,
            query,
            cfg: config,
            reqwest_client,
            stream: None,
        };

        instance.start_stream()?;

        Ok(instance)
    }

    fn start_stream(&mut self) -> Result<(), BlokliClientError> {
        let sse_err = |e| ErrorKind::Subscription(Box::new(e));
        let initial_reconnect_delay = self
            .cfg
            .stream_reconnect_timeout
            .min(Duration::from_secs(2))
            .max(MIN_RECONNECTION_DELAY);
        let client = eventsource_client::ClientBuilder::for_url(self.graphql_url.as_str())
            .map_err(sse_err)?
            .header("Accept", "text/event-stream")
            .map_err(sse_err)?
            .header("Content-Type", "application/json")
            .map_err(sse_err)?
            .method("POST".into())
            .body(self.query.clone())
            .redirect_limit(REDIRECT_LIMIT as u32)
            .reconnect(
                ReconnectOptionsBuilder::new(true)
                    .retry_initial(true)
                    .delay(initial_reconnect_delay)
                    .backoff_factor(2)
                    .delay_max(self.cfg.stream_reconnect_timeout)
                    .build(),
            )
            .build_with_transport(ReqwestTransport::new(self.reqwest_client.clone()));

        self.stream = Some(client.stream());
        Ok(())
    }
}

#[derive(Clone, Debug)]
pub struct ReqwestTransport {
    client: reqwest::Client,
}

impl ReqwestTransport {
    pub fn new(client: reqwest::Client) -> Self {
        Self { client }
    }
}

impl HttpTransport for ReqwestTransport {
    fn request(&self, request: http::Request<Option<bytes::Bytes>>) -> ResponseFuture {
        let client = self.client.clone();
        Box::pin(async move {
            let (parts, body) = request.into_parts();
            let request = http::Request::from_parts(parts, body.map(reqwest::Body::from).unwrap_or_default());
            let request = reqwest::Request::try_from(request).map_err(TransportError::new)?;
            let response = client.execute(request).await.map_err(TransportError::new)?;

            let status = response.status();
            let version = response.version();
            let headers = response.headers().clone();
            let body: ByteStream = Box::pin(response.bytes_stream().map_err(TransportError::new));

            let mut response_builder = http::Response::builder().status(status).version(version);
            if let Some(response_headers) = response_builder.headers_mut() {
                *response_headers = headers;
            }

            response_builder.body(body).map_err(TransportError::new)
        })
    }
}

/// Client implementation of the Blokli API.
///
/// The client implements the following Blokli API traits:
/// - [`BlokliQueryClient`](api::BlokliQueryClient)
/// - [`BlokliSubscriptionClient`](api::BlokliSubscriptionClient).
/// - [`BlokliTransactionClient`](api::BlokliTransactionClient)
#[derive(Clone, Debug)]
pub struct BlokliClient {
    base_url: url::Url,
    cfg: BlokliClientConfig,
}

const REDIRECT_LIMIT: usize = 3;

/// Contains all GraphQL queries used by the Blokli client.
pub struct GraphQlQueries;

impl BlokliClient {
    /// Creates a new instance given Blokli base URL and configuration.
    pub fn new(base_url: url::Url, cfg: BlokliClientConfig) -> Self {
        Self { base_url, cfg }
    }

    /// Returns the client's base Blokli URL.
    pub fn base_url(&self) -> &url::Url {
        &self.base_url
    }

    /// Returns the client's configuration.
    pub fn config(&self) -> &BlokliClientConfig {
        &self.cfg
    }

    pub async fn check_compatibility(&self) -> Result<(), BlokliClientError> {
        let compatibility = self.query_compatibility().await?;

        match crate::compatibility::validate_client_compatibility(&compatibility) {
            Ok(()) => Ok(()),
            Err(_) => Err(ErrorKind::VersionMismatch {
                client_version: CLIENT_VERSION.to_string(),
                supported_version: compatibility.supported_client_versions,
            }
            .into()),
        }
    }

    fn graphql_url(&self) -> Result<url::Url, BlokliClientError> {
        let mut base = self.base_url.clone();
        if !base.path().ends_with('/') {
            base.set_path(&format!("{}/", base.path()));
        }
        Ok(base.join("graphql").map_err(ErrorKind::from)?)
    }

    fn build_reqwest_client(&self) -> Result<reqwest::Client, BlokliClientError> {
        Ok(reqwest::Client::builder()
            .timeout(self.cfg.timeout)
            .brotli(true)
            .gzip(true)
            .zstd(true)
            .deflate(true)
            .user_agent(format!("blokli-client/{}-{}", env!("CARGO_PKG_VERSION"), VERSION))
            .redirect(RedirectPolicy::limited(REDIRECT_LIMIT))
            .build()
            .map_err(ErrorKind::from)?)
    }

    fn build_subscription_reqwest_client(&self) -> Result<reqwest::Client, BlokliClientError> {
        let mut client_builder = reqwest::Client::builder()
            .connect_timeout(self.cfg.timeout)
            .tcp_keepalive(self.cfg.subscription_tcp_keepalive)
            .brotli(true)
            .gzip(true)
            .zstd(true)
            .deflate(true)
            .user_agent(format!("blokli-client/{}-{}", env!("CARGO_PKG_VERSION"), VERSION))
            .redirect(RedirectPolicy::limited(REDIRECT_LIMIT));

        if let Some(read_timeout) = self.cfg.subscription_read_timeout {
            client_builder = client_builder.read_timeout(read_timeout);
        }

        Ok(client_builder.build().map_err(ErrorKind::from)?)
    }

    fn build_subscription_stream<Q, V>(
        &self,
        op: cynic::StreamingOperation<Q, V>,
    ) -> Result<impl futures::Stream<Item = Result<Q, BlokliClientError>>, BlokliClientError>
    where
        Q: cynic::QueryFragment + cynic::serde::de::DeserializeOwned + 'static,
        V: cynic::QueryVariables + cynic::serde::Serialize,
    {
        let query = serde_json::to_string(&op).map_err(ErrorKind::from)?;
        tracing::debug!(query, "sending SSE query");
        let graphql_url = self.graphql_url()?;
        let reqwest_client = self.build_subscription_reqwest_client()?;

        Ok(futures::stream::try_unfold(
            SubscriptionStreamState::new(graphql_url, query, self.cfg.clone(), reqwest_client)?,
            move |mut stream_state| async move {
                loop {
                    if let Some(stream) = &mut stream_state.stream {
                        match stream.next().await {
                            Some(Ok(SSE::Event(event))) => {
                                tracing::debug!(?event, "SSE event");
                                let response = serde_json::from_str::<GraphQlResponse<Q>>(&event.data)
                                    .map_err(BlokliClientError::from)
                                    .and_then(response_to_data)?;
                                return Ok(Some((response, stream_state)));
                            }
                            Some(Ok(SSE::Comment(comment))) => {
                                tracing::debug!(comment, "SSE comment");
                            }
                            Some(Ok(SSE::Connected(details))) => {
                                tracing::debug!(?details, "SSE connection details");
                            }
                            Some(Err(error)) => {
                                tracing::warn!(%error, "SSE transport issue detected, continuing subscription");
                            }
                            None => {
                                if let Some(delay) = stream_state.cfg.subscription_stream_restart_delay {
                                    let actual_delay = delay.max(MIN_RECONNECTION_DELAY);
                                    tracing::warn!(
                                        ?actual_delay,
                                        "SSE stream ended, sleeping before attempting to restart"
                                    );
                                    futures_time::task::sleep(actual_delay.into()).await;
                                    if let Err(error) = stream_state.start_stream() {
                                        tracing::error!(%error, "Failed to restart SSE stream, stopping subscription");
                                        return Ok(None);
                                    }
                                } else {
                                    tracing::warn!(
                                        "SSE stream ended and no restart delay configured, stopping subscription"
                                    );
                                    return Ok(None);
                                }
                            }
                        }
                    } else {
                        tracing::warn!("SSE stream missing, stopping subscription");
                        return Ok(None);
                    }
                }
            },
        )
        .boxed())
    }

    fn build_query<Q, V>(
        &self,
        op: cynic::Operation<Q, V>,
    ) -> Result<impl Future<Output = Result<GraphQlResponse<Q>, BlokliClientError>>, BlokliClientError>
    where
        Q: cynic::QueryFragment + cynic::serde::de::DeserializeOwned + Debug + 'static,
        V: cynic::QueryVariables + cynic::serde::Serialize,
    {
        let client = self.build_reqwest_client()?;
        tracing::debug!(query = ?serde_json::to_string(&op), "sending Blokli query");

        Ok(client
            .post(self.graphql_url()?)
            .header("Accept", "application/json")
            .json(&op)
            .send()
            .map_err(BlokliClientError::from)
            .and_then(|resp| async {
                let body = resp.bytes().await.map_err(BlokliClientError::from)?;
                tracing::trace!(body = %String::from_utf8_lossy(body.as_ref()), "received Blokli response");
                serde_json::from_slice(&body).map_err(BlokliClientError::from)
            })
            .inspect_ok(|resp| tracing::debug!(?resp, "decoded Blokli response")))
    }
}

pub(crate) fn response_to_data<Q>(response: GraphQlResponse<Q>) -> crate::api::Result<Q> {
    match (response.data, response.errors) {
        (Some(data), None) => Ok(data),
        (Some(data), Some(errors)) => {
            tracing::error!(?errors, "operation succeeded but errors were encountered");
            Ok(data)
        }
        (None, Some(errors)) => Err(errors
            .into_iter()
            .reduce(|mut acc, next_err| {
                acc.message += &format!("{}{}", if acc.message.is_empty() { "" } else { ", " }, next_err.message,);

                if let Some(next_locs) = next_err.locations {
                    acc.locations.get_or_insert_default().extend(next_locs);
                }

                if let Some(next_paths) = next_err.path {
                    acc.path.get_or_insert_default().extend(next_paths);
                }

                acc
            })
            .map(ErrorKind::GraphQLError)
            .unwrap_or(ErrorKind::NoData)
            .into()),
        (None, None) => Err(ErrorKind::NoData.into()),
    }
}

#[cfg(test)]
mod tests {
    use cynic::GraphQlResponse;
    use futures::TryStreamExt;
    use launchdarkly_sdk_transport::HttpTransport;
    use serde_json::json;

    use super::{BlokliClient, BlokliClientConfig, ReqwestTransport, response_to_data};
    use crate::errors::ErrorKind;

    #[tokio::test]
    async fn reqwest_transport_returns_streaming_response_body() {
        let mut server = mockito::Server::new_async().await;
        let _mock = server
            .mock("GET", "/sse")
            .with_status(200)
            .with_header("Content-Type", "text/event-stream")
            .with_body("data: hello\n\n")
            .create_async()
            .await;

        let transport = ReqwestTransport::new(reqwest::Client::new());
        let request = launchdarkly_sdk_transport::Request::builder()
            .method("GET")
            .uri(format!("{}/sse", server.url()))
            .body(None)
            .expect("failed to build request");

        let response = transport.request(request).await.expect("request should succeed");
        let body_chunks: Vec<bytes::Bytes> = response
            .into_body()
            .try_collect()
            .await
            .expect("failed to collect response body");

        let body = body_chunks.into_iter().fold(Vec::new(), |mut acc, chunk| {
            acc.extend_from_slice(&chunk);
            acc
        });
        assert_eq!(body, b"data: hello\n\n");
    }

    #[tokio::test]
    async fn reqwest_transport_returns_error_for_invalid_request_uri() {
        let transport = ReqwestTransport::new(reqwest::Client::new());
        let request = launchdarkly_sdk_transport::Request::builder()
            .method("GET")
            .uri("/relative-only")
            .body(None)
            .expect("failed to build request");

        let error = match transport.request(request).await {
            Ok(_) => panic!("relative URI should fail"),
            Err(error) => error,
        };

        assert!(error.to_string().contains("builder error"));
    }

    #[test]
    fn graphql_url_appends_graphql_when_base_url_has_no_trailing_slash() {
        let client = BlokliClient::new(
            url::Url::parse("http://example.com/api").expect("valid URL"),
            BlokliClientConfig::default(),
        );

        let graphql_url = client.graphql_url().expect("graphql URL should be derived");

        assert_eq!(graphql_url.as_str(), "http://example.com/api/graphql");
    }

    #[test]
    fn graphql_url_preserves_trailing_slash() {
        let client = BlokliClient::new(
            url::Url::parse("http://example.com/api/").expect("valid URL"),
            BlokliClientConfig::default(),
        );

        let graphql_url = client.graphql_url().expect("graphql URL should be derived");

        assert_eq!(graphql_url.as_str(), "http://example.com/api/graphql");
    }

    #[test]
    fn response_to_data_returns_data_even_when_graphql_errors_are_present() {
        let response: GraphQlResponse<serde_json::Value> = serde_json::from_value(json!({
            "data": {
                "version": "1.2.3"
            },
            "errors": [
                {
                    "message": "partial failure"
                }
            ]
        }))
        .expect("response should deserialize");

        let data = response_to_data(response).expect("data should still be returned");

        assert_eq!(data["version"], "1.2.3");
    }

    #[test]
    fn response_to_data_merges_graphql_errors_when_data_is_missing() {
        let response: GraphQlResponse<serde_json::Value> = serde_json::from_value(json!({
            "errors": [
                {
                    "message": "first problem",
                    "locations": [{ "line": 1, "column": 2 }],
                    "path": ["query", "fieldA"]
                },
                {
                    "message": "second problem",
                    "locations": [{ "line": 3, "column": 4 }],
                    "path": ["query", "fieldB"]
                }
            ]
        }))
        .expect("response should deserialize");

        let error = response_to_data(response).expect_err("missing data should fail");

        match error.kind() {
            ErrorKind::GraphQLError(graphql_error) => {
                assert_eq!(graphql_error.message, "first problem, second problem");
                assert_eq!(graphql_error.locations.as_ref().map(Vec::len), Some(2));
                assert_eq!(graphql_error.path.as_ref().map(Vec::len), Some(4));
            }
            other => panic!("expected GraphQLError, got {other:?}"),
        }
    }

    #[test]
    fn response_to_data_returns_no_data_error_when_response_is_empty() {
        let response = GraphQlResponse::<serde_json::Value> {
            data: None,
            errors: None,
        };

        let error = response_to_data(response).expect_err("empty response should fail");

        assert!(matches!(error.kind(), ErrorKind::NoData));
    }
}
