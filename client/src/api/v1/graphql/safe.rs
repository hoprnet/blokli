use super::{InvalidAddressError, QueryFailedError, Uint64, schema};
use crate::{
    api::ChainAddress,
    errors::{BlokliClientError, ErrorKind},
};

#[derive(cynic::QueryVariables, Debug)]
pub struct SafeVariables {
    pub address: String,
}

#[derive(cynic::QueryFragment, Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct Safe {
    pub address: String,
    pub chain_key: String,
    pub module_address: String,
}

#[derive(cynic::QueryFragment, Debug)]
#[cynic(graphql_type = "QueryRoot", variables = "SafeVariables")]
pub struct QuerySafeByChainKey {
    #[arguments(chainKey: $address)]
    pub safe_by_chain_key: Option<SafeResult>,
}

#[derive(cynic::QueryFragment, Debug)]
#[cynic(graphql_type = "QueryRoot", variables = "SafeVariables")]
pub struct QuerySafeByAddress {
    #[arguments(address: $address)]
    pub safe: Option<SafeResult>,
}

#[derive(cynic::QueryFragment, Debug)]
#[cynic(graphql_type = "SubscriptionRoot")]
pub struct SubscribeSafeDeployment {
    pub safe_deployed: Safe,
}

#[derive(cynic::QueryVariables, Debug)]
pub struct ModuleAddressVariables {
    pub nonce: Uint64,
    pub owner: String,
    pub safe_address: String,
}

#[derive(cynic::QueryFragment, Debug, Clone, PartialEq, Eq)]
pub struct ModuleAddress {
    pub __typename: String,
    pub module_address: String,
}

#[derive(cynic::QueryFragment, Debug)]
#[cynic(graphql_type = "QueryRoot", variables = "ModuleAddressVariables")]
pub struct QueryModuleAddress {
    #[arguments(nonce: $nonce, safeAddress: $safe_address, owner: $owner)]
    pub calculate_module_address: CalculateModuleAddressResult,
}

#[derive(cynic::InlineFragments, Debug)]
pub enum SafeResult {
    Safe(Safe),
    InvalidAddressError(InvalidAddressError),
    QueryFailedError(QueryFailedError),
    #[cynic(fallback)]
    Unknown,
}

impl From<SafeResult> for Result<Option<Safe>, BlokliClientError> {
    fn from(value: SafeResult) -> Self {
        match value {
            SafeResult::Safe(safe) => Ok(Some(safe)),
            SafeResult::InvalidAddressError(e) => Err(e.into()),
            SafeResult::QueryFailedError(e) => Err(e.into()),
            SafeResult::Unknown => Err(ErrorKind::NoData.into()),
        }
    }
}

#[derive(cynic::InlineFragments, Debug)]
pub enum CalculateModuleAddressResult {
    ModuleAddress(ModuleAddress),
    InvalidAddressError(InvalidAddressError),
    QueryFailedError(QueryFailedError),
    #[cynic(fallback)]
    Unknown,
}

impl From<CalculateModuleAddressResult> for Result<ChainAddress, BlokliClientError> {
    fn from(value: CalculateModuleAddressResult) -> Self {
        match value {
            CalculateModuleAddressResult::ModuleAddress(address) => {
                let address = address.module_address.to_lowercase();
                Ok(hex::decode(address.trim_start_matches("0x"))
                    .map_err(|_| BlokliClientError::from(ErrorKind::ParseError))
                    .and_then(|bytes| bytes.try_into().map_err(|_| ErrorKind::ParseError.into()))?)
            }
            CalculateModuleAddressResult::InvalidAddressError(e) => Err(e.into()),
            CalculateModuleAddressResult::QueryFailedError(e) => Err(e.into()),
            CalculateModuleAddressResult::Unknown => Err(ErrorKind::NoData.into()),
        }
    }
}
