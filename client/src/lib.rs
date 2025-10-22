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
        TokenValueString,
    };
}

pub enum AccountSelector {
    KeyId(i32),
    Address(String),
    PacketKey(String),
}

pub enum ChannelSelector {
    ChannelId(String),
    DestinationKeyId(i32),
    SourceKeyId(i32),
    SourceAndDestinationKeyIds(i32, i32),
}

#[async_trait::async_trait]
pub trait BlokliQueryClient {
    async fn query_accounts<'a>(&'a self,  selector: AccountSelector) -> Result<BoxStream<'a, types::Account>, errors::BlokliClientError>;
    async fn query_channels<'a>(&'a self, selector: ChannelSelector) -> Result<BoxStream<'a, types::Channel>, errors::BlokliClientError>;
    async fn query_chain_info(&self) -> Result<types::ChainInfo, errors::BlokliClientError>;
    async fn query_version(&self) -> Result<String, errors::BlokliClientError>;
    async fn query_health(&self) -> Result<String, errors::BlokliClientError>;
}

#[async_trait::async_trait]
pub trait BlokliSubscriptionClient {
    async fn subscribe_native_balance<'a>(&'a self, address: String) -> Result<BoxStream<'a, types::TokenValueString>, errors::BlokliClientError>;
    async fn subscribe_token_balance<'a>(&'a self, address: String) -> Result<BoxStream<'a, types::TokenValueString>, errors::BlokliClientError>;
    async fn subscribe_channels<'a>(&'a self, selector: Option<ChannelSelector>) -> Result<BoxStream<'a, types::Channel>, errors::BlokliClientError>;
    async fn subscribe_accounts<'a>(&'a self, selector: Option<AccountSelector>) -> Result<BoxStream<'a, types::Account>, errors::BlokliClientError>;
}

pub use client::BlokliClient;
