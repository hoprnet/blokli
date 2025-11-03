mod graphql;
pub mod types {
    pub use super::graphql::{
        ChannelStatus, TokenValueString,
        accounts::Account,
        balances::{HoprBalance, NativeBalance, SafeHoprAllowance},
        channels::Channel,
        graph::OpenedChannelsGraphEntry,
        info::ChainInfo,
        txs::{Transaction, TransactionStatus},
    };
}

pub(crate) mod internal {
    pub use super::graphql::{
        accounts::{AccountVariables, QueryAccountCount, QueryAccounts, SubscribeAccounts},
        balances::{BalanceVariables, QueryHoprBalance, QueryNativeBalance, QuerySafeAllowance},
        channels::{ChannelsVariables, QueryChannelCount, QueryChannels, SubscribeChannels},
        graph::SubscribeGraph,
        info::{QueryChainInfo, QueryHealth, QueryVersion},
        txs::{
            ConfirmTransactionVariables, MutateConfirmTransaction, MutateSendTransaction, MutateTrackTransaction,
            QueryTransaction, SendTransactionVariables, SubscribeTransaction, TransactionsVariables,
        },
    };
}

pub type ChainAddress = [u8; 20];
pub type PacketKey = [u8; 32];
pub type ChannelId = [u8; 32];
pub type TxReceipt = [u8; 32];
pub type KeyId = u32;
pub type TxId = String;

/// Allows selecting [`Accounts`](types::Account) by their key id, address or packet key.
#[derive(Debug, Clone)]
pub enum AccountSelector {
    /// Select an account by its key id.
    KeyId(KeyId),
    /// Select an account by its on-chain address.
    Address(ChainAddress),
    /// Select an account by its packet key.
    PacketKey(PacketKey),
}

/// Allows selecting [`Channels`](types::Channel) based on a [`ChannelFilter`] and optionally a [`ChannelStatus`].
#[derive(Debug, Clone)]
pub struct ChannelSelector {
    /// Filter for the selected channels.
    pub filter: ChannelFilter,
    /// Optional status filter for the selected channels.
    pub status: Option<types::ChannelStatus>,
}

/// Allows filtering [`Channels`](types::Channel) by their channel id, source and/or destination key id.
#[derive(Debug, Clone)]
pub enum ChannelFilter {
    /// Select a channel by its channel id.
    ChannelId(ChannelId),
    /// Select channels by its destination key id.
    DestinationKeyId(KeyId),
    /// Select channels by its source key id.
    SourceKeyId(KeyId),
    /// Select channels by both source and destination key id.
    SourceAndDestinationKeyIds(KeyId, KeyId),
}

pub(crate) type Result<T> = std::result::Result<T, crate::errors::BlokliClientError>;

/// Trait defining restricted queries to Blokli API.
#[async_trait::async_trait]
pub trait BlokliQueryClient {
    /// Counts the number of accounts optionally matching the given [`selector`](AccountSelector).
    async fn count_accounts(&self, selector: Option<AccountSelector>) -> Result<u32>;
    /// Queries the accounts matching the given [`selector`](AccountSelector).
    async fn query_accounts(&self, selector: AccountSelector) -> Result<Vec<types::Account>>;
    /// Queries the native balance of the given account.
    async fn query_native_balance(&self, address: &ChainAddress) -> Result<types::NativeBalance>;
    /// Queries the token balance of the given account.
    async fn query_token_balance(&self, address: &ChainAddress) -> Result<types::HoprBalance>;
    /// Queries the safe allowance of the given account.
    async fn query_safe_allowance(&self, address: &ChainAddress) -> Result<types::SafeHoprAllowance>;
    /// Counts the number of channels optionally matching the given [`selector`](ChannelSelector).
    async fn count_channels(&self, selector: Option<ChannelSelector>) -> Result<u32>;
    /// Queries the channels matching the given [`selector`](ChannelSelector).
    async fn query_channels(&self, selector: ChannelSelector) -> Result<Vec<types::Channel>>;
    /// Queries the status of the transaction given the `tx_id` previously returned by
    /// [`BlokliTransactionClient::submit_and_track_transaction`].
    async fn query_transaction_status(&self, tx_id: TxId) -> Result<types::Transaction>;
    /// Queries the chain info.
    async fn query_chain_info(&self) -> Result<types::ChainInfo>;
    /// Queries the version of the Blokli API.
    async fn query_version(&self) -> Result<String>;
    /// Queries the health of the Blokli server.
    async fn query_health(&self) -> Result<String>;
}

/// Trait defining subscriptions to Blokli API.
pub trait BlokliSubscriptionClient {
    /// Subscribes to channel updates optionally matching the given [`selector`](ChannelSelector).
    ///
    /// If no selector is given, subscribes to all channel updates.
    fn subscribe_channels(
        &self,
        selector: Option<ChannelSelector>,
    ) -> Result<impl futures::Stream<Item = Result<types::Channel>> + Send>;
    /// Subscribes to account updates optionally matching the given [`selector`](AccountSelector).
    ///
    /// If no selector is given, subscribes to all account updates.
    fn subscribe_accounts(
        &self,
        selector: Option<AccountSelector>,
    ) -> Result<impl futures::Stream<Item = Result<types::Account>> + Send>;
    /// Subscribes to updates of the entire channel graph.
    fn subscribe_graph(&self) -> Result<impl futures::Stream<Item = Result<types::OpenedChannelsGraphEntry>> + Send>;
}

/// Trait defining Blokli API for signed transaction submission to the chain.
#[async_trait::async_trait]
pub trait BlokliTransactionClient {
    /// Submits a signed transaction to the chain without waiting for confirmation.
    async fn submit_transaction(&self, signed_tx: &[u8]) -> Result<TxReceipt>;
    /// Submits a signed transaction to the chain and returns an ID that can be used to track the transaction
    /// status via subscription or query.
    async fn submit_and_track_transaction(&self, signed_tx: &[u8]) -> Result<TxId>;
    /// Submits a signed transaction to the chain and waits for the given number of confirmations.
    async fn submit_and_confirm_transaction(&self, signed_tx: &[u8], num_confirmations: usize) -> Result<TxReceipt>;
    /// Tracks the transaction given the `tx_id` previously returned
    /// by [`submit_and_track_transaction`](BlokliTransactionClient::submit_and_track_transaction) until it is confirmed or [fails](crate::errors::TrackingErrorKind).
    async fn track_transaction(&self, tx_id: TxId, client_timeout: std::time::Duration) -> Result<types::Transaction>;
}
