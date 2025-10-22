use super::schema;
use super::{ChannelStatus, DateTime, TokenValueString, Uint64};
use crate::api::v1::ChannelSelector;

// Channel query
#[derive(cynic::QueryVariables, Default)]
pub struct ChannelsVariables {
    pub concrete_channel_id: Option<String>,
    pub destination_key_id: Option<i32>,
    pub source_key_id: Option<i32>,
}

impl From<ChannelSelector> for ChannelsVariables {
    fn from(value: ChannelSelector) -> Self {
        match value {
            ChannelSelector::ChannelId(id) => ChannelsVariables {
                concrete_channel_id: Some(id),
                ..Default::default()
            },
            ChannelSelector::DestinationKeyId(dst) => ChannelsVariables {
                destination_key_id: Some(dst),
                ..Default::default()
            },
            ChannelSelector::SourceKeyId(src) => ChannelsVariables {
                source_key_id: Some(src),
                ..Default::default()
            },
            ChannelSelector::SourceAndDestinationKeyIds(src, dst) => ChannelsVariables {
                destination_key_id: Some(src),
                source_key_id: Some(dst),
                ..Default::default()
            },
        }
    }
}

impl From<Option<ChannelSelector>> for ChannelsVariables {
    fn from(value: Option<ChannelSelector>) -> Self {
        value.map_or_else(Default::default, From::from)
    }
}

#[derive(cynic::QueryFragment, Debug)]
#[cynic(graphql_type = "QueryRoot", variables = "ChannelsVariables")]
pub struct QueryChannels {
    #[arguments(concreteChannelId: $concrete_channel_id, destinationKeyId: $destination_key_id, sourceKeyId: $source_key_id)]
    pub channels: Vec<Channel>,
}

#[derive(cynic::QueryFragment, Debug)]
#[cynic(graphql_type = "SubscriptionRoot", variables = "ChannelsVariables")]
pub struct SubscribeChannels {
    #[arguments(concreteChannelId: $concrete_channel_id, destinationKeyId: $destination_key_id, sourceKeyId: $source_key_id)]
    pub channel_updated: Channel,
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
