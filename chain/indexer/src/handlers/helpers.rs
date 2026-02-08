use std::collections::HashMap;

use blokli_api_types::{Account, Channel, ChannelUpdate, TokenValueString, UInt64};
use blokli_db_entity::{account, conversions::account_aggregation, views::channel_current};
use chrono::Utc;
use hopr_bindings::exports::alloy::hex;
use hopr_crypto_types::prelude::Hash;
use hopr_primitive_types::prelude::{Address, HoprBalance, IntoEndian, ToHex};
use sea_orm::{ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter};

use crate::errors::{CoreEthereumIndexerError, Result};

/// Helper function to construct a complete ChannelUpdate from database channel_id
///
/// This function queries the database for the channel, its latest state, and both
/// participating accounts, then constructs a complete ChannelUpdate with all GraphQL data.
/// This is used to create events to publish to subscribers.
///
/// # Arguments
/// * `db` - Database connection
/// * `channel_id` - The on-chain channel ID (Hash, not the internal database ID)
///
/// # Returns
/// * `Result<ChannelUpdate>` - Complete channel update with all account data
///
/// # Errors
/// * Returns error if channel not found, state not found, or accounts not found
#[allow(dead_code)]
pub(super) async fn construct_channel_update<C>(conn: &C, channel_id: &Hash) -> Result<ChannelUpdate>
where
    C: ConnectionTrait,
{
    // Convert Hash to hex string for database query
    let channel_id_hex = hex::encode(channel_id.as_ref());

    // 1. Get the latest channel state via channel_current view (includes channel identity data)
    let current = channel_current::Entity::find()
        .filter(channel_current::Column::ConcreteChannelId.eq(&channel_id_hex))
        .one(conn)
        .await
        .map_err(|e| CoreEthereumIndexerError::ProcessError(format!("Failed to query channel_current: {}", e)))?
        .ok_or_else(|| CoreEthereumIndexerError::ProcessError(format!("Channel {} not found", channel_id_hex)))?;

    // 2. Fetch both accounts using the optimized aggregation function
    let account_ids = vec![current.source, current.destination];
    let accounts_result = account_aggregation::fetch_accounts_by_keyids(conn, account_ids)
        .await
        .map_err(|e| CoreEthereumIndexerError::ProcessError(format!("Failed to fetch accounts: {}", e)))?;

    let account_map: HashMap<i64, account_aggregation::AggregatedAccount> =
        accounts_result.into_iter().map(|a| (a.keyid, a)).collect();

    let source_account = account_map.get(&current.source).ok_or_else(|| {
        CoreEthereumIndexerError::ProcessError(format!("Source account {} not found", current.source))
    })?;

    let dest_account = account_map.get(&current.destination).ok_or_else(|| {
        CoreEthereumIndexerError::ProcessError(format!("Destination account {} not found", current.destination))
    })?;

    // 3. Convert to GraphQL types

    let balance_bytes_32: [u8; 32] = {
        let mut bytes = [0u8; 32];
        bytes[20..32].copy_from_slice(current.balance.as_slice());
        bytes
    };

    let hopr_balance = HoprBalance::from_be_bytes(balance_bytes_32);

    let channel_gql = Channel {
        concrete_channel_id: current.concrete_channel_id,
        source: current.source,
        destination: current.destination,
        balance: TokenValueString(hopr_balance.to_string()),
        status: current.status.into(),
        epoch: i32::try_from(current.epoch).map_err(|e| {
            CoreEthereumIndexerError::ValidationError(format!(
                "Channel epoch {} out of range for i32: {}",
                current.epoch, e
            ))
        })?,
        ticket_index: UInt64(u64::try_from(current.ticket_index).map_err(|e| {
            CoreEthereumIndexerError::ValidationError(format!(
                "Channel ticket_index {} is negative or out of range: {}",
                current.ticket_index, e
            ))
        })?),
        closure_time: current.closure_time.map(|time| time.with_timezone(&Utc)),
    };

    let source_gql = Account {
        keyid: source_account.keyid,
        chain_key: source_account.chain_key.clone(),
        packet_key: source_account.packet_key.clone(),
        safe_address: source_account.safe_address.clone(),
        multi_addresses: source_account.multi_addresses.clone(),
    };

    let dest_gql = Account {
        keyid: dest_account.keyid,
        chain_key: dest_account.chain_key.clone(),
        packet_key: dest_account.packet_key.clone(),
        safe_address: dest_account.safe_address.clone(),
        multi_addresses: dest_account.multi_addresses.clone(),
    };

    Ok(ChannelUpdate {
        channel: channel_gql,
        source: source_gql,
        destination: dest_gql,
    })
}

/// Helper function to construct a complete Account from database account address
///
/// This function queries the database for the account with all related data
/// (balances, announcements, safe address) and constructs a complete Account GraphQL object.
/// This is used to create events to publish to subscribers.
///
/// # Arguments
/// * `db` - Database connection
/// * `address` - The on-chain account address
///
/// # Returns
/// * `Result<Account>` - Complete account with all data
///
/// # Errors
/// * Returns error if account not found
pub(super) async fn construct_account_update<C>(conn: &C, address: &Address) -> Result<Account>
where
    C: ConnectionTrait,
{
    // Convert Address to binary for database query
    let address_bytes = address.as_ref().to_vec();

    // 1. Find the account by chain_key
    let account = account::Entity::find()
        .filter(account::Column::ChainKey.eq(address_bytes.clone()))
        .one(conn)
        .await
        .map_err(|e| CoreEthereumIndexerError::ProcessError(format!("Failed to query account: {}", e)))?
        .ok_or_else(|| CoreEthereumIndexerError::ProcessError(format!("Account {} not found", address.to_hex())))?;

    // 2. Fetch complete account data using the optimized aggregation function
    let accounts_result = account_aggregation::fetch_accounts_by_keyids(conn, vec![account.id])
        .await
        .map_err(|e| CoreEthereumIndexerError::ProcessError(format!("Failed to fetch account data: {}", e)))?;

    let aggregated = accounts_result
        .into_iter()
        .next()
        .ok_or_else(|| CoreEthereumIndexerError::ProcessError(format!("Account {} data not found", account.id)))?;

    // 3. Convert to GraphQL type
    Ok(Account {
        keyid: aggregated.keyid,
        chain_key: aggregated.chain_key,
        packet_key: aggregated.packet_key,
        safe_address: aggregated.safe_address,
        multi_addresses: aggregated.multi_addresses,
    })
}
