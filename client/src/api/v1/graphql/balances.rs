use super::schema;
use super::{InvalidAddressError, QueryFailedError};
use crate::api::types::TokenValueString;
use crate::errors::BlokliClientError;

#[derive(cynic::QueryVariables)]
pub struct BalanceVariables {
    pub address: String,
}

#[derive(cynic::QueryFragment, Debug)]
#[cynic(graphql_type = "QueryRoot", variables = "BalanceVariables")]
pub struct QueryHoprBalance {
    #[arguments(address: $address)]
    pub hopr_balance: Option<HoprBalanceResult>,
}

#[derive(cynic::QueryFragment, Debug)]
#[cynic(graphql_type = "QueryRoot", variables = "BalanceVariables")]
pub struct QueryNativeBalance {
    #[arguments(address: $address)]
    pub native_balance: Option<NativeBalanceResult>,
}

#[derive(cynic::InlineFragments, Debug)]
pub enum HoprBalanceResult {
    HoprBalance(HoprBalance),
    InvalidAddressError(InvalidAddressError),
    QueryFailedError(QueryFailedError),
    #[cynic(fallback)]
    Unknown,
}

impl From<HoprBalanceResult> for Result<HoprBalance, BlokliClientError> {
    fn from(value: HoprBalanceResult) -> Self {
        match value {
            HoprBalanceResult::HoprBalance(balance) => Ok(balance),
            HoprBalanceResult::InvalidAddressError(e) => Err(e.into()),
            HoprBalanceResult::QueryFailedError(e) => Err(e.into()),
            HoprBalanceResult::Unknown => Err(crate::errors::ErrorKind::NoData.into()),
        }
    }
}

#[derive(cynic::QueryFragment, Debug)]
pub struct HoprBalance {
    pub balance: TokenValueString,
}

#[derive(cynic::InlineFragments, Debug)]
pub enum NativeBalanceResult {
    NativeBalance(NativeBalance),
    InvalidAddressError(InvalidAddressError),
    QueryFailedError(QueryFailedError),
    #[cynic(fallback)]
    Unknown,
}

impl From<NativeBalanceResult> for Result<NativeBalance, BlokliClientError> {
    fn from(value: NativeBalanceResult) -> Self {
        match value {
            NativeBalanceResult::NativeBalance(balance) => Ok(balance),
            NativeBalanceResult::InvalidAddressError(e) => Err(e.into()),
            NativeBalanceResult::QueryFailedError(e) => Err(e.into()),
            NativeBalanceResult::Unknown => Err(crate::errors::ErrorKind::NoData.into()),
        }
    }
}

#[derive(cynic::QueryFragment, Debug)]
pub struct NativeBalance {
    pub balance: TokenValueString,
}
