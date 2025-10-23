use cynic::http::CynicReqwestError;

#[derive(Debug)]
pub struct BlokliClientError(Box<ErrorKind>);

impl From<ErrorKind> for BlokliClientError {
    fn from(kind: ErrorKind) -> Self {
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
pub enum ErrorKind {
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
