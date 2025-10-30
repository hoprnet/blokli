use cynic::http::CynicReqwestError;

/// Error type for the Blokli client.
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

/// Error kinds for transaction tracking failure.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum TrackingErrorKind {
    /// Transaction was reverted.
    Reverted,
    /// Transaction timed out in Blokli.
    Timeout,
    /// Transaction submission failed.
    SubmissionFailed,
    /// Transaction validation failed.
    ValidationFailed,
}

/// Error kinds for the Blokli client.
#[derive(Debug, thiserror::Error)]
pub enum ErrorKind {
    #[error("no data returned from blokli unexpectedly")]
    NoData,
    #[error("blokli: {kind} ({code}): {message}")]
    BlokliError {
        kind: &'static str,
        code: String,
        message: String,
    },
    #[error("transaction tracking error: {0:?}")]
    TrackingError(TrackingErrorKind),
    #[error("data returned from blokli was unparseable")]
    ParseError,
    #[error("operation timed out at the client")]
    Timeout,
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
