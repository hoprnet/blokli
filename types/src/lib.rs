//! GraphQL type definitions for HOPR blokli API
//!
//! This crate contains pure GraphQL type definitions that can be reused
//! by clients without depending on the full API server implementation.

use std::collections::HashMap;

mod tests;

use async_graphql::{Enum, ID, InputObject, InputValueError, Scalar, ScalarType, SimpleObject, Union, Value};
use hopr_crypto_types::types::Hash;
use hopr_primitive_types::prelude::ToHex;

/// Token value represented as a string to maintain precision
///
/// This scalar type represents token amounts as decimal strings to avoid
/// floating-point precision issues. Values are typically represented in
/// the token's base unit (e.g., wei for native tokens, smallest unit for HOPR).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TokenValueString(pub String);

#[Scalar]
impl ScalarType for TokenValueString {
    fn parse(value: Value) -> async_graphql::InputValueResult<Self> {
        match value {
            Value::String(s) => Ok(TokenValueString(s)),
            _ => Err(InputValueError::custom("TokenValueString must be a string")),
        }
    }

    fn to_value(&self) -> Value {
        Value::String(self.0.clone())
    }
}

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

impl From<&[u8; 32]> for Hex32 {
    fn from(bytes: &[u8; 32]) -> Self {
        Hex32(Hash::from(*bytes).to_hex())
    }
}

impl From<hopr_crypto_types::types::Hash> for Hex32 {
    fn from(hash: Hash) -> Self {
        Hex32(hash.to_hex())
    }
}

/// Unsigned 64-bit integer scalar type
///
/// This scalar type represents u64 values as strings in GraphQL to avoid
/// JavaScript's Number precision loss (JS Number is only safe up to 2^53-1).
/// The maximum value is 18,446,744,073,709,551,615.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UInt64(pub u64);

#[Scalar(name = "UInt64")]
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

/// Map of contract identifiers to contract addresses
///
/// This scalar type represents a mapping from contract identifier strings
/// (e.g., "token", "channels") to their deployed addresses in hexadecimal format.
/// Keys: token, channels, announcements, module_implementation, node_safe_migration, node_safe_registry,
/// ticket_price_oracle, winning_probability_oracle, node_stake_factory
///
/// Serialized as a stringified JSON object. For example:
/// `{"token":"0x123abc","channels":"0x456def","announcements":"0x789ghi"}`
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContractAddressMap(pub HashMap<String, String>);

#[Scalar]
impl ScalarType for ContractAddressMap {
    fn parse(value: Value) -> async_graphql::InputValueResult<Self> {
        match value {
            Value::String(json_str) => {
                // Parse the JSON string to extract the map
                let parsed: std::collections::BTreeMap<String, serde_json::Value> = serde_json::from_str(&json_str)
                    .map_err(|e| InputValueError::custom(format!("Invalid JSON string: {}", e)))?;

                let mut map = HashMap::new();
                for (key, val) in parsed {
                    if let Some(addr_str) = val.as_str() {
                        map.insert(key, addr_str.to_string());
                    } else {
                        return Err(InputValueError::custom("ContractAddressMap values must be strings"));
                    }
                }
                Ok(ContractAddressMap(map))
            }
            _ => Err(InputValueError::custom("ContractAddressMap must be a JSON string")),
        }
    }

    fn to_value(&self) -> Value {
        // Serialize the map to a JSON string
        let json = serde_json::to_string(&self.0).unwrap_or_else(|_| "{}".to_string());
        Value::String(json)
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

impl From<i16> for ChannelStatus {
    fn from(status: i16) -> Self {
        match status {
            0 => ChannelStatus::Closed,
            1 => ChannelStatus::Open,
            2 => ChannelStatus::PendingToClose,
            _ => ChannelStatus::Closed, // Default to closed for invalid values
        }
    }
}

impl From<ChannelStatus> for i16 {
    fn from(status: ChannelStatus) -> Self {
        match status {
            ChannelStatus::Closed => 0,
            ChannelStatus::Open => 1,
            ChannelStatus::PendingToClose => 2,
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
    /// Network name (e.g., 'dufour', 'rotsee', 'jura')
    pub network: String,
    /// Current HOPR token price
    #[graphql(name = "ticketPrice")]
    pub ticket_price: TokenValueString,
    /// Current key binding fee
    #[graphql(name = "keyBindingFee")]
    pub key_binding_fee: TokenValueString,
    /// Current minimum ticket winning probability (decimal value between 0.0 and 1.0)
    #[graphql(name = "minTicketWinningProbability")]
    pub min_ticket_winning_probability: f64,
    /// Channel smart contract domain separator (hex string)
    #[graphql(name = "channelDst")]
    pub channel_dst: Option<String>,
    /// Map of contract identifiers to their deployed addresses
    #[graphql(name = "contractAddresses")]
    pub contract_addresses: ContractAddressMap,
    /// Ledger smart contract domain separator (hex string)
    #[graphql(name = "ledgerDst")]
    pub ledger_dst: Option<String>,
    /// Safe Registry smart contract domain separator (hex string)
    #[graphql(name = "safeRegistryDst")]
    pub safe_registry_dst: Option<String>,
    /// Channel closure grace period in seconds
    #[graphql(name = "channelClosureGracePeriod")]
    pub channel_closure_grace_period: UInt64,
    /// Expected block time in seconds
    #[graphql(name = "expectedBlockTime")]
    pub expected_block_time: UInt64,
    /// Number of block confirmations required for finality
    #[graphql(name = "finality")]
    pub finality: UInt64,
}

/// Result type for chain info queries
#[derive(Union, Clone, Debug)]
pub enum ChainInfoResult {
    /// Successful chain info
    ChainInfo(ChainInfo),
    /// Query failed
    QueryFailed(QueryFailedError),
}

/// Account information
///
/// The Account type contains identity information for HOPR nodes including keys,
/// addresses, and network announcements. To query balances and allowances, use the
/// dedicated balance and allowance queries (hoprBalance, nativeBalance, safeHoprAllowance).
#[derive(SimpleObject, Clone, Debug, PartialEq)]
pub struct Account {
    /// Unique identifier for the account
    pub keyid: i64,
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
}

/// Success response for accounts list query
#[derive(SimpleObject, Clone, Debug)]
pub struct AccountsList {
    /// List of accounts
    pub accounts: Vec<Account>,
}

/// Result type for accounts list query
#[derive(Union, Clone, Debug)]
pub enum AccountsResult {
    /// Successful accounts list
    Accounts(AccountsList),
    /// Missing required filter parameter
    MissingFilter(MissingFilterError),
    /// Query failed
    QueryFailed(QueryFailedError),
}

/// Network announcement with multiaddress information
#[derive(SimpleObject, Clone, Debug)]
pub struct Announcement {
    pub id: i64,
    pub account_id: i64,
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
    pub source: i64,
    /// Account keyid of the destination node
    pub destination: i64,
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

/// Success response for channels list query
#[derive(SimpleObject, Clone, Debug)]
pub struct ChannelsList {
    /// List of channels
    pub channels: Vec<Channel>,
}

/// Result type for channels list query
#[derive(Union, Clone, Debug)]
pub enum ChannelsResult {
    /// Successful channels list
    Channels(ChannelsList),
    /// Missing required filter parameter
    MissingFilter(MissingFilterError),
    /// Query failed
    QueryFailed(QueryFailedError),
}

/// Count value for count queries
#[derive(SimpleObject, Clone, Debug)]
pub struct Count {
    /// Count value
    pub count: i32,
}

/// Result type for count queries
#[derive(Union, Clone, Debug)]
pub enum CountResult {
    /// Successful count
    Count(Count),
    /// Missing required filter parameter
    MissingFilter(MissingFilterError),
    /// Query failed
    QueryFailed(QueryFailedError),
}

/// Channel update event for subscriptions
///
/// Contains complete channel information along with source and destination account details.
/// Used in the openedChannelsGraphStream subscription to provide real-time updates.
#[derive(SimpleObject, Clone, Debug)]
pub struct ChannelUpdate {
    /// The updated channel
    pub channel: Channel,
    /// Source account of the channel
    pub source: Account,
    /// Destination account of the channel
    pub destination: Account,
}

/// A single edge in the opened payment channels graph
///
/// Represents one channel with its associated source and destination accounts.
/// This is a directed edge: source → destination. If channels exist in both
/// directions (A→B and B→A), these are emitted as separate entries.
///
/// **Structure:**
/// - Each entry contains exactly one channel with its source and destination accounts
/// - If multiple channels exist between the same account pair, each is emitted as a separate entry
/// - The channel is always open (closed channels are not included)
///
/// **Usage in subscriptions:**
/// The `openedChannelGraphUpdated` subscription streams these entries one at a time.
/// Clients must accumulate entries to build the complete channel graph.
/// An entry is emitted whenever that specific channel is updated.
#[derive(SimpleObject, Clone, Debug)]
pub struct OpenedChannelsGraphEntry {
    /// The open payment channel from source to destination
    pub channel: Channel,
    /// Source account (sender end of the directed edge)
    pub source: Account,
    /// Destination account (recipient end of the directed edge)
    pub destination: Account,
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

// ========================================
// Transaction Submission Types
// ========================================

/// Status of a submitted transaction
#[derive(Enum, Copy, Clone, Eq, PartialEq, Debug)]
pub enum TransactionStatus {
    /// Transaction is pending submission to the chain
    #[graphql(name = "PENDING")]
    Pending,
    /// Transaction has been submitted and is awaiting confirmation
    #[graphql(name = "SUBMITTED")]
    Submitted,
    /// Transaction has been confirmed on-chain with success
    #[graphql(name = "CONFIRMED")]
    Confirmed,
    /// Transaction was included on-chain but reverted (receipt.status = 0)
    #[graphql(name = "REVERTED")]
    Reverted,
    /// Transaction was not mined within timeout window
    #[graphql(name = "TIMEOUT")]
    Timeout,
    /// Transaction validation failed
    #[graphql(name = "VALIDATION_FAILED")]
    ValidationFailed,
    /// Transaction submission failed
    #[graphql(name = "SUBMISSION_FAILED")]
    SubmissionFailed,
}

/// Input for transaction submission
#[derive(InputObject, Clone, Debug)]
pub struct TransactionInput {
    /// Raw signed transaction data in hexadecimal format (with or without 0x prefix)
    #[graphql(name = "rawTransaction")]
    pub raw_transaction: String,
}

/// Internal Safe contract execution result
#[derive(SimpleObject, Clone, Debug)]
pub struct SafeExecution {
    /// Whether the internal Safe transaction succeeded
    pub success: bool,
    /// Safe internal transaction hash (bytes32 hex).
    /// Null if the event data was malformed and the hash could not be extracted.
    #[graphql(name = "safeTxHash")]
    pub safe_tx_hash: Option<Hex32>,
    /// Revert reason (if execution failed and reason is decodable)
    #[graphql(name = "revertReason")]
    pub revert_reason: Option<String>,
}

/// Transaction submission result
#[derive(SimpleObject, Clone, Debug)]
pub struct Transaction {
    /// Unique identifier for the transaction (UUID)
    pub id: ID,
    /// Current status of the transaction
    pub status: TransactionStatus,
    /// Timestamp when transaction was submitted
    #[graphql(name = "submittedAt")]
    pub submitted_at: chrono::DateTime<chrono::Utc>,
    /// Transaction hash from successful blockchain submission
    #[graphql(name = "transactionHash")]
    pub transaction_hash: Hex32,
    /// Internal Safe execution result (null for non-Safe transactions or before confirmation)
    #[graphql(name = "safeExecution")]
    pub safe_execution: Option<SafeExecution>,
}

/// Success response for fire-and-forget transaction submission
#[derive(SimpleObject, Clone, Debug)]
pub struct SendTransactionSuccess {
    /// Transaction hash after successful submission
    #[graphql(name = "transactionHash")]
    pub transaction_hash: Hex32,
}

// ========================================
// Transaction Error Types
// ========================================

/// RPC or blockchain error during transaction submission
#[derive(SimpleObject, Clone, Debug)]
pub struct RpcError {
    /// Error code
    pub code: String,
    /// Human-readable error message
    pub message: String,
}

/// Target contract not in allowlist
#[derive(SimpleObject, Clone, Debug)]
pub struct ContractNotAllowedError {
    /// Error code
    pub code: String,
    /// Human-readable error message
    pub message: String,
    /// Contract address that was rejected
    #[graphql(name = "contractAddress")]
    pub contract_address: String,
}

/// Function selector not allowed
#[derive(SimpleObject, Clone, Debug)]
pub struct FunctionNotAllowedError {
    /// Error code
    pub code: String,
    /// Human-readable error message
    pub message: String,
    /// Contract address
    #[graphql(name = "contractAddress")]
    pub contract_address: String,
    /// Function selector that was rejected
    #[graphql(name = "functionSelector")]
    pub function_selector: String,
}

/// Operation timed out
#[derive(SimpleObject, Clone, Debug)]
pub struct TimeoutError {
    /// Error code
    pub code: String,
    /// Human-readable error message
    pub message: String,
}

/// Transaction ID format is invalid
#[derive(SimpleObject, Clone, Debug)]
pub struct InvalidTransactionIdError {
    /// Error code
    pub code: String,
    /// Human-readable error message
    pub message: String,
    /// The invalid transaction ID that was provided
    #[graphql(name = "transactionId")]
    pub transaction_id: String,
}

/// Address format is invalid
#[derive(SimpleObject, Clone, Debug)]
pub struct InvalidAddressError {
    /// Error code
    pub code: String,
    /// Human-readable error message
    pub message: String,
    /// The invalid address that was provided
    pub address: String,
}

/// Database or internal query error
#[derive(SimpleObject, Clone, Debug)]
pub struct QueryFailedError {
    /// Error code
    pub code: String,
    /// Human-readable error message
    pub message: String,
}

/// Missing required filter parameter error
#[derive(SimpleObject, Clone, Debug)]
pub struct MissingFilterError {
    /// Error code
    pub code: String,
    /// Human-readable error message
    pub message: String,
}

/// Safe HOPR token allowance information for a specific Safe address
#[derive(SimpleObject, Clone, Debug)]
pub struct SafeHoprAllowance {
    /// Safe contract address
    pub address: String,
    /// wxHOPR token allowance granted by the safe to the channels contract
    pub allowance: TokenValueString,
}

/// Transaction count information for any Ethereum address
///
/// For EOAs (Externally Owned Accounts): Returns the transaction count via eth_getTransactionCount
/// For Safe contracts: Returns the internal nonce via nonce() function
/// For other contracts: Attempts nonce() call, falls back to eth_getTransactionCount
#[derive(SimpleObject, Clone, Debug)]
pub struct TransactionCount {
    /// Address queried (hexadecimal format)
    pub address: String,
    /// Current transaction count or nonce for the address
    pub count: UInt64,
}

/// HOPR Safe contract deployment information
#[derive(SimpleObject, Clone, Debug)]
pub struct Safe {
    /// Safe contract address (hexadecimal format)
    pub address: String,
    /// HOPR Node Management Module address (hexadecimal format)
    #[graphql(name = "moduleAddress")]
    pub module_address: String,
    /// Chain key (owner address, hexadecimal format)
    #[graphql(name = "chainKey")]
    pub chain_key: String,
    /// List of node addresses (chain keys) registered to this safe via RegisteredNodeSafe events
    #[graphql(name = "registeredNodes")]
    pub registered_nodes: Vec<String>,
}

/// Calculated module address
#[derive(SimpleObject, Clone, Debug)]
pub struct ModuleAddress {
    /// Predicted module address (hexadecimal format)
    #[graphql(name = "moduleAddress")]
    pub module_address: String,
}

/// Ticket price and winning probability parameters
#[derive(SimpleObject, Clone, Debug, PartialEq)]
pub struct TicketParameters {
    /// Current minimum ticket winning probability (decimal value between 0.0 and 1.0)
    #[graphql(name = "minTicketWinningProbability")]
    pub min_ticket_winning_probability: f64,
    /// Current HOPR token price
    #[graphql(name = "ticketPrice")]
    pub ticket_price: TokenValueString,
}

impl From<&blokli_chain_types::ContractAddresses> for ContractAddressMap {
    fn from(addresses: &blokli_chain_types::ContractAddresses) -> Self {
        let map = [
            ("token", &addresses.token),
            ("channels", &addresses.channels),
            ("announcements", &addresses.announcements),
            ("module_implementation", &addresses.module_implementation),
            ("node_safe_migration", &addresses.node_safe_migration),
            ("node_safe_registry", &addresses.node_safe_registry),
            ("ticket_price_oracle", &addresses.ticket_price_oracle),
            ("winning_probability_oracle", &addresses.winning_probability_oracle),
            ("node_stake_factory", &addresses.node_stake_factory),
        ]
        .into_iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();

        ContractAddressMap(map)
    }
}
