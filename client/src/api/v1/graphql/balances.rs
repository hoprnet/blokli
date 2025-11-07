use super::{InvalidAddressError, QueryFailedError, schema};
use crate::{api::types::TokenValueString, errors::BlokliClientError};

#[derive(cynic::QueryVariables)]
pub struct BalanceVariables {
    pub address: String,
}

#[derive(cynic::QueryFragment, Debug)]
#[cynic(graphql_type = "QueryRoot", variables = "BalanceVariables")]
pub struct QueryHoprBalance {
    #[arguments(address: $address)]
    pub hopr_balance: HoprBalanceResult,
}

#[derive(cynic::QueryFragment, Debug)]
#[cynic(graphql_type = "QueryRoot", variables = "BalanceVariables")]
pub struct QueryNativeBalance {
    #[arguments(address: $address)]
    pub native_balance: NativeBalanceResult,
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

#[derive(cynic::QueryFragment, Debug, Clone, PartialEq, Eq)]
pub struct HoprBalance {
    pub __typename: String,
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

#[derive(cynic::QueryFragment, Debug, Clone, PartialEq, Eq)]
pub struct NativeBalance {
    pub __typename: String,
    pub balance: TokenValueString,
}

#[derive(cynic::QueryFragment, Debug, Clone, PartialEq, Eq)]
pub struct SafeHoprAllowance {
    pub __typename: String,
    pub allowance: TokenValueString,
}

#[derive(cynic::QueryFragment, Debug)]
#[cynic(graphql_type = "QueryRoot", variables = "BalanceVariables")]
pub struct QuerySafeAllowance {
    #[arguments(address: $address)]
    pub safe_hopr_allowance: SafeHoprAllowanceResult,
}

#[derive(cynic::InlineFragments, Debug)]
pub enum SafeHoprAllowanceResult {
    SafeHoprAllowance(SafeHoprAllowance),
    InvalidAddressError(InvalidAddressError),
    QueryFailedError(QueryFailedError),
    #[cynic(fallback)]
    Unknown,
}

impl From<SafeHoprAllowanceResult> for Result<SafeHoprAllowance, BlokliClientError> {
    fn from(value: SafeHoprAllowanceResult) -> Self {
        match value {
            SafeHoprAllowanceResult::SafeHoprAllowance(allowance) => Ok(allowance),
            SafeHoprAllowanceResult::InvalidAddressError(e) => Err(e.into()),
            SafeHoprAllowanceResult::QueryFailedError(e) => Err(e.into()),
            SafeHoprAllowanceResult::Unknown => Err(crate::errors::ErrorKind::NoData.into()),
        }
    }
}
