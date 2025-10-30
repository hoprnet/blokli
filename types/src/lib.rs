//! GraphQL type definitions for HOPR blokli API
//!
//! This crate contains pure GraphQL type definitions that can be reused
//! by clients without depending on the full API server implementation.

use async_graphql::{Enum, InputValueError, NewType, Scalar, ScalarType, SimpleObject, Value};

/// Token value represented as a string to maintain precision
///
/// This scalar type represents token amounts as decimal strings to avoid
/// floating-point precision issues. Values are typically represented in
/// the token's base unit (e.g., wei for native tokens, smallest unit for HOPR).
#[derive(Debug, Clone, NewType)]
pub struct TokenValueString(pub String);

/// 32-byte hexadecimal string scalar type (with optional 0x prefix)
///
/// This scalar type represents 32-byte values as hexadecimal strings.
/// Accepts strings with or without "0x" prefix, validates length to be exactly 64 hex characters.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hex32(pub String);

#[Scalar]
impl ScalarType for Hex32 {
    fn parse(value: Value) -> async_graphql::InputValueResult<Self> {
        match value {
            Value::String(s) => {
                let hex_str = s.strip_prefix("0x").unwrap_or(&s);
                if hex_str.len() != 64 {
                    return Err(InputValueError::custom(format!(
                        "Hex32 must be 64 hex characters (got {})",
                        hex_str.len()
                    )));
                }
                if !hex_str.chars().all(|c| c.is_ascii_hexdigit()) {
                    return Err(InputValueError::custom("Hex32 must contain only hex characters"));
                }
                Ok(Hex32(s))
            }
            _ => Err(InputValueError::custom("Hex32 must be a string")),
        }
    }

    fn to_value(&self) -> Value {
        Value::String(self.0.clone())
    }
}

/// Unsigned 64-bit integer scalar type
///
/// This scalar type represents u64 values as strings in GraphQL to avoid
/// JavaScript's Number precision loss (JS Number is only safe up to 2^53-1).
/// The maximum value is 18,446,744,073,709,551,615.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UInt64(pub u64);

#[Scalar]
impl ScalarType for UInt64 {
    fn parse(value: Value) -> async_graphql::InputValueResult<Self> {
        match value {
            Value::String(s) => {
                let n = s.parse::<u64>().map_err(|e| format!("Invalid UInt64: {}", e))?;
                Ok(UInt64(n))
            }
            Value::Number(n) => {
                if let Some(n) = n.as_u64() {
                    Ok(UInt64(n))
                } else {
                    Err("UInt64 must be a positive integer".into())
                }
            }
            _ => Err("UInt64 must be a string or number".into()),
        }
    }

    fn to_value(&self) -> Value {
        Value::String(self.0.to_string())
    }
}

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
    /// Channel smart contract domain separator (hex string)
    #[graphql(name = "channelDst")]
    pub channel_dst: Option<Hex32>,
    /// Ledger smart contract domain separator (hex string)
    #[graphql(name = "ledgerDst")]
    pub ledger_dst: Option<Hex32>,
    /// Safe Registry smart contract domain separator (hex string)
    #[graphql(name = "safeRegistryDst")]
    pub safe_registry_dst: Option<Hex32>,
    /// Channel closure grace period in seconds
    #[graphql(name = "channelClosureGracePeriod")]
    pub channel_closure_grace_period: Option<u64>,
}

/// Account information
///
/// The Account type contains identity information for HOPR nodes including keys,
/// addresses, and network announcements. To query balances and allowances, use the
/// dedicated balance and allowance queries (hoprBalance, nativeBalance, safeHoprAllowance).
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
    /// HOPR Safe contract address to which the account is linked
    #[graphql(name = "safeAddress")]
    pub safe_address: Option<String>,
    /// List of multiaddresses associated with the packet key
    #[graphql(name = "multiAddresses")]
    pub multi_addresses: Vec<String>,
    /// HOPR Safe contract transaction count
    #[graphql(name = "safeTransactionCount")]
    pub safe_transaction_count: UInt64,
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
    /// Current epoch of the channel (uint24)
    pub epoch: i32,
    /// Latest ticket index used in the channel (uint48, max: 281474976710655)
    #[graphql(name = "ticketIndex")]
    pub ticket_index: UInt64,
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

/// Safe HOPR token allowance information for a specific Safe address
#[derive(SimpleObject, Clone, Debug)]
pub struct SafeHoprAllowance {
    /// Safe contract address
    pub address: String,
    /// wxHOPR token allowance granted by the safe to the channels contract
    pub allowance: TokenValueString,
}
