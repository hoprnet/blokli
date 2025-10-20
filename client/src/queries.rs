#[cynic::schema("blokli")]
mod schema {}

// https://generator.cynic-rs.dev/

#[derive(cynic::QueryFragment, Debug)]
#[cynic(graphql_type = "QueryRoot")]
pub struct AccountsQuery {
    pub accounts: Vec<Account>,
}

#[derive(cynic::QueryFragment, Debug)]
pub struct Account {
    pub keyid: i32,
    pub multi_addresses: Vec<String>,
    pub packet_key: String,
    pub chain_key: String,
}

#[derive(cynic::QueryFragment, Debug)]
#[cynic(graphql_type = "QueryRoot")]
pub struct BalancesQuery {
    #[arguments(address: "")]
    pub hopr_balance: Option<HoprBalance>,
    #[arguments(address: "")]
    pub native_balance: Option<NativeBalance>,
}

#[derive(cynic::QueryFragment, Debug)]
pub struct NativeBalance {
    pub address: String,
    pub balance: TokenValueString,
}

#[derive(cynic::QueryFragment, Debug)]
pub struct HoprBalance {
    pub address: String,
    pub balance: TokenValueString,
}

#[derive(cynic::QueryFragment, Debug)]
#[cynic(graphql_type = "QueryRoot")]
pub struct BlokliVersionQuery {
    pub version: String,
}

#[derive(cynic::QueryFragment, Debug)]
#[cynic(graphql_type = "QueryRoot")]
pub struct ChainInfoQuery {
    pub chain_info: ChainInfo,
}

#[derive(cynic::QueryFragment, Debug)]
pub struct ChainInfo {
    pub block_number: i32,
    pub chain_id: i32,
    pub min_ticket_winning_probability: f64,
    pub ticket_price: TokenValueString,
}


#[derive(cynic::QueryFragment, Debug)]
#[cynic(graphql_type = "QueryRoot")]
pub struct ChannelsQuery {
    pub channels: Vec<Channel>,
}

#[derive(cynic::QueryFragment, Debug)]
#[cynic(graphql_type = "QueryRoot")]
pub struct OpenedChannelsGraphQuery {
    pub opened_channels_graph: OpenedChannelsGraph,
}

#[derive(cynic::QueryFragment, Debug)]
pub struct OpenedChannelsGraph {
    pub accounts: Vec<Account>,
    pub channels: Vec<Channel>,
}

#[derive(cynic::QueryFragment, Debug)]
pub struct Channel {
    pub balance: TokenValueString,
    pub closure_time: Option<DateTime>,
    pub concrete_channel_id: String,
    pub destination: i32,
    pub epoch: i32,
    pub source: i32,
    pub status: ChannelStatus,
    pub ticket_index: i32,
}

#[derive(cynic::Enum, Clone, Copy, Debug)]
pub enum ChannelStatus {
    Open,
    Pendingtoclose,
    Closed,
}

#[derive(cynic::Scalar, Debug, Clone)]
pub struct DateTime(pub String);

#[derive(cynic::Scalar, Debug, Clone)]
pub struct TokenValueString(pub String);

