#[derive(Debug, thiserror::Error)]
pub enum BlokliClientError {
    #[error(transparent)]
    SubscriptionError(#[from] eventsource_client::Error),
    #[error(transparent)]
    UrlParseError(#[from] url::ParseError),
}