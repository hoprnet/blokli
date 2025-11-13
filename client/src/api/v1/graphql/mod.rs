use crate::errors::ErrorKind;

pub mod accounts;
pub mod balances;
pub mod channels;
pub mod graph;
pub mod info;
pub mod txs;

#[cynic::schema("blokli")]
pub(crate) mod schema {}

// https://generator.cynic-rs.dev/

#[derive(cynic::Enum, Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChannelStatus {
    #[cynic(rename = "OPEN")]
    Open,
    #[cynic(rename = "PENDINGTOCLOSE")]
    PendingToClose,
    #[cynic(rename = "CLOSED")]
    Closed,
}

#[derive(cynic::Scalar, Debug, Clone, PartialEq, Eq)]
pub struct DateTime(pub String);

#[derive(cynic::Scalar, Debug, Clone, PartialEq, Eq)]
pub struct TokenValueString(pub String);

#[derive(cynic::Scalar, Debug, Clone, PartialEq, Eq)]
#[cynic(graphql_type = "UInt64")]
pub struct Uint64(pub String);

#[derive(cynic::Scalar, Debug, Clone, PartialEq, Eq)]
pub struct Hex32(pub String);

#[derive(cynic::InlineFragments, Debug)]
pub enum CountResult {
    Count(Count),
    MissingFilterError(MissingFilterError),
    QueryFailedError(QueryFailedError),
    #[cynic(fallback)]
    Unknown,
}

impl From<CountResult> for Result<u32, crate::errors::BlokliClientError> {
    fn from(value: CountResult) -> Self {
        match value {
            CountResult::Count(count) => Ok(count.count as u32),
            CountResult::MissingFilterError(e) => Err(e.into()),
            CountResult::QueryFailedError(e) => Err(e.into()),
            CountResult::Unknown => Err(ErrorKind::NoData.into()),
        }
    }
}

#[derive(cynic::QueryFragment, Debug)]
pub struct Count {
    pub __typename: String,
    pub count: i32,
}

#[derive(cynic::QueryFragment, Debug)]
pub struct QueryFailedError {
    pub __typename: String,
    pub message: String,
    pub code: String,
}

impl From<QueryFailedError> for crate::errors::BlokliClientError {
    fn from(value: QueryFailedError) -> Self {
        ErrorKind::BlokliError {
            kind: "query failed",
            code: value.code,
            message: value.message,
        }
        .into()
    }
}

#[derive(cynic::QueryFragment, Debug)]
pub struct MissingFilterError {
    pub __typename: String,
    pub code: String,
    pub message: String,
}

impl From<MissingFilterError> for crate::errors::BlokliClientError {
    fn from(value: MissingFilterError) -> Self {
        ErrorKind::BlokliError {
            kind: "missing filter",
            code: value.code,
            message: value.message,
        }
        .into()
    }
}

#[derive(cynic::QueryFragment, Debug)]
pub struct InvalidAddressError {
    pub __typename: String,
    pub code: String,
    pub message: String,
}

impl From<InvalidAddressError> for crate::errors::BlokliClientError {
    fn from(value: InvalidAddressError) -> Self {
        ErrorKind::BlokliError {
            kind: "invalid address",
            code: value.code,
            message: value.message,
        }
        .into()
    }
}
