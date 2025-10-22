use super::TokenValueString;
use super::schema;
use crate::api::v1::AccountSelector;

// Account queries
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
                keyid: Some(keyid),
                ..Default::default()
            },
            AccountSelector::Address(address) => AccountVariables {
                chain_key: Some(address),
                ..Default::default()
            },
            AccountSelector::PacketKey(address) => AccountVariables {
                packet_key: Some(address),
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
    pub accounts: Vec<Account>,
}

#[derive(cynic::QueryFragment, Debug)]
#[cynic(graphql_type = "SubscriptionRoot", variables = "AccountVariables")]
pub struct SubscribeAccounts {
    #[arguments(keyid: $keyid, packetKey: $packet_key, chainKey: $chain_key)]
    pub account_updated: Account,
}

#[derive(cynic::QueryFragment, Debug)]
pub struct Account {
    pub account_hopr_balance: TokenValueString,
    pub account_native_balance: TokenValueString,
    pub chain_key: String,
    pub keyid: i32,
    pub multi_addresses: Vec<String>,
    pub packet_key: String,
    pub safe_address: Option<String>,
    pub safe_hopr_balance: Option<TokenValueString>,
    pub safe_native_balance: Option<TokenValueString>,
}

// Account count query
#[derive(cynic::QueryFragment, Debug)]
#[cynic(graphql_type = "QueryRoot", variables = "AccountVariables")]
pub struct QueryAccountCount {
    #[arguments(keyid: $keyid, packetKey: $packet_key, chainKey: $chain_key)]
    pub account_count: i32,
}

// HOPR balance query
#[derive(cynic::QueryVariables)]
pub struct BalanceVariables {
    pub address: String,
}

#[derive(cynic::QueryFragment, Debug)]
#[cynic(graphql_type = "QueryRoot", variables = "BalanceVariables")]
pub struct QueryAccountHoprBalance {
    #[arguments(address: $address)]
    pub hopr_balance: Option<HoprBalance>,
}

#[derive(cynic::QueryFragment, Debug)]
#[cynic(graphql_type = "SubscriptionRoot", variables = "BalanceVariables")]
pub struct SubscribeAccountHoprBalance {
    #[arguments(address: $address)]
    pub hopr_balance_updated: Option<HoprBalance>,
}

#[derive(cynic::QueryFragment, Debug)]
pub struct HoprBalance {
    pub address: String,
    pub balance: TokenValueString,
}

// Native balance query

#[derive(cynic::QueryFragment, Debug)]
#[cynic(graphql_type = "QueryRoot", variables = "BalanceVariables")]
pub struct QueryAccountNativeBalance {
    #[arguments(address: $address)]
    pub native_balance: Option<NativeBalance>,
}

#[derive(cynic::QueryFragment, Debug)]
#[cynic(graphql_type = "SubscriptionRoot", variables = "BalanceVariables")]
pub struct SubscribeAccountNativeBalance {
    #[arguments(address: $address)]
    pub native_balance_updated: Option<NativeBalance>,
}

#[derive(cynic::QueryFragment, Debug)]
pub struct NativeBalance {
    pub address: String,
    pub balance: TokenValueString,
}
