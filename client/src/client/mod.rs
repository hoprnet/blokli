use url::Url;
use crate::errors::BlokliClientError;
use crate::queries;

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
        Self {
            cfg,
        }
    }

    fn build_subscription_client(&self) -> Result<impl eventsource_client::Client, BlokliClientError> {
        Ok(eventsource_client::ClientBuilder::for_url(self.cfg.url.join("graphql")?.as_str())?
            .connect_timeout(self.cfg.timeout)
            .build())
    }

}