//! GraphQL type definitions for HOPR blokli API
//!
//! This crate contains pure GraphQL type definitions that can be reused
//! by clients without depending on the full API server implementation.

use async_graphql::{Enum, NewType, SimpleObject};

/// Token value represented as a string to maintain precision
///
/// This scalar type represents token amounts as decimal strings to avoid
/// floating-point precision issues. Values are typically represented in
/// the token's base unit (e.g., wei for native tokens, smallest unit for HOPR).
#[derive(Debug, Clone, NewType)]
pub struct TokenValueString(pub String);

/// Status of a payment channel
#[derive(Enum, Copy, Clone, Eq, PartialEq, Debug)]
pub enum ChannelStatus {
    /// Channel is open and operational
    #[graphql(name = "OPEN")]
    Open,
    /// Channel is in the process of closing
    #[graphql(name = "PENDINGTOCLOSE")]
    PendingToClose,
    /// Channel has been closed
    #[graphql(name = "CLOSED")]
    Closed,
}

impl From<i8> for ChannelStatus {
    fn from(status: i8) -> Self {
        match status {
            0 => ChannelStatus::Closed,
            1 => ChannelStatus::Open,
            2 => ChannelStatus::PendingToClose,
            _ => ChannelStatus::Closed, // Default to closed for invalid values
        }
    }
}

/// Token type for balance queries
#[derive(Enum, Copy, Clone, Eq, PartialEq, Debug)]
pub enum Token {
    /// HOPR token
    #[graphql(name = "HOPR")]
    Hopr,
    /// Native token
    #[graphql(name = "NATIVE")]
    Native,
}

/// Balance information for subscriptions
#[derive(SimpleObject, Clone, Debug)]
pub struct Balance {
    /// Unique account on-chain address in hexadecimal format
    pub address: String,
    /// Token balance associated with the on-chain address
    pub value: f64,
    /// Type of token (HOPR or Native)
    pub token: Token,
}

/// Blockchain and HOPR network information
#[derive(SimpleObject, Clone, Debug)]
pub struct ChainInfo {
    /// Current block number of the blockchain
    #[graphql(name = "blockNumber")]
    pub block_number: i32,
    /// Chain ID of the connected blockchain network
    #[graphql(name = "chainId")]
    pub chain_id: i32,
    /// Current HOPR token price
    #[graphql(name = "ticketPrice")]
    pub ticket_price: TokenValueString,
    /// Current minimum ticket winning probability (decimal value between 0.0 and 1.0)
    #[graphql(name = "minTicketWinningProbability")]
    pub min_ticket_winning_probability: f64,
}

/// Account information containing balances and multiaddresses
#[derive(SimpleObject, Clone, Debug)]
pub struct Account {
    /// Unique identifier for the account
    pub keyid: i32,
    /// Unique account on-chain address in hexadecimal format
    #[graphql(name = "chainKey")]
    pub chain_key: String,
    /// Unique account packet key in peer id format
    #[graphql(name = "packetKey")]
    pub packet_key: String,
    /// wxHOPR balance associated with the on-chain address
    ///
    /// Returns zero balance if no balance record exists in the database.
    #[graphql(name = "accountHoprBalance")]
    pub account_hopr_balance: TokenValueString,
    /// Native balance associated with the on-chain address
    ///
    /// Returns zero balance if no balance record exists in the database.
    #[graphql(name = "accountNativeBalance")]
    pub account_native_balance: TokenValueString,
    /// HOPR Safe contract address to which the account is linked
    #[graphql(name = "safeAddress")]
    pub safe_address: Option<String>,
    /// wxHOPR balance associated with the linked Safe contract address
    #[graphql(name = "safeHoprBalance")]
    pub safe_hopr_balance: Option<TokenValueString>,
    /// Native balance associated with the linked Safe contract address
    #[graphql(name = "safeNativeBalance")]
    pub safe_native_balance: Option<TokenValueString>,
    /// List of multiaddresses associated with the packet key
    #[graphql(name = "multiAddresses")]
    pub multi_addresses: Vec<String>,
}

/// Network announcement with multiaddress information
#[derive(SimpleObject, Clone, Debug)]
pub struct Announcement {
    pub id: i32,
    pub account_id: i32,
    /// Multiaddress for the node
    pub multiaddress: String,
    /// Published block as hex string
    pub published_block: String,
}

/// Payment channel between two nodes
#[derive(SimpleObject, Clone, Debug)]
pub struct Channel {
    /// Unique identifier for the payment channel in hexadecimal format
    #[graphql(name = "concreteChannelId")]
    pub concrete_channel_id: String,
    /// Account keyid of the source node
    pub source: i32,
    /// Account keyid of the destination node
    pub destination: i32,
    /// Total amount of HOPR tokens allocated to the channel
    pub balance: TokenValueString,
    /// Current state of the channel (OPEN, PENDINGTOCLOSE, or CLOSED)
    pub status: ChannelStatus,
    /// Current epoch of the channel
    pub epoch: i32,
    /// Latest ticket index used in the channel
    #[graphql(name = "ticketIndex")]
    pub ticket_index: i32,
    /// Timestamp when the channel closure was initiated (null if no closure initiated)
    #[graphql(name = "closureTime")]
    pub closure_time: Option<chrono::DateTime<chrono::Utc>>,
}

/// Graph of opened payment channels with associated accounts
#[derive(SimpleObject, Clone, Debug)]
pub struct OpenedChannelsGraph {
    /// List of all open payment channels
    pub channels: Vec<Channel>,
    /// List of accounts referenced by the open channels (source and destination nodes)
    pub accounts: Vec<Account>,
}

/// HOPR token balance information for a specific address
#[derive(SimpleObject, Clone, Debug)]
pub struct HoprBalance {
    /// Address holding the HOPR token balance
    pub address: String,
    /// HOPR token balance
    pub balance: TokenValueString,
}

/// Native token balance information for a specific address
#[derive(SimpleObject, Clone, Debug)]
pub struct NativeBalance {
    /// Address holding the native token balance
    pub address: String,
    /// Native token balance
    pub balance: TokenValueString,
}
