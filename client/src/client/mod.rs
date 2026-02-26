mod queries;
mod subscriptions;
#[cfg(feature = "testing")]
mod testing;
mod transactions;

use std::{fmt::Debug, future::Future, time::Duration};

use cynic::GraphQlResponse;
use eventsource_client::{Client, ReconnectOptionsBuilder};
use futures::{StreamExt, TryFutureExt, TryStreamExt};
use http_body_util::BodyExt;
use launchdarkly_sdk_transport::{ByteStream, HttpTransport, ResponseFuture, TransportError};
use reqwest::redirect::Policy as RedirectPolicy;
use tower::ServiceExt;
use tower_reqwest::HttpClientService;
#[cfg(feature = "testing")]
pub use testing::{
    BlokliTestClient, BlokliTestState, BlokliTestStateMutator, BlokliTestStateSnapshot, NopStateMutator,
};

use crate::{
    api::VERSION,
    errors::{BlokliClientError, ErrorKind},
};

/// Configuration for the [`BlokliClient`].
#[derive(Clone, Debug, PartialEq, Eq, smart_default::SmartDefault)]
pub struct BlokliClientConfig {
    /// General timeout for all requests.
    #[default(Duration::from_secs(10))]
    pub timeout: Duration,
    /// Reconnection timeout for SSE streams.
    #[default(Duration::from_secs(30))]
    pub stream_reconnect_timeout: Duration,
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

#[derive(Clone, Debug)]
struct ReqwestTransport {
    service: HttpClientService<reqwest::Client>,
}

impl ReqwestTransport {
    fn new(client: reqwest::Client) -> Self {
        Self {
            service: HttpClientService::new(client),
        }
    }
}

impl HttpTransport for ReqwestTransport {
    fn request(&self, request: http::Request<Option<bytes::Bytes>>) -> ResponseFuture {
        let service = self.service.clone();
        Box::pin(async move {
            let (parts, body) = request.into_parts();
            let body = match body {
                Some(body) => reqwest::Body::from(body),
                None => reqwest::Body::default(),
            };
            let request = http::Request::from_parts(parts, body);
            let response = service.oneshot(request).await.map_err(TransportError::new)?;
            let (parts, body) = response.into_parts();
            let body: ByteStream = Box::pin(body.into_data_stream().map_err(TransportError::new));
            Ok(http::Response::from_parts(parts, body))
        })
    }
}

impl BlokliClient {
    pub fn new(base_url: url::Url, cfg: BlokliClientConfig) -> Self {
        Self { base_url, cfg }
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

        let client = eventsource_client::ClientBuilder::for_url(self.graphql_url()?.as_str())
            .map_err(ErrorKind::from)?
            .header("Accept", "text/event-stream")
            .map_err(ErrorKind::from)?
            .header("Content-Type", "application/json")
            .map_err(ErrorKind::from)?
            .method("POST".into())
            .body(query)
            .redirect_limit(REDIRECT_LIMIT as u32)
            .reconnect(
                ReconnectOptionsBuilder::new(true)
                    .retry_initial(false)
                    .delay(Duration::from_secs(2))
                    .backoff_factor(2)
                    .delay_max(self.cfg.stream_reconnect_timeout)
                    .build(),
            )
            .build_with_transport(ReqwestTransport::new(self.build_reqwest_client()?));

        Ok(client
            .stream()
            .fuse()
            .inspect(|res| tracing::debug!(?res, "SSE response"))
            .map_err(BlokliClientError::from)
            .try_filter_map(move |item| {
                futures::future::ready(match item {
                    eventsource_client::SSE::Event(event) => {
                        tracing::debug!(?event, "SSE event");
                        serde_json::from_str::<GraphQlResponse<Q>>(&event.data)
                            .map_err(BlokliClientError::from)
                            .and_then(response_to_data)
                            .map(Some)
                    }
                    eventsource_client::SSE::Comment(comment) => {
                        tracing::debug!(comment, "SSE comment");
                        Ok::<_, BlokliClientError>(None)
                    }
                    eventsource_client::SSE::Connected(details) => {
                        tracing::debug!(?details, "SSE connection details");
                        Ok::<_, BlokliClientError>(None)
                    }
                })
            }))
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
    use super::ReqwestTransport;

    use futures::TryStreamExt;
    use launchdarkly_sdk_transport::HttpTransport;

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
}
