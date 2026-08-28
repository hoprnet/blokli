//! GraphQL type definitions for HOPR blokli API
//!
//! This crate contains pure GraphQL type definitions that can be reused
//! by clients without depending on the full API server implementation.

use std::collections::HashMap;

mod tests;

pub use async_graphql::ID;
use async_graphql::{Enum, InputObject, InputValueError, Scalar, ScalarType, SimpleObject, Union, Value};
use hopr_types::{crypto::types::Hash, primitive::prelude::ToHex};
use serde::Serialize;

/// Token value represented as a string to maintain precision
///
/// This scalar type represents token amounts as decimal strings to avoid
/// floating-point precision issues. Values are typically represented in
/// the token's base unit (e.g., wei for native tokens, smallest unit for HOPR).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
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

impl From<hopr_types::crypto::types::Hash> for Hex32 {
    fn from(hash: Hash) -> Self {
        Hex32(hash.to_hex())
    }
}

/// Unsigned 64-bit integer scalar type
///
/// This scalar type represents u64 values as strings in GraphQL to avoid
/// JavaScript's Number precision loss (JS Number is only safe up to 2^53-1).
/// The maximum value is 18,446,744,073,709,551,615.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
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
/// ticket_price_oracle, winning_probability_oracle, node_stake_factory, xhopr_token, service_registry
///
/// The keys match the field names of `hopr_types::chain::ContractAddresses`, because consumers
/// deserialize this map straight into that struct. Dropping a key breaks them at runtime, not at
/// compile time.
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
    /// wxHOPR token
    #[graphql(name = "HOPR")]
    WxHOPR,
    /// xHOPR token
    #[graphql(name = "XHOPR")]
    XHOPR,
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
    /// Type of token ((w)xHOPR or Native)
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
    /// Network name (e.g., 'jura-dev', 'jura-prod')
    pub network: String,
    /// Current HOPR token price
    #[graphql(name = "ticketPrice")]
    pub ticket_price: TokenValueString,
    /// Current key binding fee
    #[graphql(name = "keyBindingFee")]
    pub key_binding_fee: TokenValueString,
    /// Estimated legacy gas price in wei from RPC
    #[graphql(name = "gasPrice")]
    pub gas_price: Option<String>,
    /// Estimated EIP-1559 max fee per gas in wei from RPC, scaled by api.gas_multiplier
    #[graphql(name = "maxFeePerGas")]
    pub max_fee_per_gas: Option<String>,
    /// Estimated EIP-1559 max priority fee per gas in wei from RPC, scaled by api.gas_multiplier
    #[graphql(name = "maxPriorityFeePerGas")]
    pub max_priority_fee_per_gas: Option<String>,
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
    ChainInfo(Box<ChainInfo>),
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
    /// Latest announced multiaddress for the packet key, returned as an empty or single-element list
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

/// A single entry in the on-chain service registry: one node offering one service type
///
/// The registry treats the metadata as opaque bytes: its schema belongs to the service type, not
/// to the registry, so it is exposed as hex rather than parsed.
#[derive(SimpleObject, Clone, Debug, PartialEq, Serialize)]
pub struct ServiceEntry {
    /// Service type identifier - ASCII name, or 0x-prefixed hex when the id is not printable ASCII
    #[graphql(name = "serviceType")]
    pub service_type: String,
    /// Chain address of the node offering the service in hexadecimal format
    pub node: String,
    /// Safe that performed the last write to this entry, in hexadecimal format
    pub safe: String,
    /// Opaque metadata as 0x-prefixed hex
    pub metadata: String,
    /// Unix timestamp in seconds at which the entry was registered
    ///
    /// A `UInt64` rather than an `Int`: the on-chain source is a `uint48`, which a signed 32-bit
    /// GraphQL `Int` cannot carry past 2038.
    #[graphql(name = "registeredAt")]
    pub registered_at: UInt64,
    /// Unix timestamp in seconds at which the entry was last updated
    #[graphql(name = "updatedAt")]
    pub updated_at: UInt64,
}

/// Configuration of a single service type
#[derive(SimpleObject, Clone, Debug, PartialEq, Serialize)]
pub struct ServiceTypeInfo {
    /// Service type identifier - ASCII name, or 0x-prefixed hex
    #[graphql(name = "serviceType")]
    pub service_type: String,
    /// Owner of the type; null once the type has been abandoned, which is one-way
    pub owner: Option<String>,
    /// Requirement contract gating registration; null for an open type
    pub requirement: Option<String>,
    /// wxHOPR burned on self-registration
    #[graphql(name = "registrationBurn")]
    pub registration_burn: TokenValueString,
    /// wxHOPR burned on self-update
    #[graphql(name = "updateBurn")]
    pub update_burn: TokenValueString,
}

/// Registry-wide configuration, shared by every service type
#[derive(SimpleObject, Clone, Debug, PartialEq, Serialize)]
pub struct ServiceRegistryConfig {
    /// wxHOPR burned to register a new service type
    #[graphql(name = "typeRegistrationFee")]
    pub type_registration_fee: TokenValueString,
    /// Node-safe registry the service registry resolves node bindings against, in hexadecimal format
    #[graphql(name = "nodeSafeRegistry")]
    pub node_safe_registry: String,
}

/// Kind of change to a single registry entry
#[derive(Enum, Copy, Clone, Eq, PartialEq, Debug, Serialize)]
pub enum ServiceUpdateKind {
    /// The entry was created
    #[graphql(name = "REGISTERED")]
    Registered,
    /// An existing entry changed
    #[graphql(name = "UPDATED")]
    Updated,
    /// The entry was removed
    #[graphql(name = "DEREGISTERED")]
    Deregistered,
}

/// Kind of change to service-type or registry-wide configuration
#[derive(Enum, Copy, Clone, Eq, PartialEq, Debug, Serialize)]
pub enum ServiceTypeUpdateKind {
    /// A new service type was registered
    #[graphql(name = "REGISTERED")]
    Registered,
    /// Type ownership moved, or the type was abandoned
    #[graphql(name = "OWNER_CHANGED")]
    OwnerChanged,
    /// The requirement contract gating the type changed
    #[graphql(name = "REQUIREMENT_CHANGED")]
    RequirementChanged,
    /// The self-registration burn of the type changed
    #[graphql(name = "REGISTRATION_BURN_CHANGED")]
    RegistrationBurnChanged,
    /// The self-update burn of the type changed
    #[graphql(name = "UPDATE_BURN_CHANGED")]
    UpdateBurnChanged,
    /// The registry-wide type registration fee changed
    #[graphql(name = "REGISTRATION_FEE_CHANGED")]
    RegistrationFeeChanged,
    /// The node-safe registry the service registry points at changed
    #[graphql(name = "REGISTRY_POINTER_CHANGED")]
    RegistryPointerChanged,
}

/// A change to one registry entry
///
/// The kind is explicit rather than inferred from the timestamps: registration sets `updatedAt`
/// to `registeredAt`, and so does an update landing in the registration block, so the two are
/// not distinguishable from the entry alone. Deregistration carries no entry at all.
#[derive(SimpleObject, Clone, Debug, PartialEq, Serialize)]
pub struct ServiceUpdate {
    /// What happened to the entry
    pub kind: ServiceUpdateKind,
    /// Service type the entry belongs to
    #[graphql(name = "serviceType")]
    pub service_type: String,
    /// Node the entry belongs to, in hexadecimal format
    pub node: String,
    /// Entry state after the change; null for `DEREGISTERED`, where the entry no longer exists
    pub entry: Option<ServiceEntry>,
}

/// A change to service-type or registry-wide configuration
#[derive(SimpleObject, Clone, Debug, PartialEq, Serialize)]
pub struct ServiceTypeUpdate {
    /// What changed
    pub kind: ServiceTypeUpdateKind,
    /// Service type affected; null for the two registry-wide kinds
    #[graphql(name = "serviceType")]
    pub service_type: Option<String>,
    /// Type configuration after the change; null for the two registry-wide kinds
    pub config: Option<ServiceTypeInfo>,
    /// Registry-wide configuration after the change; null for the five per-type kinds
    #[graphql(name = "registryConfig")]
    pub registry_config: Option<ServiceRegistryConfig>,
}

/// Success response for the services query
#[derive(SimpleObject, Clone, Debug)]
pub struct ServicesList {
    /// Matching registry entries
    pub services: Vec<ServiceEntry>,
    /// Fully indexed block at which this page is evaluated. Pass it unchanged for later pages.
    pub watermark: UInt64,
    /// Immutable service-entry id after which the next page starts, or null at the end.
    #[graphql(name = "nextCursor")]
    pub next_cursor: Option<UInt64>,
}

/// Success response for the serviceTypes query
#[derive(SimpleObject, Clone, Debug)]
pub struct ServiceTypesList {
    /// Matching service types
    #[graphql(name = "serviceTypes")]
    pub service_types: Vec<ServiceTypeInfo>,
}

/// Result type for the services list query
#[derive(Union, Clone, Debug)]
pub enum ServicesResult {
    /// Successful services list
    Services(ServicesList),
    /// Missing required filter parameter
    MissingFilter(MissingFilterError),
    /// Query failed
    QueryFailed(QueryFailedError),
}

/// Result type for the registry-wide configuration query
#[derive(Union, Clone, Debug)]
pub enum ServiceRegistryConfigResult {
    /// The current registry-wide configuration
    Config(ServiceRegistryConfig),
    /// Query failed
    QueryFailed(QueryFailedError),
}

/// Result type for the service types query
#[derive(Union, Clone, Debug)]
pub enum ServiceTypesResult {
    /// Successful service type list
    ServiceTypes(ServiceTypesList),
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
    /// Address format is invalid
    InvalidAddress(InvalidAddressError),
    /// Missing required filter parameter
    MissingFilter(MissingFilterError),
    /// Query failed
    QueryFailed(QueryFailedError),
}

/// Aggregated wxHOPR holdings across all or a filtered subset of indexed safe contracts
#[derive(SimpleObject, Clone, Debug)]
pub struct SafesBalance {
    /// Sum of wxHOPR balances for all safe contract addresses
    pub balance: TokenValueString,
    /// Number of safes included
    pub count: i32,
}

/// Result type for total safe wxHOPR balance query
#[derive(Union, Clone, Debug)]
pub enum SafesBalanceResult {
    /// Invalid owner address
    InvalidAddress(InvalidAddressError),
    /// Query failed
    QueryFailed(QueryFailedError),
    /// Successful total safe balance
    SafesBalance(SafesBalance),
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

/// Aggregated channel statistics: count and total balance
#[derive(SimpleObject, Clone, Debug)]
pub struct ChannelStats {
    /// Number of channels matching the filters
    pub count: i32,
    /// Total wxHOPR balance across all matching channels
    pub balance: TokenValueString,
}

/// Result type for channel statistics query
#[derive(Union, Clone, Debug)]
pub enum ChannelStatsResult {
    /// Successful channel statistics
    ChannelStats(ChannelStats),
    /// Address format is invalid
    InvalidAddress(InvalidAddressError),
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
    /// Transactions are never emitted in this state; they go directly to Submitted.
    #[graphql(
        name = "PENDING",
        deprecation = "Transactions go directly to SUBMITTED. This variant exists only for backwards compatibility \
                       and will be removed in a future release."
    )]
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

/// Internal Safe contract execution result.
///
/// This is supplementary to [`TransactionStatus`]: the `status` field on [`Transaction`] is
/// the authoritative terminal outcome (e.g. `Confirmed` means the outer on-chain tx succeeded).
/// When `safe_execution` is present, it describes the *internal* Safe module call outcome,
/// which can differ from the outer tx status — a `Confirmed` transaction may still have
/// `safe_execution.success == false` if the internal call reverted.
#[derive(SimpleObject, Clone, Debug)]
pub struct SafeExecution {
    /// Whether the internal Safe transaction succeeded
    pub success: bool,
    /// Safe internal transaction hash (bytes32 hex).
    /// Null for module-executed transactions (`execTransactionFromModule`) which do not
    /// emit a txHash, or if the event data was malformed and the hash could not be extracted.
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

/// Filter for ticket redemption stats queries.
///
/// At least one field must be provided. Providing both fields restricts the result
/// to the single matching safe/node pair; providing only one aggregates all rows
/// for that address.
#[derive(InputObject, Clone, Debug, Default)]
pub struct RedeemedStatsFilter {
    /// Safe contract address to filter by (hexadecimal format)
    #[graphql(name = "safeAddress")]
    pub safe_address: Option<String>,
    /// Destination node address to filter by (hexadecimal format)
    #[graphql(name = "nodeAddress")]
    pub node_address: Option<String>,
}

/// Outcome of a ticket redemption attempt.
///
/// Carried in [`RedeemTicketDetails`] to allow subscribers to distinguish
/// successful on-chain redemptions from inner Safe transaction failures
/// (rejected) without polling the chain.
#[derive(Enum, Copy, Clone, Eq, PartialEq, Debug)]
pub enum RedemptionResult {
    /// Ticket was successfully redeemed on-chain.
    Redeemed,
    /// Ticket redemption was rejected (inner Safe transaction failed).
    Rejected,
}

/// GraphQL output type for a ticket redemption event.
///
/// Uniquely identifies the ticket (`issuerAddress` + `recipientAddress` +
/// `epoch` + `index`) and reports whether it was accepted or rejected.
///
/// Returned by the `ticketRedeemed` subscription.
#[derive(SimpleObject, Clone, Debug)]
pub struct RedeemTicketDetails {
    /// Issuer account on-chain address in hexadecimal format
    #[graphql(name = "issuerAddress")]
    pub issuer_address: String,
    /// Recipient account on-chain address in hexadecimal format
    #[graphql(name = "recipientAddress")]
    pub recipient_address: String,
    /// Epoch of the channel where the ticket was redeemed
    pub epoch: UInt64,
    /// Index of the ticket within the channel epoch
    pub index: UInt64,
    /// Outcome of the redemption attempt
    pub result: RedemptionResult,
}

/// Unsigned 256-bit integer represented as a decimal string.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UInt256(pub String);

#[Scalar(name = "UInt256")]
impl ScalarType for UInt256 {
    fn parse(value: Value) -> async_graphql::InputValueResult<Self> {
        let value = match value {
            Value::String(value) => value,
            Value::Number(value) => value.to_string(),
            _ => return Err("UInt256 must be a decimal string or non-negative integer".into()),
        };
        if value.is_empty() {
            return Err("UInt256 must not be empty".into());
        }
        let normalized = value.trim_start_matches('0');
        let normalized = if normalized.is_empty() { "0" } else { normalized };
        const MAX_U256: &str = "115792089237316195423570985008687907853269984665640564039457584007913129639935";
        if !normalized.bytes().all(|byte| byte.is_ascii_digit())
            || normalized.len() > MAX_U256.len()
            || (normalized.len() == MAX_U256.len() && normalized > MAX_U256)
        {
            return Err("UInt256 must be a decimal integer between 0 and 2^256 - 1".into());
        }
        Ok(Self(normalized.to_string()))
    }

    fn to_value(&self) -> Value {
        Value::String(self.0.clone())
    }
}

/// Exclusive pagination cursor for indexed Curvy events.
#[derive(InputObject, Clone, Debug, PartialEq, Eq)]
pub struct CurvyEventCursor {
    /// Block number containing the event.
    pub block: UInt64,
    /// Zero-based transaction index inside the block.
    #[graphql(name = "transactionIndex")]
    pub transaction_index: UInt64,
    /// Zero-based log index inside the transaction receipt.
    #[graphql(name = "logIndex")]
    pub log_index: UInt64,
    /// Zero-based position of the item inside the event array.
    #[graphql(name = "eventItemIndex")]
    pub event_item_index: UInt64,
    /// Hash of the block containing the event, when known.
    #[graphql(name = "blockHash")]
    pub block_hash: Option<Hex32>,
}

/// Position and transaction identity shared by indexed Curvy events.
#[derive(SimpleObject, Clone, Debug)]
pub struct CurvyEventPosition {
    /// Hash of the transaction that emitted the event.
    #[graphql(name = "transactionHash")]
    pub transaction_hash: Hex32,
    /// Hash of the block containing the event.
    #[graphql(name = "blockHash")]
    pub block_hash: Hex32,
    /// Block number containing the event.
    pub block: UInt64,
    /// Zero-based transaction index inside the block.
    #[graphql(name = "transactionIndex")]
    pub transaction_index: UInt64,
    /// Zero-based log index inside the transaction receipt.
    #[graphql(name = "logIndex")]
    pub log_index: UInt64,
    /// Zero-based position of the item inside the event array.
    #[graphql(name = "eventItemIndex")]
    pub event_item_index: UInt64,
}

/// One note emitted by `PendingNotes`.
#[derive(SimpleObject, Clone, Debug)]
pub struct CurvyPendingNote {
    /// Pending note identifier.
    #[graphql(name = "noteId")]
    pub note_id: Hex32,
    /// Baby Jubjub ephemeral public key coordinates.
    #[graphql(name = "ephemeralKey")]
    pub ephemeral_key: Vec<UInt256>,
    /// View tag used for local ownership detection.
    #[graphql(name = "viewTag")]
    pub view_tag: i32,
    /// Vault token identifier.
    #[graphql(name = "tokenId")]
    pub token_id: UInt256,
    /// Raw note amount.
    pub amount: UInt256,
    /// Whether the note payload is plaintext.
    #[graphql(name = "isPlaintext")]
    pub is_plaintext: bool,
    /// Chain position of the array item that emitted this note.
    pub position: CurvyEventPosition,
}

/// One note emitted by `CommittedNotes`.
#[derive(SimpleObject, Clone, Debug)]
pub struct CurvyCommittedNote {
    /// Commitment batch index as a fixed-width 32-byte value.
    #[graphql(name = "batchIndex")]
    pub batch_index: Hex32,
    /// Committed note identifier.
    #[graphql(name = "noteId")]
    pub note_id: Hex32,
    /// Dense zero-based position in the notes tree.
    #[graphql(name = "leafIndex")]
    pub leaf_index: UInt64,
    /// Chain position of the array item that emitted this note.
    pub position: CurvyEventPosition,
}

/// One nullifier emitted by `CommittedNullifiers`.
#[derive(SimpleObject, Clone, Debug)]
pub struct CurvyCommittedNullifier {
    /// Nullifier batch index as a fixed-width 32-byte value.
    #[graphql(name = "batchIndex")]
    pub batch_index: Hex32,
    /// Committed nullifier value.
    pub nullifier: Hex32,
    /// Dense zero-based position in the nullifier sequence.
    #[graphql(name = "nullifierIndex")]
    pub nullifier_index: UInt64,
    /// Chain position of the array item that emitted this nullifier.
    pub position: CurvyEventPosition,
}

/// Finalized, immutable Curvy synchronization checkpoint.
#[derive(SimpleObject, Clone, Debug)]
pub struct CurvySyncCheckpoint {
    /// Number of the finalized checkpoint block.
    #[graphql(name = "blockNumber")]
    pub block_number: UInt64,
    /// Hash of the finalized checkpoint block.
    #[graphql(name = "blockHash")]
    pub block_hash: Hex32,
    /// Address of the indexed Curvy Aggregator.
    #[graphql(name = "aggregatorAddress")]
    pub aggregator_address: String,
    /// Version of the persisted notes-tree representation.
    #[graphql(name = "treeVersion")]
    pub tree_version: i32,
    /// Depth of the Curvy notes tree.
    #[graphql(name = "treeDepth")]
    pub tree_depth: i32,
    /// Height of each persisted notes-tree shard.
    #[graphql(name = "shardHeight")]
    pub shard_height: i32,
    /// Number of leaves in each notes-tree shard.
    #[graphql(name = "shardSize")]
    pub shard_size: UInt64,
    /// Number of indexed non-padding notes.
    #[graphql(name = "noteCount")]
    pub note_count: UInt64,
    /// Number of indexed non-padding nullifiers.
    #[graphql(name = "nullifierCount")]
    pub nullifier_count: UInt64,
    /// Number of completed notes-tree shards.
    #[graphql(name = "shardCount")]
    pub shard_count: UInt64,
    /// Notes-tree root at the checkpoint.
    #[graphql(name = "notesRoot")]
    pub notes_root: Hex32,
}

/// Committed note plus its optional announcement metadata for SDK synchronization.
#[derive(SimpleObject, Clone, Debug)]
pub struct CurvySyncNote {
    /// Dense zero-based position in the notes tree.
    #[graphql(name = "leafIndex")]
    pub leaf_index: UInt64,
    /// Committed note identifier.
    #[graphql(name = "noteId")]
    pub note_id: Hex32,
    /// Commitment batch index.
    #[graphql(name = "batchIndex")]
    pub batch_index: Hex32,
    /// Matching pending-note announcement, when indexed.
    pub announcement: Option<CurvyPendingNote>,
    /// Chain position at which the note was committed.
    #[graphql(name = "commitPosition")]
    pub commit_position: CurvyEventPosition,
}

/// One completed Curvy notes-tree shard.
#[derive(SimpleObject, Clone, Debug)]
pub struct CurvyShardRoot {
    /// Dense zero-based shard index.
    #[graphql(name = "shardIndex")]
    pub shard_index: UInt64,
    /// Root of the completed shard.
    pub root: Hex32,
    /// Chain position at which the shard became complete.
    #[graphql(name = "completionPosition")]
    pub completion_position: CurvyEventPosition,
}

/// Checkpoint-pinned page of dense Curvy committed notes.
#[derive(SimpleObject, Clone, Debug)]
pub struct CurvySyncNotePage {
    /// Block hash identifying the synchronization checkpoint.
    pub checkpoint: Hex32,
    /// Committed notes in this page.
    pub notes: Vec<CurvySyncNote>,
    /// Dense index from which the next page starts.
    #[graphql(name = "nextIndex")]
    pub next_index: UInt64,
    /// Total number of notes at the checkpoint.
    pub total: UInt64,
}

/// Checkpoint-pinned page of dense Curvy nullifiers.
#[derive(SimpleObject, Clone, Debug)]
pub struct CurvySyncNullifierPage {
    /// Block hash identifying the synchronization checkpoint.
    pub checkpoint: Hex32,
    /// Committed nullifiers in this page.
    pub nullifiers: Vec<CurvyCommittedNullifier>,
    /// Dense index from which the next page starts.
    #[graphql(name = "nextIndex")]
    pub next_index: UInt64,
    /// Total number of nullifiers at the checkpoint.
    pub total: UInt64,
}

/// Checkpoint-pinned page of completed Curvy shard roots.
#[derive(SimpleObject, Clone, Debug)]
pub struct CurvyShardRootPage {
    /// Block hash identifying the synchronization checkpoint.
    pub checkpoint: Hex32,
    /// Completed shard roots in this page.
    #[graphql(name = "shardRoots")]
    pub shard_roots: Vec<CurvyShardRoot>,
    /// Dense index from which the next page starts.
    #[graphql(name = "nextIndex")]
    pub next_index: UInt64,
    /// Total number of completed shards at the checkpoint.
    pub total: UInt64,
}

/// Current per-token gas fees read from the Curvy Vault.
#[derive(SimpleObject, Clone, Debug)]
pub struct CurvyGasFees {
    /// Identifier of the configured vault token.
    #[graphql(name = "tokenId")]
    pub token_id: UInt256,
    /// Gas fee charged when deploying a portal.
    #[graphql(name = "portalDeployment")]
    pub portal_deployment: UInt256,
    /// Gas fee charged when committing a pending note.
    #[graphql(name = "pendingNoteCommitment")]
    pub pending_note_commitment: UInt256,
    /// Gas fee charged when withdrawing a note.
    pub withdrawal: UInt256,
}

/// Current Curvy Aggregator indices and notes-tree root.
#[derive(SimpleObject, Clone, Debug)]
pub struct CurvyAggregatorState {
    /// Current notes-tree root.
    #[graphql(name = "notesTreeRoot")]
    pub notes_tree_root: Hex32,
    /// Current committed-notes batch index.
    #[graphql(name = "notesBatchIndex")]
    pub notes_batch_index: UInt256,
    /// Current committed-nullifiers batch index.
    #[graphql(name = "nullifiersBatchIndex")]
    pub nullifiers_batch_index: UInt256,
    /// Number of non-padding notes committed to the notes tree.
    #[graphql(name = "noteIndex")]
    pub note_index: UInt256,
}

/// Current Curvy Vault protocol-level fees.
#[derive(SimpleObject, Clone, Debug)]
pub struct CurvyVaultFees {
    /// Protocol fee charged on deposits.
    #[graphql(name = "depositFee")]
    pub deposit_fee: UInt256,
    /// Protocol fee charged on withdrawals.
    #[graphql(name = "withdrawalFee")]
    pub withdrawal_fee: UInt256,
}

/// Curvy Aggregator fee configuration needed to build a valid aggregation proof.
#[derive(SimpleObject, Clone, Debug)]
pub struct CurvyAggregatorFees {
    /// Protocol fee charged per thousand units.
    #[graphql(name = "protocolFeePerThousand")]
    pub protocol_fee_per_thousand: UInt256,
    /// Root of the commitment gas-fee tree.
    #[graphql(name = "commitmentFeeRoot")]
    pub commitment_fee_root: Hex32,
    /// Baby Jubjub public key that owns protocol fee notes.
    #[graphql(name = "feeNotePublicKey")]
    pub fee_note_public_key: Vec<UInt256>,
}

/// The number of tokens registered in the Curvy Vault.
#[derive(SimpleObject, Clone, Debug)]
pub struct CurvyVaultTokenCount {
    /// Number of registered vault tokens.
    pub count: UInt256,
}

/// A Curvy Vault token and its configured gas fees.
#[derive(SimpleObject, Clone, Debug)]
pub struct CurvyVaultToken {
    /// ERC-20 token contract address.
    #[graphql(name = "tokenAddress")]
    pub token_address: String,
    /// Gas fees configured for the token.
    #[graphql(name = "gasFees")]
    pub gas_fees: CurvyGasFees,
}

/// Collection of indexed Curvy pending notes.
#[derive(SimpleObject, Clone, Debug)]
pub struct CurvyPendingNotes {
    /// Pending notes ordered by chain position.
    pub notes: Vec<CurvyPendingNote>,
}

/// Collection of indexed Curvy committed notes.
#[derive(SimpleObject, Clone, Debug)]
pub struct CurvyCommittedNotes {
    /// Committed notes ordered by chain position.
    pub notes: Vec<CurvyCommittedNote>,
}

/// Collection of indexed Curvy committed nullifiers.
#[derive(SimpleObject, Clone, Debug)]
pub struct CurvyCommittedNullifiers {
    /// Committed nullifiers ordered by chain position.
    pub nullifiers: Vec<CurvyCommittedNullifier>,
}

/// Boolean value returned by Curvy contract checks.
#[derive(SimpleObject, Clone, Debug)]
pub struct CurvyBooleanValue {
    /// Result of the contract check.
    pub value: bool,
}

/// Raw status of a Curvy note.
#[derive(SimpleObject, Clone, Debug)]
pub struct CurvyNoteStatus {
    /// Numeric `NoteStatus` value returned by the Aggregator.
    pub status: i32,
}

/// Address returned by a Curvy portal lookup.
#[derive(SimpleObject, Clone, Debug)]
pub struct CurvyAddress {
    /// Curvy portal address in hexadecimal format.
    pub address: String,
}

/// Selector for safe lookup queries.
///
/// This enum is used together with a single `address` argument when querying
/// for a safe. The selected variant determines how that `address` value is
/// interpreted:
/// - `Address`: `address` is the safe contract address
/// - `Owner`: `address` is a current safe owner address
/// - `ChainKey`: legacy alias for `Owner`
/// - `RegisteredNode`: `address` is a registered node address
#[derive(Enum, Copy, Clone, Eq, PartialEq, Debug)]
pub enum SafeSelectorInput {
    /// Safe contract address to filter by (hexadecimal format)
    #[graphql(name = "ADDRESS")]
    Address,
    /// Current safe owner address to filter by (hexadecimal format)
    #[graphql(name = "OWNER")]
    Owner,
    /// Legacy alias for owner address filtering (hexadecimal format)
    #[graphql(
        name = "CHAIN_KEY",
        deprecation = "Use OWNER instead. CHAIN_KEY is a legacy alias for Safe owner lookup."
    )]
    ChainKey,
    /// Registered node address to filter by (hexadecimal format)
    #[graphql(name = "REGISTERED_NODE")]
    RegisteredNode,
}

/// Aggregated ticket redemption attempt statistics
#[derive(SimpleObject, Clone, Debug)]
pub struct RedeemedStats {
    /// Total amount redeemed from matching ticket redemption events
    #[graphql(name = "redeemedAmount")]
    pub redeemed_amount: TokenValueString,
    /// Total number of matching ticket redemption events
    #[graphql(name = "redemptionCount")]
    pub redemption_count: UInt64,
    /// Total amount from matching failed ticket redemption attempts
    #[graphql(name = "rejectedAmount")]
    pub rejected_amount: TokenValueString,
    /// Total number of matching failed ticket redemption attempts
    #[graphql(name = "rejectionCount")]
    pub rejection_count: UInt64,
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
    /// Legacy chain key field retained for backward compatibility
    #[graphql(
        name = "chainKey",
        deprecation = "Use owners instead. chainKey is legacy Safe metadata and may not reflect the current owner set."
    )]
    pub chain_key: String,
    /// Current signer threshold reconstructed from indexed Safe events
    pub threshold: Option<String>,
    /// Current Safe owner addresses reconstructed from indexed Safe events
    pub owners: Vec<String>,
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
        let map: HashMap<String, String> = [
            ("token", &addresses.token),
            ("channels", &addresses.channels),
            ("announcements", &addresses.announcements),
            ("module_implementation", &addresses.module_implementation),
            ("node_safe_migration", &addresses.node_safe_migration),
            ("node_safe_registry", &addresses.node_safe_registry),
            ("ticket_price_oracle", &addresses.ticket_price_oracle),
            ("winning_probability_oracle", &addresses.winning_probability_oracle),
            ("node_stake_factory", &addresses.node_stake_factory),
            ("xhopr_token", &addresses.xhopr_token),
            ("curvy_aggregator", &addresses.curvy_aggregator),
            ("curvy_portal_factory", &addresses.curvy_portal_factory),
            ("curvy_vault", &addresses.curvy_vault),
            ("service_registry", &addresses.service_registry),
        ]
        .into_iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();

        ContractAddressMap(map)
    }
}
