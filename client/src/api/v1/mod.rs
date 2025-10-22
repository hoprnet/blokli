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

pub(crate) type Result<T> = std::result::Result<T, errors::BlokliClientError>;

#[async_trait::async_trait]
pub trait BlokliQueryClient {
    async fn count_accounts(&self, selector: AccountSelector) -> Result<u32>;
    async fn query_accounts<'a>(&'a self, selector: AccountSelector) -> Result<BoxStream<'a, types::Account>>;
    async fn query_channels<'a>(&'a self, selector: ChannelSelector) -> Result<BoxStream<'a, types::Channel>>;
    async fn query_chain_info(&self) -> Result<types::ChainInfo>;
    async fn query_version(&self) -> Result<String>;
    async fn query_health(&self) -> Result<String>;
}

pub trait BlokliSubscriptionClient {
    fn subscribe_native_balance(
        &self,
        address: String,
    ) -> Result<impl futures::Stream<Item = Result<types::NativeBalance>>>;
    fn subscribe_token_balance(
        &self,
        address: String,
    ) -> Result<impl futures::Stream<Item = Result<types::HoprBalance>>>;
    fn subscribe_channels(
        &self,
        selector: Option<ChannelSelector>,
    ) -> Result<impl futures::Stream<Item = Result<types::Channel>>>;
    fn subscribe_accounts(
        &self,
        selector: Option<AccountSelector>,
    ) -> Result<impl futures::Stream<Item = Result<types::Account>>>;
}
