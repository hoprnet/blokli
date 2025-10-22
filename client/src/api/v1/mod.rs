use crate::errors;
use futures::stream::BoxStream;

mod queries;
pub mod types {
    pub use super::queries::{
        TokenValueString,
        accounts::{
            Account, AccountVariables, BalanceVariables, HoprBalance, NativeBalance, QueryAccountCount,
            QueryAccountHoprBalance, QueryAccountNativeBalance, QueryAccounts, SubscribeAccountHoprBalance,
            SubscribeAccountNativeBalance, SubscribeAccounts,
        },
        channels::{Channel, ChannelsVariables, QueryChannels, SubscribeChannels},
        info::{ChainInfo, QueryChainInfo, QueryHealth, QueryVersion},
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
    async fn count_accounts(&self, selector: AccountSelector) -> Result<u32, errors::BlokliClientError>;
    async fn query_accounts<'a>(
        &'a self,
        selector: AccountSelector,
    ) -> Result<BoxStream<'a, types::Account>, errors::BlokliClientError>;
    async fn query_channels<'a>(
        &'a self,
        selector: ChannelSelector,
    ) -> Result<BoxStream<'a, types::Channel>, errors::BlokliClientError>;
    async fn query_chain_info(&self) -> Result<types::ChainInfo, errors::BlokliClientError>;
    async fn query_version(&self) -> Result<String, errors::BlokliClientError>;
    async fn query_health(&self) -> Result<String, errors::BlokliClientError>;
}

pub type EventStream<'a, T> = BoxStream<'a, Result<T, errors::BlokliClientError>>;

#[async_trait::async_trait]
pub trait BlokliSubscriptionClient {
    async fn subscribe_native_balance<'a>(
        &'a self,
        address: String,
    ) -> Result<EventStream<'a, types::NativeBalance>, errors::BlokliClientError>;
    async fn subscribe_token_balance<'a>(
        &'a self,
        address: String,
    ) -> Result<EventStream<'a, types::HoprBalance>, errors::BlokliClientError>;
    async fn subscribe_channels<'a>(
        &'a self,
        selector: Option<ChannelSelector>,
    ) -> Result<EventStream<'a, types::Channel>, errors::BlokliClientError>;
    async fn subscribe_accounts<'a>(
        &'a self,
        selector: Option<AccountSelector>,
    ) -> Result<EventStream<'a, types::Account>, errors::BlokliClientError>;
}
