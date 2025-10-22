use cynic::http::CynicReqwestError;

#[derive(Debug)]
pub struct BlokliClientError(Box<BlokliClientErrorKind>);

impl From<BlokliClientErrorKind> for BlokliClientError {
    fn from(kind: BlokliClientErrorKind) -> Self {
        Self(Box::new(kind))
    }
}

impl std::fmt::Display for BlokliClientError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl std::error::Error for BlokliClientError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(self.0.as_ref())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum BlokliClientErrorKind {
    #[error("no data returned from server unexpectedly")]
    NoData,
    #[error(transparent)]
    Subscription(#[from] eventsource_client::Error),
    #[error(transparent)]
    UrlParse(#[from] url::ParseError),
    #[error(transparent)]
    Serialization(#[from] serde_json::Error),
    #[error(transparent)]
    Reqwest(#[from] reqwest::Error),
    #[error(transparent)]
    Cynic(#[from] CynicReqwestError),
    #[error(transparent)]
    GraphQLError(#[from] cynic::GraphQlError),
}
