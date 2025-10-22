pub mod accounts;
pub mod channels;
pub mod info;

#[cynic::schema("blokli")]
pub(crate) mod schema {}

// https://generator.cynic-rs.dev/

#[derive(cynic::QueryFragment, Debug)]
#[cynic(graphql_type = "QueryRoot")]
pub struct QueryFullGraph {
    pub opened_channels_graph: OpenedChannelsGraph,
}

#[derive(cynic::QueryFragment, Debug)]
pub struct OpenedChannelsGraph {
    pub accounts: Vec<accounts::Account>,
    pub channels: Vec<channels::Channel>,
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

#[derive(cynic::Scalar, Debug, Clone)]
#[cynic(graphql_type = "UInt64")]
pub struct Uint64(pub String);