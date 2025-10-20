//! GraphQL type definitions for HOPR blokli API

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
    #[graphql(name = "accountHoprBalance")]
    pub account_hopr_balance: TokenValueString,
    /// Native balance associated with the on-chain address
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

impl From<blokli_db_entity::announcement::Model> for Announcement {
    fn from(model: blokli_db_entity::announcement::Model) -> Self {
        Self {
            id: model.id,
            account_id: model.account_id,
            multiaddress: model.multiaddress,
            published_block: hex::encode(&model.published_block),
        }
    }
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
    /// Seconds until the channel is closed once closure is initiated
    #[graphql(name = "closureTime")]
    pub closure_time: i32,
}

impl From<blokli_db_entity::channel::Model> for Channel {
    fn from(model: blokli_db_entity::channel::Model) -> Self {
        use blokli_db_entity::conversions::balances::{bytes_to_u64, hopr_balance_to_string};

        let balance = TokenValueString(hopr_balance_to_string(&model.balance));
        let epoch = bytes_to_u64(&model.epoch) as i32;
        let ticket_index = bytes_to_u64(&model.ticket_index) as i32;

        // Convert closure_time to seconds until close (0 if None or already closed)
        let closure_time = model
            .closure_time
            .map(|ct| {
                let now = chrono::Utc::now();
                let diff = ct - now;
                diff.num_seconds().max(0) as i32
            })
            .unwrap_or(0);

        Self {
            concrete_channel_id: model.concrete_channel_id,
            source: model.source,
            destination: model.destination,
            balance,
            status: ChannelStatus::from(model.status),
            epoch,
            ticket_index,
            closure_time,
        }
    }
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

impl From<blokli_db_entity::hopr_balance::Model> for HoprBalance {
    fn from(model: blokli_db_entity::hopr_balance::Model) -> Self {
        use blokli_db_entity::conversions::balances::balance_to_string;

        Self {
            address: model.address,
            balance: TokenValueString(balance_to_string(&model.balance)),
        }
    }
}

/// Native token balance information for a specific address
#[derive(SimpleObject, Clone, Debug)]
pub struct NativeBalance {
    /// Address holding the native token balance
    pub address: String,
    /// Native token balance
    pub balance: TokenValueString,
}

impl From<blokli_db_entity::native_balance::Model> for NativeBalance {
    fn from(model: blokli_db_entity::native_balance::Model) -> Self {
        use blokli_db_entity::conversions::balances::balance_to_string;

        Self {
            address: model.address,
            balance: TokenValueString(balance_to_string(&model.balance)),
        }
    }
}
