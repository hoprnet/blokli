use super::schema;
use super::{ChannelStatus, DateTime, TokenValueString, Uint64};

// Channel query
#[derive(cynic::QueryVariables)]
pub struct ChannelsVariables {
    concrete_channel_id: Option<String>,
    destination_key_id: Option<i32>,
    source_key_id: Option<i32>,
}

#[derive(cynic::QueryFragment, Debug)]
#[cynic(
    graphql_type = "QueryRoot",
    variables = "ChannelsVariables"
)]
pub struct QueryChannels {
    #[arguments(concreteChannelId: $concrete_channel_id, destinationKeyId: $destination_key_id, sourceKeyId: $source_key_id)]
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
    pub ticket_index: Uint64,
}