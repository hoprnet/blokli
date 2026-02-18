use std::{fmt::Formatter, time::Duration};

mod graphql;
pub mod types {
    pub use super::graphql::{
        ChannelStatus, DateTime, Hex32, TokenValueString, Uint64,
        accounts::Account,
        balances::{HoprBalance, NativeBalance, RedeemedStats, SafeHoprAllowance, SafeRedeemedStats},
        channels::Channel,
        graph::OpenedChannelsGraphEntry,
        info::{ChainInfo, ContractAddressMap, TicketParameters},
        safe::{ModuleAddress, Safe},
        txs::{Transaction, TransactionStatus},
    };
}

pub(crate) mod internal {
    pub use super::graphql::{
        accounts::{
            AccountVariables, QueryAccountCount, QueryAccounts, QueryTxCount, SubscribeAccounts, TxCountVariables,
        },
        balances::{
            BalanceVariables, QueryHoprBalance, QueryNativeBalance, QueryRedeemedStats, QuerySafeAllowance,
            RedeemedStatsVariables,
        },
        channels::{ChannelsVariables, QueryChannelCount, QueryChannels, SubscribeChannels},
        graph::SubscribeGraph,
        info::{QueryChainInfo, QueryHealth, QueryVersion, SubscribeTicketParams},
        safe::{
            ModuleAddressVariables, QueryModuleAddress, QuerySafeByAddress, QuerySafeByChainKey,
            QuerySafeByRegisteredNode, SafeVariables, SubscribeSafeDeployment,
        },
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
#[derive(Clone)]
pub enum AccountSelector {
    /// Select an account by its key id.
    KeyId(KeyId),
    /// Select an account by its on-chain address.
    Address(ChainAddress),
    /// Select an account by its packet key.
    PacketKey(PacketKey),
    /// Matches any account.
    Any,
}

impl std::fmt::Debug for AccountSelector {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::KeyId(key_id) => write!(f, "KeyId({})", key_id),
            Self::Address(address) => write!(f, "Address({})", hex::encode(address)),
            Self::PacketKey(packet_key) => write!(f, "PacketKey({})", hex::encode(packet_key)),
            AccountSelector::Any => write!(f, "Any"),
        }
    }
}

/// Allows selecting [`Channels`](types::Channel) based on a [`ChannelFilter`] and optionally a [`ChannelStatus`].
#[derive(Debug, Clone)]
pub struct ChannelSelector {
    /// Filter for the selected channels.
    pub filter: Option<ChannelFilter>,
    /// Optional status filter for the selected channels.
    pub status: Option<types::ChannelStatus>,
}

impl ChannelSelector {
    /// Returns `true` if the selector matches any channel.
    pub fn matches_all(&self) -> bool {
        self.filter.is_none() && self.status.is_none()
    }
}

/// Allows filtering [`Channels`](types::Channel) by their channel id, source and/or destination key id.
#[derive(Clone)]
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

impl std::fmt::Debug for ChannelFilter {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ChannelId(channel_id) => write!(f, "ChannelId({})", hex::encode(channel_id)),
            Self::DestinationKeyId(key_id) => write!(f, "DestinationKeyId({})", key_id),
            Self::SourceKeyId(key_id) => write!(f, "SourceKeyId({})", key_id),
            Self::SourceAndDestinationKeyIds(source_key_id, destination_key_id) => write!(
                f,
                "SourceAndDestinationKeyIds({}, {})",
                source_key_id, destination_key_id
            ),
        }
    }
}

/// Allows querying existing [`Safes`](types::Safe) by their address or chain key.
#[derive(Clone)]
pub enum SafeSelector {
    /// Select a safe by its address.
    SafeAddress(ChainAddress),
    /// Select a safe by the owner's chain key.
    ChainKey(ChainAddress),
    /// Select a safe by any of the registered nodes.
    RegisteredNode(ChainAddress),
}

impl std::fmt::Debug for SafeSelector {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::SafeAddress(address) => write!(f, "SafeAddress({})", hex::encode(address)),
            Self::ChainKey(address) => write!(f, "ChainKey({})", hex::encode(address)),
            Self::RegisteredNode(address) => write!(f, "RegisteredNode({})", hex::encode(address)),
        }
    }
}

/// Allows querying redeemed ticket aggregates by safe and/or node.
#[derive(Debug, Clone, Copy, Default)]
pub struct RedeemedStatsSelector {
    /// Optional safe address filter.
    pub safe_address: Option<ChainAddress>,
    /// Optional destination node address filter.
    pub node_address: Option<ChainAddress>,
}

/// Input for the [`query_module_address_prediction`] query.
#[derive(Clone, PartialEq, Eq)]
pub struct ModulePredictionInput {
    /// Safe deployment nonce.
    pub nonce: u64,
    /// Owner of the deployed Safe.
    pub owner: ChainAddress,
    /// Predicted Safe address.
    pub safe_address: ChainAddress,
}

impl std::fmt::Debug for ModulePredictionInput {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ModulePredictionInput")
            .field("nonce", &self.nonce)
            .field("owner", &hex::encode(self.owner))
            .field("safe_address", &hex::encode(self.safe_address))
            .finish()
    }
}

pub(crate) type Result<T> = std::result::Result<T, crate::errors::BlokliClientError>;

/// Trait defining restricted queries to Blokli API.
#[async_trait::async_trait]
pub trait BlokliQueryClient {
    /// Counts the number of accounts optionally matching the given [`selector`](AccountSelector).
    async fn count_accounts(&self, selector: AccountSelector) -> Result<u32>;
    /// Queries the accounts matching the given [`selector`](AccountSelector).
    async fn query_accounts(&self, selector: AccountSelector) -> Result<Vec<types::Account>>;
    /// Queries the native balance of the given account.
    async fn query_native_balance(&self, address: &ChainAddress) -> Result<types::NativeBalance>;
    /// Queries the token balance of the given account.
    async fn query_token_balance(&self, address: &ChainAddress) -> Result<types::HoprBalance>;
    /// Queries the number of transactions sent from the given account.
    async fn query_transaction_count(&self, address: &ChainAddress) -> Result<u64>;
    /// Queries the safe allowance of the given account.
    async fn query_safe_allowance(&self, address: &ChainAddress) -> Result<types::SafeHoprAllowance>;
    /// Queries redeemed ticket stats filtered by safe, node, or both.
    async fn query_redeemed_stats(&self, selector: RedeemedStatsSelector) -> Result<types::RedeemedStats>;
    /// Queries the deployed Safe by the given [`selector`](SafeSelector).
    async fn query_safe(&self, selector: SafeSelector) -> Result<Option<types::Safe>>;
    /// Queries the module address prediction of the given [Safe deployment data](ModulePredictionInput).
    async fn query_module_address_prediction(&self, input: ModulePredictionInput) -> Result<ChainAddress>;
    /// Counts the number of channels matching the given [`selector`](ChannelSelector).
    async fn count_channels(&self, selector: ChannelSelector) -> Result<u32>;
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
    /// Subscribes to channel updates matching the given [`selector`](ChannelSelector).
    ///
    /// If no selector is given, subscribes to all channel updates.
    fn subscribe_channels(
        &self,
        selector: ChannelSelector,
    ) -> Result<impl futures::Stream<Item = Result<types::Channel>> + Send>;
    /// Subscribes to account updates optionally matching the given [`selector`](AccountSelector).
    ///
    /// If no selector is given, subscribes to all account updates.
    fn subscribe_accounts(
        &self,
        selector: AccountSelector,
    ) -> Result<impl futures::Stream<Item = Result<types::Account>> + Send>;
    /// Subscribes to updates of the entire channel graph.
    fn subscribe_graph(&self) -> Result<impl futures::Stream<Item = Result<types::OpenedChannelsGraphEntry>> + Send>;
    /// Subscribes to updates of the ticket parameters.
    fn subscribe_ticket_params(&self) -> Result<impl futures::Stream<Item = Result<types::TicketParameters>> + Send>;
    /// Subscribes to on-chain Safe deployments.
    fn subscribe_safe_deployments(&self) -> Result<impl futures::Stream<Item = Result<types::Safe>> + Send>;
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
    /// by [`submit_and_track_transaction`](BlokliTransactionClient::submit_and_track_transaction) until it is confirmed
    /// or [fails](crate::errors::TrackingErrorKind).
    async fn track_transaction(&self, tx_id: TxId, client_timeout: Duration) -> Result<types::Transaction>;
}
