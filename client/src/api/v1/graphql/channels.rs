use hex::ToHex;
use primitive_types::U256;

use super::{
    ChannelStatus, CountResult, DateTime, InvalidAddressError, MissingFilterError, QueryFailedError, TokenValueString,
    Uint64, schema,
};
use crate::{
    api::v1::{ChannelFilter, ChannelSelector},
    errors::ErrorKind,
};

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

#[derive(cynic::QueryFragment, Debug, Clone, PartialEq, Eq)]
#[cynic(graphql_type = "ChannelsList")]
pub struct ChannelsListQuery {
    pub __typename: String,
    pub channels: Vec<Channel>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct ChannelsList {
    pub __typename: String,
    pub channels: Vec<Channel>,
    pub total_balance: TokenValueString,
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
    ChannelsList(ChannelsListQuery),
    MissingFilterError(MissingFilterError),
    QueryFailedError(QueryFailedError),
    #[cynic(fallback)]
    Unknown,
}

impl From<ChannelsResult> for Result<ChannelsList, crate::errors::BlokliClientError> {
    fn from(value: ChannelsResult) -> Self {
        match value {
            ChannelsResult::ChannelsList(list) => {
                let mut total_balance = U256::zero();
                for channel in &list.channels {
                    let channel_balance = U256::from_dec_str(&channel.balance.0)
                        .map_err(|_| crate::errors::BlokliClientError::from(ErrorKind::ParseError))?;
                    total_balance = total_balance
                        .checked_add(channel_balance)
                        .ok_or_else(|| crate::errors::BlokliClientError::from(ErrorKind::ParseError))?;
                }
                Ok(ChannelsList {
                    __typename: list.__typename,
                    channels: list.channels,
                    total_balance: TokenValueString(total_balance.to_string()),
                })
            }
            ChannelsResult::MissingFilterError(e) => Err(e.into()),
            ChannelsResult::QueryFailedError(e) => Err(e.into()),
            ChannelsResult::Unknown => Err(crate::errors::ErrorKind::NoData.into()),
        }
    }
}

#[derive(cynic::QueryVariables, Default)]
pub struct SafesBalanceVariables {
    pub owner_address: Option<String>,
}

#[derive(cynic::QueryFragment, Debug)]
#[cynic(graphql_type = "QueryRoot", variables = "SafesBalanceVariables")]
pub struct QuerySafesBalance {
    #[arguments(ownerAddress: $owner_address)]
    pub safes_balance: SafesBalanceResult,
}

#[derive(cynic::QueryFragment, Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct SafesBalance {
    pub total_balance: TokenValueString,
    pub safe_count: i32,
}

#[derive(cynic::InlineFragments, Debug)]
pub enum SafesBalanceResult {
    InvalidAddressError(InvalidAddressError),
    QueryFailedError(QueryFailedError),
    SafesBalance(SafesBalance),
    #[cynic(fallback)]
    Unknown,
}

impl From<SafesBalanceResult> for Result<SafesBalance, crate::errors::BlokliClientError> {
    fn from(value: SafesBalanceResult) -> Self {
        match value {
            SafesBalanceResult::SafesBalance(balance) => Ok(balance),
            SafesBalanceResult::InvalidAddressError(e) => Err(e.into()),
            SafesBalanceResult::QueryFailedError(e) => Err(e.into()),
            SafesBalanceResult::Unknown => Err(crate::errors::ErrorKind::NoData.into()),
        }
    }
}
