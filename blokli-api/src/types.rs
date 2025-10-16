//! GraphQL type definitions for HOPR blokli API

use async_graphql::SimpleObject;
use chrono::{DateTime, NaiveDateTime, Utc};

/// Account information containing chain and packet keys
#[derive(SimpleObject, Clone, Debug)]
pub struct Account {
    pub id: i32,
    pub chain_key: String,
    pub packet_key: String,
    pub published_block: i32,
}

impl From<blokli_db_entity::account::Model> for Account {
    fn from(model: blokli_db_entity::account::Model) -> Self {
        Self {
            id: model.id,
            chain_key: model.chain_key,
            packet_key: model.packet_key,
            published_block: model.published_block,
        }
    }
}

/// Network announcement with multiaddress information
#[derive(SimpleObject, Clone, Debug)]
pub struct Announcement {
    pub id: i32,
    pub account_id: i32,
    /// Multiaddress list encoded as hex string
    pub multiaddress_list: String,
    pub published_block: i32,
}

impl From<blokli_db_entity::announcement::Model> for Announcement {
    fn from(model: blokli_db_entity::announcement::Model) -> Self {
        Self {
            id: model.id,
            account_id: model.account_id,
            multiaddress_list: hex::encode(&model.multiaddress_list),
            published_block: model.published_block,
        }
    }
}

/// Payment channel between two nodes
#[derive(SimpleObject, Clone, Debug)]
pub struct Channel {
    pub id: i32,
    pub concrete_channel_id: String,
    pub source: String,
    pub destination: String,
    /// Channel balance as decimal string
    pub balance: String,
    pub status: i8,
    /// Epoch as hex string
    pub epoch: String,
    /// Ticket index as hex string
    pub ticket_index: String,
    pub closure_time: Option<DateTime<Utc>>,
    pub corrupted_state: bool,
}

impl From<blokli_db_entity::channel::Model> for Channel {
    fn from(model: blokli_db_entity::channel::Model) -> Self {
        // Convert 12-byte balance to u128 for decimal representation
        let balance_str = if model.balance.len() == 12 {
            let mut bytes = [0u8; 16];
            bytes[4..].copy_from_slice(&model.balance);
            u128::from_be_bytes(bytes).to_string()
        } else {
            hex::encode(&model.balance)
        };

        Self {
            id: model.id,
            concrete_channel_id: model.concrete_channel_id,
            source: model.source,
            destination: model.destination,
            balance: balance_str,
            status: model.status,
            epoch: hex::encode(&model.epoch),
            ticket_index: hex::encode(&model.ticket_index),
            closure_time: model.closure_time,
            corrupted_state: model.corrupted_state,
        }
    }
}

/// HOPR token (wxHOPR) balance for an address
#[derive(SimpleObject, Clone, Debug)]
pub struct HoprBalance {
    pub address: String,
    /// Token balance as decimal string
    pub balance: String,
    pub last_changed_block: Option<String>,
    pub last_changed_tx_index: Option<String>,
    pub last_changed_log_index: Option<String>,
    pub updated_at: Option<NaiveDateTime>,
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
            last_changed_block: model.last_changed_block.map(|b| hex::encode(&b)),
            last_changed_tx_index: model.last_changed_tx_index.map(|b| hex::encode(&b)),
            last_changed_log_index: model.last_changed_log_index.map(|b| hex::encode(&b)),
            updated_at: model.updated_at,
        }
    }
}

/// Native token (xDai) balance for an address
#[derive(SimpleObject, Clone, Debug)]
pub struct NativeBalance {
    pub address: String,
    /// Native balance as decimal string
    pub balance: String,
    pub last_changed_block: Option<String>,
    pub last_changed_tx_index: Option<String>,
    pub last_changed_log_index: Option<String>,
    pub updated_at: Option<NaiveDateTime>,
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
            last_changed_block: model.last_changed_block.map(|b| hex::encode(&b)),
            last_changed_tx_index: model.last_changed_tx_index.map(|b| hex::encode(&b)),
            last_changed_log_index: model.last_changed_log_index.map(|b| hex::encode(&b)),
            updated_at: model.updated_at,
        }
    }
}
