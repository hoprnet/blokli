mod queries;
mod subscriptions;

use crate::errors::{BlokliClientError, BlokliClientErrorKind};
use cynic::GraphQlResponse;
use futures::{TryFutureExt, TryStreamExt};
use url::Url;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BlokliClientConfig {
    pub url: Url,
    pub timeout: std::time::Duration,
}

pub struct BlokliClient {
    cfg: BlokliClientConfig,
}

impl BlokliClient {
    pub fn new(cfg: BlokliClientConfig) -> Self {
        Self { cfg }
    }

    fn base_url(&self) -> Result<Url, BlokliClientError> {
        Ok(self.cfg.url.join("graphql").map_err(BlokliClientErrorKind::from)?)
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
        let client = eventsource_client::ClientBuilder::for_url(self.base_url()?.as_str())
            .map_err(BlokliClientErrorKind::from)?
            .connect_timeout(self.cfg.timeout)
            .body(serde_json::to_string(&op).map_err(BlokliClientErrorKind::from)?)
            .build();

        Ok(client
            .stream()
            .map_err(BlokliClientErrorKind::from)
            .try_filter_map(move |item| {
                futures::future::ready(match item {
                    eventsource_client::SSE::Event(event) => serde_json::from_str::<Q>(&event.data)
                        .map(|f| Some(f))
                        .map_err(BlokliClientErrorKind::from),
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

        let client = reqwest::Client::builder()
            .build()
            .map_err(BlokliClientErrorKind::from)?;

        Ok(client
            .post(self.base_url()?)
            .run_graphql(op)
            .into_future()
            .map_err(|e| BlokliClientErrorKind::from(e).into()))
    }
}
