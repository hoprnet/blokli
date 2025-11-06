mod queries;
mod subscriptions;
#[cfg(feature = "testing")]
mod testing;
mod transactions;

use cynic::GraphQlResponse;
use futures::{TryFutureExt, TryStreamExt};
#[cfg(feature = "testing")]
pub use testing::BlokliTestClient;
use url::Url;

use crate::{
    api::VERSION,
    errors::{BlokliClientError, ErrorKind},
};

#[cfg(feature = "testing")]
pub use testing::{BlokliTestClient, MockBlokliTransactionClientImpl};

/// Configuration for the [`BlokliClient`].
#[derive(Clone, Debug, PartialEq, Eq, smart_default::SmartDefault)]
pub struct BlokliClientConfig {
    /// General timeout for all requests.
    #[default(std::time::Duration::from_secs(10))]
    pub timeout: std::time::Duration,
}

/// Client implementation of the Blokli API.
///
/// The client implements the following Blokli API traits:
/// - [`BlokliQueryClient`](api::BlokliQueryClient)
/// - [`BlokliSubscriptionClient`](api::BlokliSubscriptionClient).
/// - [`BlokliTransactionClient`](api::BlokliTransactionClient)
#[derive(Clone, Debug)]
pub struct BlokliClient {
    base_url: Url,
    cfg: BlokliClientConfig,
}

const REDIRECT_LIMIT: usize = 3;

impl BlokliClient {
    pub fn new(base_url: Url, cfg: BlokliClientConfig) -> Self {
        Self { base_url, cfg }
    }

    fn graphql_url(&self) -> Result<Url, BlokliClientError> {
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
        use eventsource_client::Client;
        let client = eventsource_client::ClientBuilder::for_url(self.graphql_url()?.as_str())
            .map_err(ErrorKind::from)?
            .connect_timeout(self.cfg.timeout)
            .header("Accept", "text/event-stream")
            .map_err(ErrorKind::from)?
            .header("Content-Type", "application/json")
            .map_err(ErrorKind::from)?
            .method("GET".into())
            .body(serde_json::to_string(&op).map_err(ErrorKind::from)?)
            .redirect_limit(REDIRECT_LIMIT as u32)
            .build();

        Ok(client
            .stream()
            .map_err(ErrorKind::from)
            .try_filter_map(move |item| {
                futures::future::ready(match item {
                    eventsource_client::SSE::Event(event) => serde_json::from_str::<Q>(&event.data)
                        .map(|f| Some(f))
                        .map_err(ErrorKind::from),
                    eventsource_client::SSE::Comment(comment) => {
                        tracing::debug!(comment, "SSE comment");
                        Ok(None)
                    }
                    eventsource_client::SSE::Connected(details) => {
                        tracing::debug!(?details, "SSE connection details");
                        Ok(None)
                    }
                })
            })
            .map_err(BlokliClientError::from))
    }

    fn build_query<Q, V>(
        &self,
        op: cynic::Operation<Q, V>,
    ) -> Result<impl Future<Output = Result<GraphQlResponse<Q>, BlokliClientError>>, BlokliClientError>
    where
        Q: cynic::QueryFragment + cynic::serde::de::DeserializeOwned + 'static,
        V: cynic::QueryVariables + cynic::serde::Serialize,
    {
        use cynic::http::ReqwestExt;

        let client = self.build_reqwest_client()?;

        Ok(client
            .post(self.graphql_url()?)
            .run_graphql(op)
            .into_future()
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
        (None, Some(errors)) => {
            if !errors.is_empty() {
                Err(ErrorKind::GraphQLError(errors.first().cloned().unwrap()).into())
            } else {
                Err(ErrorKind::NoData.into())
            }
        }
        (None, None) => Err(ErrorKind::NoData.into()),
    }
}
