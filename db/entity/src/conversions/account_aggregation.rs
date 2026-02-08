//! Account aggregation utilities using the `account_current` database view

use std::collections::HashMap;

use hopr_primitive_types::{primitives::Address, traits::ToHex};
use sea_orm::{ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter};

use crate::{codegen::announcement, views::account_current};

fn bytes_to_address_hex(bytes: &[u8]) -> Result<String, sea_orm::DbErr> {
    if bytes.len() != 20 {
        return Err(sea_orm::DbErr::Custom(format!(
            "Invalid address length: expected 20 bytes, got {}",
            bytes.len()
        )));
    }
    let mut addr_bytes = [0u8; 20];
    addr_bytes.copy_from_slice(bytes);
    Ok(Address::new(&addr_bytes).to_hex())
}

/// Aggregated account data with all related information
#[derive(Debug, Clone)]
pub struct AggregatedAccount {
    pub keyid: i64,
    pub chain_key: String,
    pub packet_key: String,
    pub safe_address: Option<String>,
    pub multi_addresses: Vec<String>,
}

/// Given a list of `account_current` view rows, fetch announcements and build aggregated accounts
async fn aggregate_accounts<C>(
    conn: &C,
    current_accounts: Vec<account_current::Model>,
) -> Result<Vec<AggregatedAccount>, sea_orm::DbErr>
where
    C: ConnectionTrait,
{
    if current_accounts.is_empty() {
        return Ok(Vec::new());
    }

    let account_ids: Vec<i64> = current_accounts.iter().map(|a| a.account_id).collect();

    // Batch fetch all announcements
    let announcements = announcement::Entity::find()
        .filter(announcement::Column::AccountId.is_in(account_ids))
        .all(conn)
        .await?;

    // Group announcements by account_id
    let mut announcements_by_account: HashMap<i64, Vec<String>> = HashMap::new();
    for ann in announcements {
        announcements_by_account
            .entry(ann.account_id)
            .or_default()
            .push(ann.multiaddress);
    }

    // Aggregate all data
    current_accounts
        .into_iter()
        .map(|row| {
            let multi_addresses = announcements_by_account
                .get(&row.account_id)
                .cloned()
                .unwrap_or_default();

            let chain_key_str = bytes_to_address_hex(&row.chain_key)?;

            let safe_address_str = row
                .safe_address
                .as_ref()
                .map(|addr| bytes_to_address_hex(addr))
                .transpose()?;

            Ok(AggregatedAccount {
                keyid: row.account_id,
                chain_key: chain_key_str,
                packet_key: row.packet_key,
                safe_address: safe_address_str,
                multi_addresses,
            })
        })
        .collect()
}

/// Fetch all accounts with their related data using the `account_current` view
///
/// The `account_current` view already returns one row per account with the latest state,
/// so no deduplication or ordering is needed in application code.
///
/// # Arguments
/// * `conn` - Database connection
///
/// # Returns
/// * `Result<Vec<AggregatedAccount>, sea_orm::DbErr>` - List of aggregated accounts
pub async fn fetch_accounts_with_balances<C>(conn: &C) -> Result<Vec<AggregatedAccount>, sea_orm::DbErr>
where
    C: ConnectionTrait,
{
    let current_accounts = account_current::Entity::find().all(conn).await?;
    aggregate_accounts(conn, current_accounts).await
}

/// Fetch accounts for specific addresses with their related data
///
/// Filters by chain_key addresses, then uses the `account_current` view for latest state.
///
/// # Arguments
/// * `conn` - Database connection
/// * `addresses` - List of chain_key addresses to filter by
///
/// # Returns
/// * `Result<Vec<AggregatedAccount>, sea_orm::DbErr>` - List of aggregated accounts matching the addresses
pub async fn fetch_accounts_with_balances_for_addresses<C>(
    conn: &C,
    addresses: Vec<String>,
) -> Result<Vec<AggregatedAccount>, sea_orm::DbErr>
where
    C: ConnectionTrait,
{
    if addresses.is_empty() {
        return Ok(Vec::new());
    }

    // Convert string addresses to binary for query
    let binary_addresses: Result<Vec<Vec<u8>>, sea_orm::DbErr> = addresses
        .iter()
        .map(|a| {
            Address::from_hex(a)
                .map(|addr| addr.as_ref().to_vec())
                .map_err(|e| sea_orm::DbErr::Custom(format!("Invalid address: {}", e)))
        })
        .collect();
    let binary_addresses = binary_addresses?;

    let current_accounts = account_current::Entity::find()
        .filter(account_current::Column::ChainKey.is_in(binary_addresses))
        .all(conn)
        .await?;

    aggregate_accounts(conn, current_accounts).await
}

/// Fetch accounts by their keyids with the `account_current` view
///
/// Filters by account IDs, useful when you have keyids from channels.
///
/// # Arguments
/// * `conn` - Database connection
/// * `keyids` - List of account IDs (keyids) to fetch
///
/// # Returns
/// * `Result<Vec<AggregatedAccount>, sea_orm::DbErr>` - List of aggregated accounts matching the keyids
pub async fn fetch_accounts_by_keyids<C>(conn: &C, keyids: Vec<i64>) -> Result<Vec<AggregatedAccount>, sea_orm::DbErr>
where
    C: ConnectionTrait,
{
    if keyids.is_empty() {
        return Ok(Vec::new());
    }

    let current_accounts = account_current::Entity::find()
        .filter(account_current::Column::AccountId.is_in(keyids))
        .all(conn)
        .await?;

    aggregate_accounts(conn, current_accounts).await
}

/// Fetch accounts with optional filters using the `account_current` view
///
/// Multiple filters can be combined. If no filters are provided, returns all accounts.
///
/// # Arguments
/// * `conn` - Database connection
/// * `keyid` - Optional filter by account ID (keyid)
/// * `packet_key` - Optional filter by packet key
/// * `chain_key` - Optional filter by chain key
///
/// # Returns
/// * `Result<Vec<AggregatedAccount>, sea_orm::DbErr>` - List of aggregated accounts matching the filters
pub async fn fetch_accounts_with_filters<C>(
    conn: &C,
    keyid: Option<i64>,
    packet_key: Option<String>,
    chain_key: Option<String>,
) -> Result<Vec<AggregatedAccount>, sea_orm::DbErr>
where
    C: ConnectionTrait,
{
    let mut query = account_current::Entity::find();

    if let Some(id) = keyid {
        query = query.filter(account_current::Column::AccountId.eq(id));
    }

    if let Some(pk) = packet_key {
        query = query.filter(account_current::Column::PacketKey.eq(pk.strip_prefix("0x").unwrap_or(&pk).to_string()));
    }

    if let Some(ck) = chain_key {
        let binary_chain_key = Address::from_hex(&ck)
            .map_err(|e| sea_orm::DbErr::Custom(format!("Invalid address: {}", e)))?
            .as_ref()
            .to_vec();
        query = query.filter(account_current::Column::ChainKey.eq(binary_chain_key));
    }

    let current_accounts = query.all(conn).await?;
    aggregate_accounts(conn, current_accounts).await
}

#[cfg(test)]
mod tests {
    // Note: These tests would require a test database setup
    // For now, we just ensure the module compiles
    #[test]
    fn test_module_compiles() {
        assert!(true);
    }
}
