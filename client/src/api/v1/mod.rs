use crate::errors;

mod graphql;
pub mod types {
    pub use super::graphql::{
        TokenValueString,
        accounts::{Account, HoprBalance, NativeBalance},
        channels::Channel,
        info::ChainInfo,
    };
}

pub(crate) mod internal {
    pub use super::graphql::{
        accounts::{
            AccountVariables, BalanceVariables, QueryAccountCount, QueryAccountHoprBalance, QueryAccountNativeBalance,
            QueryAccounts, SubscribeAccountHoprBalance, SubscribeAccountNativeBalance, SubscribeAccounts,
        },
        channels::{ChannelsVariables, QueryChannels, SubscribeChannels},
        info::{QueryChainInfo, QueryHealth, QueryVersion},
    };
}

pub type ChainAddress = [u8; 20];
pub type PacketKey = [u8; 32];
pub type ChannelId = [u8; 32];
pub type KeyId = u32;

#[derive(Debug, Clone)]
pub enum AccountSelector {
    KeyId(KeyId),
    Address(ChainAddress),
    PacketKey(PacketKey),
}

#[derive(Debug, Clone)]
pub enum ChannelSelector {
    ChannelId(ChannelId),
    DestinationKeyId(KeyId),
    SourceKeyId(KeyId),
    SourceAndDestinationKeyIds(KeyId, KeyId),
}

pub(crate) type Result<T> = std::result::Result<T, errors::BlokliClientError>;

#[async_trait::async_trait]
pub trait BlokliQueryClient {
    async fn count_accounts(&self, selector: AccountSelector) -> Result<u32>;
    async fn query_accounts(&self, selector: AccountSelector) -> Result<Vec<types::Account>>;
    async fn query_native_balance(&self, address: &ChainAddress) -> Result<types::NativeBalance>;
    async fn query_token_balance(&self, address: &ChainAddress) -> Result<types::HoprBalance>;
    async fn query_channels(&self, selector: ChannelSelector) -> Result<Vec<types::Channel>>;
    async fn query_chain_info(&self) -> Result<types::ChainInfo>;
    async fn query_version(&self) -> Result<String>;
    async fn query_health(&self) -> Result<String>;
}

pub trait BlokliSubscriptionClient {
    fn subscribe_native_balance(
        &self,
        address: &ChainAddress,
    ) -> Result<impl futures::Stream<Item = Result<types::NativeBalance>>>;
    fn subscribe_token_balance(
        &self,
        address: &ChainAddress,
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
