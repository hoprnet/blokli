mod queries;
mod subscriptions;
#[cfg(feature = "testing")]
mod testing;
mod transactions;

use std::fmt::Debug;

use cynic::GraphQlResponse;
use eventsource_client::{Client, ReconnectOptionsBuilder};
use futures::{StreamExt, TryFutureExt, TryStreamExt};
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
    #[default(std::time::Duration::from_secs(10))]
    pub timeout: std::time::Duration,
    /// Reconnection timeout for SSE streams.
    #[default(std::time::Duration::from_secs(30))]
    pub stream_reconnect_timeout: std::time::Duration,
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
            .redirect(reqwest::redirect::Policy::limited(REDIRECT_LIMIT))
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
            .connect_timeout(self.cfg.timeout)
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
                    .delay(std::time::Duration::from_secs(2))
                    .backoff_factor(2)
                    .delay_max(self.cfg.stream_reconnect_timeout)
                    .build(),
            )
            .build();

        Ok(client
            .stream()
            .fuse()
            .inspect(|res| tracing::debug!(?res, "SSE response"))
            .map_err(|e| BlokliClientError::from(ErrorKind::from(e)))
            .try_filter_map(move |item| {
                futures::future::ready(match item {
                    eventsource_client::SSE::Event(event) => {
                        tracing::debug!(?event, "SSE event");
                        serde_json::from_str::<GraphQlResponse<Q>>(&event.data)
                            .map_err(|e| BlokliClientError::from(ErrorKind::from(e)))
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
            .and_then(|resp| resp.json::<GraphQlResponse<Q>>())
            .inspect_ok(|resp| tracing::debug!(?resp, "received Blokli response"))
            .map_err(|e| ErrorKind::from(e).into()))
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
