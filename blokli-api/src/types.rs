//! GraphQL type definitions for HOPR blokli API

use async_graphql::{Enum, SimpleObject};

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

/// Chain information
#[derive(SimpleObject, Clone, Debug)]
pub struct ChainInfo {
    /// Current block number of the blockchain
    #[graphql(name = "blockNumber")]
    pub block_number: i32,
    /// Chain ID of the connected blockchain network
    #[graphql(name = "chainId")]
    pub chain_id: i32,
    /// Current HOPR token price in wxHOPR
    #[graphql(name = "ticketPrice")]
    pub ticket_price: f64,
    /// Current Minimum ticket winning probability
    #[graphql(name = "minTicketWinningProbability")]
    pub min_ticket_winning_probability: f64,
}

/// Account information containing balances and multiaddresses
#[derive(SimpleObject, Clone, Debug)]
pub struct Account {
    /// Unique account on-chain address in hexadecimal format
    #[graphql(name = "chainKey")]
    pub chain_key: String,
    /// Unique account packet key in peer id format
    #[graphql(name = "packetKey")]
    pub packet_key: String,
    /// wxHOPR balance associated with the on-chain address
    #[graphql(name = "accountHoprBalance")]
    pub account_hopr_balance: f64,
    /// Native balance associated with the on-chain address
    #[graphql(name = "accountNativeBalance")]
    pub account_native_balance: f64,
    /// HOPR Safe contract address to which the account is linked
    #[graphql(name = "safeAddress")]
    pub safe_address: Option<String>,
    /// wxHOPR balance associated with the linked Safe contract address
    #[graphql(name = "safeHoprBalance")]
    pub safe_hopr_balance: Option<f64>,
    /// Native balance associated with the linked Safe contract address
    #[graphql(name = "safeNativeBalance")]
    pub safe_native_balance: Option<f64>,
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
    /// On-chain address of the source node in hexadecimal format
    pub source: String,
    /// On-chain address of the destination node in hexadecimal format
    pub destination: String,
    /// Total amount of tokens allocated to the channel
    pub balance: f64,
    /// State of the channel
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
        // Convert 12-byte balance to u128 and then to f64
        let balance = if model.balance.len() == 12 {
            let mut bytes = [0u8; 16];
            bytes[4..].copy_from_slice(&model.balance);
            u128::from_be_bytes(bytes) as f64
        } else {
            0.0
        };

        // Convert 8-byte epoch to u64 and then to i32
        let epoch = if model.epoch.len() == 8 {
            let bytes: [u8; 8] = model.epoch.as_slice().try_into().unwrap_or([0u8; 8]);
            u64::from_be_bytes(bytes) as i32
        } else {
            0
        };

        // Convert 8-byte ticket_index to u64 and then to i32
        let ticket_index = if model.ticket_index.len() == 8 {
            let bytes: [u8; 8] = model.ticket_index.as_slice().try_into().unwrap_or([0u8; 8]);
            u64::from_be_bytes(bytes) as i32
        } else {
            0
        };

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

/// HOPR token (wxHOPR) balance for an address
#[derive(SimpleObject, Clone, Debug)]
pub struct HoprBalance {
    pub address: String,
    /// Token balance as decimal string
    pub balance: String,
    /// Last changed block as hex string
    pub last_changed_block: String,
    /// Last changed transaction index as hex string
    pub last_changed_tx_index: String,
    /// Last changed log index as hex string
    pub last_changed_log_index: String,
}

impl From<blokli_db_entity::hopr_balance::Model> for HoprBalance {
    fn from(model: blokli_db_entity::hopr_balance::Model) -> Self {
        // Convert 12-byte balance to u128 for decimal representation
        let balance_str = if model.balance.len() == 12 {
            let mut bytes = [0u8; 16];
            bytes[4..].copy_from_slice(&model.balance);
            u128::from_be_bytes(bytes).to_string()
        } else {
            hex::encode(&model.balance)
        };

        Self {
            address: model.address,
            balance: balance_str,
            last_changed_block: hex::encode(&model.last_changed_block),
            last_changed_tx_index: hex::encode(&model.last_changed_tx_index),
            last_changed_log_index: hex::encode(&model.last_changed_log_index),
        }
    }
}

/// Native token (xDai) balance for an address
#[derive(SimpleObject, Clone, Debug)]
pub struct NativeBalance {
    pub address: String,
    /// Native balance as decimal string
    pub balance: String,
    /// Last changed block as hex string
    pub last_changed_block: String,
    /// Last changed transaction index as hex string
    pub last_changed_tx_index: String,
    /// Last changed log index as hex string
    pub last_changed_log_index: String,
}

impl From<blokli_db_entity::native_balance::Model> for NativeBalance {
    fn from(model: blokli_db_entity::native_balance::Model) -> Self {
        // Convert 12-byte balance to u128 for decimal representation
        let balance_str = if model.balance.len() == 12 {
            let mut bytes = [0u8; 16];
            bytes[4..].copy_from_slice(&model.balance);
            u128::from_be_bytes(bytes).to_string()
        } else {
            hex::encode(&model.balance)
        };

        Self {
            address: model.address,
            balance: balance_str,
            last_changed_block: hex::encode(&model.last_changed_block),
            last_changed_tx_index: hex::encode(&model.last_changed_tx_index),
            last_changed_log_index: hex::encode(&model.last_changed_log_index),
        }
    }
}
