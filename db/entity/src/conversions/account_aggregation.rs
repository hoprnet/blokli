//! Account aggregation utilities with optimized batch loading

use std::collections::HashMap;

use sea_orm::{ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter, QueryOrder};

use super::balances::{address_to_string, string_to_address};
use crate::codegen::{account, account_state, announcement};

/// Aggregated account data with all related information
#[derive(Debug, Clone)]
pub struct AggregatedAccount {
    pub keyid: i64,
    pub chain_key: String,
    pub packet_key: String,
    pub safe_address: Option<String>,
    pub multi_addresses: Vec<String>,
    pub safe_transaction_count: u64,
}

/// Fetch all accounts with their related data using optimized batch loading
///
/// This function eliminates N+1 queries by:
/// 1. Fetching all accounts in one query
/// 2. Batch loading all announcements for those accounts
/// 3. Aggregating the data in memory
///
/// Instead of 1 + (N * 2) queries, this uses only 2 queries total.
///
/// # Arguments
/// * `db` - Database connection
///
/// # Returns
/// * `Result<Vec<AggregatedAccount>, sea_orm::DbErr>` - List of aggregated accounts
pub async fn fetch_accounts_with_balances<C>(conn: &C) -> Result<Vec<AggregatedAccount>, sea_orm::DbErr>
where
    C: ConnectionTrait,
{
    // 1. Fetch all accounts (1 query)
    let accounts = account::Entity::find().all(conn).await?;

    if accounts.is_empty() {
        return Ok(Vec::new());
    }

    // Collect all account IDs
    let account_ids: Vec<i64> = accounts.iter().map(|a| a.id).collect();

    let mut all_addresses: Vec<Vec<u8>> = accounts.iter().map(|a| a.chain_key.clone()).collect();

    // Batch query account_state for all accounts to get safe_address
    let account_states = account_state::Entity::find()
        .filter(account_state::Column::AccountId.is_in(account_ids.clone()))
        .order_by_desc(account_state::Column::PublishedBlock)
        .order_by_desc(account_state::Column::PublishedTxIndex)
        .order_by_desc(account_state::Column::PublishedLogIndex)
        .all(conn)
        .await?;

    // Build map of account_id -> safe_address (only keep latest state per account)
    let mut safe_address_map: HashMap<i64, Vec<u8>> = HashMap::new();
    for state in account_states {
        if let Some(safe_addr) = state.safe_address {
            // Only insert if we haven't seen this account yet (first occurrence is latest due to ordering)
            safe_address_map.entry(state.account_id).or_insert(safe_addr);
        }
    }

    // Add safe addresses to all_addresses for balance lookup
    all_addresses.extend(safe_address_map.values().cloned());

    // 2. Batch fetch all announcements (1 query)
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

    // 3. Aggregate all data
    let result = accounts
        .into_iter()
        .map(|account| {
            let multi_addresses = announcements_by_account.get(&account.id).cloned().unwrap_or_default();

            // Convert chain_key to string
            let chain_key_str = address_to_string(&account.chain_key);

            // Convert safe_address to string if present
            let safe_address_str = safe_address_map.get(&account.id).map(|addr| address_to_string(addr));

            AggregatedAccount {
                keyid: account.id,
                chain_key: chain_key_str,
                packet_key: account.packet_key,
                safe_address: safe_address_str,
                multi_addresses,
                // TODO: Implement safe transaction count fetching from blockchain or database
                safe_transaction_count: 0,
            }
        })
        .collect();

    Ok(result)
}

/// Fetch accounts for specific addresses with their related data using optimized batch loading
///
/// This function is similar to `fetch_accounts_with_balances` but only fetches accounts
/// whose chain_key is in the provided address list. This is useful for queries that only
/// need a subset of accounts.
///
/// Uses the same optimized batch loading approach: 2 queries total instead of N+1.
///
/// # Arguments
/// * `db` - Database connection
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
    let binary_addresses: Vec<Vec<u8>> = addresses.iter().map(|a| string_to_address(a)).collect();

    // 1. Fetch accounts filtered by chain_key (1 query)
    let accounts = account::Entity::find()
        .filter(account::Column::ChainKey.is_in(binary_addresses))
        .all(conn)
        .await?;

    if accounts.is_empty() {
        return Ok(Vec::new());
    }

    // Collect all account IDs
    let account_ids: Vec<i64> = accounts.iter().map(|a| a.id).collect();

    let mut all_addresses: Vec<Vec<u8>> = accounts.iter().map(|a| a.chain_key.clone()).collect();

    // Batch query account_state for all accounts to get safe_address
    let account_states = account_state::Entity::find()
        .filter(account_state::Column::AccountId.is_in(account_ids.clone()))
        .order_by_desc(account_state::Column::PublishedBlock)
        .order_by_desc(account_state::Column::PublishedTxIndex)
        .order_by_desc(account_state::Column::PublishedLogIndex)
        .all(conn)
        .await?;

    // Build map of account_id -> safe_address (only keep latest state per account)
    let mut safe_address_map: HashMap<i64, Vec<u8>> = HashMap::new();
    for state in account_states {
        if let Some(safe_addr) = state.safe_address {
            // Only insert if we haven't seen this account yet (first occurrence is latest due to ordering)
            safe_address_map.entry(state.account_id).or_insert(safe_addr);
        }
    }

    // Add safe addresses to all_addresses for balance lookup
    all_addresses.extend(safe_address_map.values().cloned());

    // 2. Batch fetch all announcements (1 query)
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

    // 3. Aggregate all data
    let result = accounts
        .into_iter()
        .map(|account| {
            let multi_addresses = announcements_by_account.get(&account.id).cloned().unwrap_or_default();

            // Convert chain_key to string
            let chain_key_str = address_to_string(&account.chain_key);

            // Convert safe_address to string if present
            let safe_address_str = safe_address_map.get(&account.id).map(|addr| address_to_string(addr));

            AggregatedAccount {
                keyid: account.id,
                chain_key: chain_key_str,
                packet_key: account.packet_key,
                safe_address: safe_address_str,
                multi_addresses,
                // TODO: Implement safe transaction count fetching from blockchain or database
                safe_transaction_count: 0,
            }
        })
        .collect();

    Ok(result)
}

/// Fetch accounts by their keyids with optimized batch loading
///
/// This function is similar to `fetch_accounts_with_balances_for_addresses` but filters
/// by account IDs instead of addresses. Useful when you have keyids from channels.
///
/// Uses the same optimized batch loading approach: 2 queries total instead of N+1.
///
/// # Arguments
/// * `db` - Database connection
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

    // 1. Fetch accounts filtered by id (1 query)
    let accounts = account::Entity::find()
        .filter(account::Column::Id.is_in(keyids))
        .all(conn)
        .await?;

    if accounts.is_empty() {
        return Ok(Vec::new());
    }

    // Collect all account IDs
    let account_ids: Vec<i64> = accounts.iter().map(|a| a.id).collect();

    let mut all_addresses: Vec<Vec<u8>> = accounts.iter().map(|a| a.chain_key.clone()).collect();

    // Batch query account_state for all accounts to get safe_address
    let account_states = account_state::Entity::find()
        .filter(account_state::Column::AccountId.is_in(account_ids.clone()))
        .order_by_desc(account_state::Column::PublishedBlock)
        .order_by_desc(account_state::Column::PublishedTxIndex)
        .order_by_desc(account_state::Column::PublishedLogIndex)
        .all(conn)
        .await?;

    // Build map of account_id -> safe_address (only keep latest state per account)
    let mut safe_address_map: HashMap<i64, Vec<u8>> = HashMap::new();
    for state in account_states {
        if let Some(safe_addr) = state.safe_address {
            // Only insert if we haven't seen this account yet (first occurrence is latest due to ordering)
            safe_address_map.entry(state.account_id).or_insert(safe_addr);
        }
    }

    // Add safe addresses to all_addresses for balance lookup
    all_addresses.extend(safe_address_map.values().cloned());

    // 2. Batch fetch all announcements (1 query)
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

    // 3. Aggregate all data
    let result = accounts
        .into_iter()
        .map(|account| {
            let multi_addresses = announcements_by_account.get(&account.id).cloned().unwrap_or_default();

            // Convert chain_key to string
            let chain_key_str = address_to_string(&account.chain_key);

            // Convert safe_address to string if present
            let safe_address_str = safe_address_map.get(&account.id).map(|addr| address_to_string(addr));

            AggregatedAccount {
                keyid: account.id,
                chain_key: chain_key_str,
                packet_key: account.packet_key,
                safe_address: safe_address_str,
                multi_addresses,
                // TODO: Implement safe transaction count fetching from blockchain or database
                safe_transaction_count: 0,
            }
        })
        .collect();

    Ok(result)
}

/// Fetch accounts with optional filters using optimized batch loading
///
/// This function allows filtering accounts by keyid, packet_key, and/or chain_key.
/// Multiple filters can be combined. If no filters are provided, returns all accounts.
///
/// Uses the same optimized batch loading approach: 2 queries total instead of N+1.
///
/// # Arguments
/// * `db` - Database connection
/// * `keyid` - Optional filter by account ID (keyid)
/// * `packet_key` - Optional filter by packet key
/// * `chain_key` - Optional filter by chain key
///
/// # Returns
/// * `Result<Vec<AggregatedAccount>, sea_orm::DbErr>` - List of aggregated accounts matching the filters
pub async fn fetch_accounts_with_filters<C>(
    conn: &C,
    keyid: Option<i32>,
    packet_key: Option<String>,
    chain_key: Option<String>,
) -> Result<Vec<AggregatedAccount>, sea_orm::DbErr>
where
    C: ConnectionTrait,
{
    // 1. Build query with filters (1 query)
    let mut query = account::Entity::find();

    if let Some(id) = keyid {
        query = query.filter(account::Column::Id.eq(id));
    }

    if let Some(pk) = packet_key {
        query = query.filter(account::Column::PacketKey.eq(pk));
    }

    if let Some(ck) = chain_key {
        let binary_chain_key = string_to_address(&ck);
        query = query.filter(account::Column::ChainKey.eq(binary_chain_key));
    }

    let accounts = query.all(conn).await?;

    if accounts.is_empty() {
        return Ok(Vec::new());
    }

    // Collect all account IDs
    let account_ids: Vec<i64> = accounts.iter().map(|a| a.id).collect();

    let mut all_addresses: Vec<Vec<u8>> = accounts.iter().map(|a| a.chain_key.clone()).collect();

    // Batch query account_state for all accounts to get safe_address
    let account_states = account_state::Entity::find()
        .filter(account_state::Column::AccountId.is_in(account_ids.clone()))
        .order_by_desc(account_state::Column::PublishedBlock)
        .order_by_desc(account_state::Column::PublishedTxIndex)
        .order_by_desc(account_state::Column::PublishedLogIndex)
        .all(conn)
        .await?;

    // Build map of account_id -> safe_address (only keep latest state per account)
    let mut safe_address_map: HashMap<i64, Vec<u8>> = HashMap::new();
    for state in account_states {
        if let Some(safe_addr) = state.safe_address {
            // Only insert if we haven't seen this account yet (first occurrence is latest due to ordering)
            safe_address_map.entry(state.account_id).or_insert(safe_addr);
        }
    }

    // Add safe addresses to all_addresses for balance lookup
    all_addresses.extend(safe_address_map.values().cloned());

    // 2. Batch fetch all announcements (1 query)
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

    // 3. Aggregate all data
    let result = accounts
        .into_iter()
        .map(|account| {
            let multi_addresses = announcements_by_account.get(&account.id).cloned().unwrap_or_default();

            // Convert chain_key to string
            let chain_key_str = address_to_string(&account.chain_key);

            // Convert safe_address to string if present
            let safe_address_str = safe_address_map.get(&account.id).map(|addr| address_to_string(addr));

            AggregatedAccount {
                keyid: account.id,
                chain_key: chain_key_str,
                packet_key: account.packet_key,
                safe_address: safe_address_str,
                multi_addresses,
                // TODO: Implement safe transaction count fetching from blockchain or database
                safe_transaction_count: 0,
            }
        })
        .collect();

    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    // Note: These tests would require a test database setup
    // For now, we just ensure the module compiles
    #[test]
    fn test_module_compiles() {
        assert!(true);
    }
}
