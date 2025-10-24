use super::schema;
use super::{CountResult, MissingFilterError, QueryFailedError, Uint64};
use crate::api::v1::AccountSelector;
use crate::errors::{BlokliClientError, ErrorKind};
use hex::ToHex;

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

#[derive(cynic::QueryFragment, Debug)]
pub struct Account {
    pub chain_key: String,
    pub keyid: i32,
    pub multi_addresses: Vec<String>,
    pub packet_key: String,
    pub safe_address: Option<String>,
    pub safe_transaction_count: Option<Uint64>,
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
