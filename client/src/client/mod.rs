mod queries;
mod subscriptions;
#[cfg(feature = "testing")]
mod testing;
mod transactions;

use std::{fmt::Debug, future::Future, time::Duration};

use cynic::GraphQlResponse;
use eventsource_client::{Client, ReconnectOptionsBuilder, SSE};
use futures::{StreamExt, TryFutureExt};
use reqwest::redirect::Policy as RedirectPolicy;
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
    /// Delay before recreating a completed SSE stream.
    #[default(Duration::from_secs(1))]
    pub subscription_stream_restart_delay: Duration,
    /// Recreate the subscription stream if closed
    #[default(true)]
    pub recreate_stream_on_close: bool,
}

/// Internal state for managing GraphQL subscription streams.
struct SubscriptionStreamState {
    graphql_url: url::Url,
    query: String,
    timeout: Duration,
    stream_reconnect_timeout: Duration,
    stream_restart_delay: Duration,
    stream: eventsource_client::BoxStream<eventsource_client::Result<SSE>>,
}

impl SubscriptionStreamState {
    fn new(graphql_url: url::Url, query: String, config: BlokliClientConfig) -> Result<Self, BlokliClientError> {
        let stream = Self::build_eventsource_stream_with_config(
            &graphql_url,
            &query,
            config.timeout,
            config.stream_reconnect_timeout,
        )?;

        Ok(Self {
            graphql_url,
            query,
            timeout: config.timeout,
            stream_reconnect_timeout: config.stream_reconnect_timeout,
            stream_restart_delay: config.subscription_stream_restart_delay,
            stream,
        })
    }

    fn build_eventsource_stream_with_config(
        graphql_url: &url::Url,
        query: &str,
        timeout: Duration,
        stream_reconnect_timeout: Duration,
    ) -> Result<eventsource_client::BoxStream<eventsource_client::Result<SSE>>, BlokliClientError> {
        let client = eventsource_client::ClientBuilder::for_url(graphql_url.as_str())
            .map_err(ErrorKind::from)?
            .connect_timeout(timeout)
            .header("Accept", "text/event-stream")
            .map_err(ErrorKind::from)?
            .header("Content-Type", "application/json")
            .map_err(ErrorKind::from)?
            .method("POST".into())
            .body(query.to_owned())
            .redirect_limit(REDIRECT_LIMIT as u32)
            .reconnect(
                ReconnectOptionsBuilder::new(true)
                    .retry_initial(true)
                    .delay(Duration::from_secs(2))
                    .backoff_factor(2)
                    .delay_max(stream_reconnect_timeout)
                    .build(),
            )
            .build();

        Ok(client.stream())
    }

    fn build_eventsource_stream(
        &self,
    ) -> Result<eventsource_client::BoxStream<eventsource_client::Result<SSE>>, BlokliClientError> {
        Self::build_eventsource_stream_with_config(
            &self.graphql_url,
            &self.query,
            self.timeout,
            self.stream_reconnect_timeout,
        )
    }

    async fn restart_stream(&mut self) -> Result<(), BlokliClientError> {
        tracing::warn!("SSE stream closed, recreating subscription stream");
        futures_time::task::sleep(self.stream_restart_delay.into()).await;
        self.stream = self.build_eventsource_stream()?;
        Ok(())
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
        let graphql_url = self.graphql_url()?;
        let stream_state = SubscriptionStreamState::new(graphql_url, query, self.cfg.clone())?;

        Ok(futures::stream::try_unfold(
            stream_state,
            move |mut stream_state: SubscriptionStreamState| async move {
                loop {
                    match stream_state.stream.next().await {
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
                            if self.cfg.recreate_stream_on_close {
                                stream_state.restart_stream().await?;
                            }
                        }
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
