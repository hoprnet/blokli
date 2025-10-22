use std::ops::RangeBounds;
use futures::stream::BoxStream;

mod errors;
mod client;
pub(crate) mod queries;

pub mod types {
    pub use super::queries::{
        accounts::Account,
        channels::Channel,
        info::ChainInfo,
        OpenedChannelsGraph,
        TokenValueString,
    };
}

pub enum AccountSelector {
    KeyId(i32),
    Address(String),
    PacketKey(String),
}

pub enum ChannelSelector {
    ConcreteChannelId(String),
    DestinationKeyId(i32),
    SourceKeyId(i32),
    SourceAndDestinationKeyIds(i32, i32),
}

#[async_trait::async_trait]
pub trait BlokliQueryClient {
    async fn count_accounts(&self, selector: Option<AccountSelector>) -> Result<usize, errors::BlokliClientError>;
    async fn query_accounts<'a>(&'a self, pages: impl RangeBounds<usize>, selector: Option<AccountSelector>) -> Result<BoxStream<'a, types::Account>, errors::BlokliClientError>;
    async fn count_channels(&self, selector: Option<ChannelSelector>) -> Result<usize, errors::BlokliClientError>;
    async fn query_channels<'a>(&'a self, pages: impl RangeBounds<usize>, selector: Option<ChannelSelector>) -> Result<BoxStream<'a, types::Channel>, errors::BlokliClientError>;
    async fn count_full_graph(&self) -> Result<usize, errors::BlokliClientError>;
    async fn query_full_graph<'a>(&'a self, pages: impl RangeBounds<usize>) -> Result<BoxStream<'a, types::OpenedChannelsGraph>, errors::BlokliClientError>;
    async fn query_chain_info(&self) -> Result<types::ChainInfo, errors::BlokliClientError>;
    async fn query_version(&self) -> Result<String, errors::BlokliClientError>;
    async fn query_health(&self) -> Result<String, errors::BlokliClientError>;
}

#[async_trait::async_trait]
pub trait BlokliSubscriptionClient {
    async fn subscribe_native_balance<'a>(&'a self, address: String) -> Result<BoxStream<'a, types::TokenValueString>, errors::BlokliClientError>;
    async fn subscribe_token_balance<'a>(&'a self, address: String) -> Result<BoxStream<'a, types::TokenValueString>, errors::BlokliClientError>;
}

pub use client::BlokliClient;
