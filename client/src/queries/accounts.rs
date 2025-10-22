use super::schema;
use super::TokenValueString;


// Account queries
#[derive(cynic::QueryVariables)]
pub struct AccountVariables {
    keyid: Option<i32>,
    packet_key: Option<String>,
    chain_key: Option<String>,
}


#[derive(cynic::QueryFragment, Debug)]
#[cynic(
    graphql_type = "QueryRoot",
    variables = "AccountVariables"
)]
pub struct QueryAccount {
    #[arguments(keyid: $keyid, packetKey: $packet_key, chainKey: $chain_key)]
    pub accounts: Vec<Account>,
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
#[cynic(
    graphql_type = "QueryRoot",
    variables = "AccountVariables"
)]
pub struct QueryAccountCount {
    #[arguments(keyid: $keyid, packetKey: $packet_key, chainKey: $chain_key)]
    pub account_count: i32,
}


// HOPR balance query
#[derive(cynic::QueryVariables)]
pub struct BalanceVariables {
    address: String,
}

#[derive(cynic::QueryFragment, Debug)]
#[cynic(
    graphql_type = "QueryRoot",
    variables = "BalanceVariables"
)]
pub struct QueryAccountHoprBalance {
    #[arguments(address: $address)]
    pub hopr_balance: Option<HoprBalance>,
}

#[derive(cynic::QueryFragment, Debug)]
pub struct HoprBalance {
    pub address: String,
    pub balance: TokenValueString,
}

// Native balance query

#[derive(cynic::QueryFragment, Debug)]
#[cynic(
    graphql_type = "QueryRoot",
    variables = "BalanceVariables"
)]
pub struct QueryAccountNativeBalance {
    #[arguments(address: $address)]
    pub native_balance: Option<NativeBalance>,
}

#[derive(cynic::QueryFragment, Debug)]
pub struct NativeBalance {
    pub address: String,
    pub balance: TokenValueString,
}