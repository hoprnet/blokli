use hex::ToHex;

use super::{
    ChannelStatus, CountResult, DateTime, MissingFilterError, QueryFailedError, TokenValueString, Uint64, schema,
};
use crate::api::v1::{ChannelFilter, ChannelSelector};

#[derive(cynic::QueryVariables, Default)]
pub struct ChannelsVariables {
    pub concrete_channel_id: Option<String>,
    pub destination_key_id: Option<i32>,
    pub source_key_id: Option<i32>,
    pub status: Option<ChannelStatus>,
}

impl From<ChannelSelector> for ChannelsVariables {
    fn from(value: ChannelSelector) -> Self {
        match value.filter {
            Some(ChannelFilter::ChannelId(id)) => ChannelsVariables {
                concrete_channel_id: Some(id.encode_hex()),
                status: value.status,
                ..Default::default()
            },
            Some(ChannelFilter::DestinationKeyId(dst)) => ChannelsVariables {
                destination_key_id: Some(dst as i32),
                status: value.status,
                ..Default::default()
            },
            Some(ChannelFilter::SourceKeyId(src)) => ChannelsVariables {
                source_key_id: Some(src as i32),
                status: value.status,
                ..Default::default()
            },
            Some(ChannelFilter::SourceAndDestinationKeyIds(src, dst)) => ChannelsVariables {
                destination_key_id: Some(dst as i32),
                source_key_id: Some(src as i32),
                status: value.status,
                ..Default::default()
            },
            None => ChannelsVariables {
                status: value.status,
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
    #[arguments(concreteChannelId: $concrete_channel_id, destinationKeyId: $destination_key_id, sourceKeyId: $source_key_id, status: $status)]
    pub channels: ChannelsResult,
}

#[derive(cynic::QueryFragment, Debug)]
#[cynic(graphql_type = "SubscriptionRoot", variables = "ChannelsVariables")]
pub struct SubscribeChannels {
    #[arguments(concreteChannelId: $concrete_channel_id, destinationKeyId: $destination_key_id, sourceKeyId: $source_key_id, status: $status)]
    pub channel_updated: Channel,
}

#[derive(cynic::QueryFragment, Debug)]
#[cynic(graphql_type = "QueryRoot", variables = "ChannelsVariables")]
pub struct QueryChannelCount {
    #[arguments(concreteChannelId: $concrete_channel_id, destinationKeyId: $destination_key_id, sourceKeyId: $source_key_id, status: $status)]
    pub channel_count: CountResult,
}

#[derive(cynic::QueryFragment, Debug)]
pub struct ChannelsList {
    pub __typename: String,
    pub channels: Vec<Channel>,
}

#[derive(cynic::QueryFragment, Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
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

#[derive(cynic::InlineFragments, Debug)]
pub enum ChannelsResult {
    ChannelsList(ChannelsList),
    MissingFilterError(MissingFilterError),
    QueryFailedError(QueryFailedError),
    #[cynic(fallback)]
    Unknown,
}

impl From<ChannelsResult> for Result<Vec<Channel>, crate::errors::BlokliClientError> {
    fn from(value: ChannelsResult) -> Self {
        match value {
            ChannelsResult::ChannelsList(list) => Ok(list.channels),
            ChannelsResult::MissingFilterError(e) => Err(e.into()),
            ChannelsResult::QueryFailedError(e) => Err(e.into()),
            ChannelsResult::Unknown => Err(crate::errors::ErrorKind::NoData.into()),
        }
    }
}
