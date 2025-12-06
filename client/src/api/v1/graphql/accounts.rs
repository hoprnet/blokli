use hex::ToHex;

use super::{CountResult, InvalidAddressError, MissingFilterError, QueryFailedError, Uint64, schema};
use crate::{
    api::v1::AccountSelector,
    errors::{BlokliClientError, ErrorKind},
};

#[derive(cynic::QueryVariables, Default)]
pub struct AccountVariables {
    pub keyid: Option<i32>,
    pub packet_key: Option<String>,
    pub chain_key: Option<String>,
}

impl From<AccountSelector> for AccountVariables {
    fn from(value: AccountSelector) -> Self {
        match value {
            AccountSelector::KeyId(keyid) => AccountVariables {
                keyid: Some(keyid as i32),
                ..Default::default()
            },
            AccountSelector::Address(address) => AccountVariables {
                chain_key: Some(address.encode_hex()),
                ..Default::default()
            },
            AccountSelector::PacketKey(address) => AccountVariables {
                packet_key: Some(address.encode_hex()),
                ..Default::default()
            },
        }
    }
}
impl From<Option<AccountSelector>> for AccountVariables {
    fn from(value: Option<AccountSelector>) -> Self {
        value.map_or_else(Default::default, From::from)
    }
}

#[derive(cynic::QueryFragment, Debug)]
#[cynic(graphql_type = "QueryRoot", variables = "AccountVariables")]
pub struct QueryAccounts {
    #[arguments(keyid: $keyid, packetKey: $packet_key, chainKey: $chain_key)]
    pub accounts: AccountsResult,
}

#[derive(cynic::QueryFragment, Debug)]
#[cynic(graphql_type = "SubscriptionRoot", variables = "AccountVariables")]
pub struct SubscribeAccounts {
    #[arguments(keyid: $keyid, packetKey: $packet_key, chainKey: $chain_key)]
    pub account_updated: Account,
}

#[derive(cynic::QueryFragment, Debug)]
#[cynic(graphql_type = "QueryRoot", variables = "AccountVariables")]
pub struct QueryAccountCount {
    #[arguments(keyid: $keyid, packetKey: $packet_key, chainKey: $chain_key)]
    pub account_count: CountResult,
}

#[derive(cynic::QueryFragment, Debug)]
pub struct AccountsList {
    pub __typename: String,
    pub accounts: Vec<Account>,
}

#[derive(cynic::QueryFragment, Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct Account {
    pub chain_key: String,
    pub keyid: i32,
    pub multi_addresses: Vec<String>,
    pub packet_key: String,
    pub safe_address: Option<String>,
}

#[derive(cynic::QueryVariables, Debug)]
pub struct SafeVariables {
    pub address: String,
}

#[derive(cynic::QueryFragment, Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct Safe {
    pub __typename: String,
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

#[derive(cynic::InlineFragments, Debug)]
pub enum AccountsResult {
    AccountsList(AccountsList),
    MissingFilterError(MissingFilterError),
    QueryFailedError(QueryFailedError),
    #[cynic(fallback)]
    Unknown,
}

impl From<AccountsResult> for Result<Vec<Account>, BlokliClientError> {
    fn from(value: AccountsResult) -> Self {
        match value {
            AccountsResult::AccountsList(list) => Ok(list.accounts),
            AccountsResult::MissingFilterError(e) => Err(e.into()),
            AccountsResult::QueryFailedError(e) => Err(e.into()),
            AccountsResult::Unknown => Err(ErrorKind::NoData.into()),
        }
    }
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

#[derive(cynic::QueryVariables, Debug)]
pub struct TxCountVariables {
    pub address: String,
}

#[derive(cynic::QueryFragment, Debug)]
pub struct SafeTransactionCount {
    pub __typename: String,
    pub count: Uint64,
}

#[derive(cynic::QueryFragment, Debug)]
#[cynic(graphql_type = "QueryRoot", variables = "TxCountVariables")]
pub struct QueryTxCount {
    #[arguments(address: $address)]
    pub safe_transaction_count: SafeTransactionCountResult,
}

#[derive(cynic::InlineFragments, Debug)]
pub enum SafeTransactionCountResult {
    SafeTransactionCount(SafeTransactionCount),
    InvalidAddressError(InvalidAddressError),
    QueryFailedError(QueryFailedError),
    #[cynic(fallback)]
    Unknown,
}

impl From<SafeTransactionCountResult> for Result<u64, BlokliClientError> {
    fn from(value: SafeTransactionCountResult) -> Self {
        match value {
            SafeTransactionCountResult::SafeTransactionCount(count) => {
                Ok(count.count.0.parse().map_err(|_| ErrorKind::ParseError)?)
            }
            SafeTransactionCountResult::InvalidAddressError(e) => Err(e.into()),
            SafeTransactionCountResult::QueryFailedError(e) => Err(e.into()),
            SafeTransactionCountResult::Unknown => Err(ErrorKind::NoData.into()),
        }
    }
}
