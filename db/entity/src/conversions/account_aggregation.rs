//! Account aggregation utilities with optimized batch loading

use std::collections::HashMap;

use sea_orm::{ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter};

use super::balances::{address_to_string, hopr_balance_to_string, native_balance_to_string, string_to_address};
use crate::codegen::{account, announcement, hopr_balance, native_balance};

/// Aggregated account data with all related information
#[derive(Debug, Clone)]
pub struct AggregatedAccount {
    pub keyid: i32,
    pub chain_key: String,
    pub packet_key: String,
    pub account_hopr_balance: String,
    pub account_native_balance: String,
    pub safe_address: Option<String>,
    pub safe_hopr_balance: Option<String>,
    pub safe_native_balance: Option<String>,
    pub multi_addresses: Vec<String>,
}

/// Fetch all accounts with their related data using optimized batch loading
///
/// This function eliminates N+1 queries by:
/// 1. Fetching all accounts in one query
/// 2. Batch loading all announcements for those accounts
/// 3. Batch loading all balances (HOPR and native) for all relevant addresses
/// 4. Aggregating the data in memory
///
/// Instead of 1 + (N * 5) queries, this uses only 4 queries total.
///
/// # Arguments
/// * `db` - Database connection
///
/// # Returns
/// * `Result<Vec<AggregatedAccount>, sea_orm::DbErr>` - List of aggregated accounts
pub async fn fetch_accounts_with_balances(db: &DatabaseConnection) -> Result<Vec<AggregatedAccount>, sea_orm::DbErr> {
    // 1. Fetch all accounts (1 query)
    let accounts = account::Entity::find().all(db).await?;

    if accounts.is_empty() {
        return Ok(Vec::new());
    }

    // Collect all account IDs and addresses
    let account_ids: Vec<i32> = accounts.iter().map(|a| a.id).collect();
    let all_addresses: Vec<Vec<u8>> = accounts.iter().map(|a| a.chain_key.clone()).collect();

    // Add safe addresses if they exist
    for _account in &accounts {
        // TODO(Phase 2-3): Query account_state for safe_address
        if false {}
    }

    // 2. Batch fetch all announcements (1 query)
    let announcements = announcement::Entity::find()
        .filter(announcement::Column::AccountId.is_in(account_ids))
        .all(db)
        .await?;

    // Group announcements by account_id
    let mut announcements_by_account: HashMap<i32, Vec<String>> = HashMap::new();
    for ann in announcements {
        announcements_by_account
            .entry(ann.account_id)
            .or_default()
            .push(ann.multiaddress);
    }

    // 3. Batch fetch all HOPR balances (1 query)
    let hopr_balances = hopr_balance::Entity::find()
        .filter(hopr_balance::Column::Address.is_in(all_addresses.clone()))
        .all(db)
        .await?;

    let hopr_balance_map: HashMap<String, String> = hopr_balances
        .into_iter()
        .map(|b| (address_to_string(&b.address), hopr_balance_to_string(&b.balance)))
        .collect();

    // 4. Batch fetch all native balances (1 query)
    let native_balances = native_balance::Entity::find()
        .filter(native_balance::Column::Address.is_in(all_addresses))
        .all(db)
        .await?;

    let native_balance_map: HashMap<String, String> = native_balances
        .into_iter()
        .map(|b| (address_to_string(&b.address), native_balance_to_string(&b.balance)))
        .collect();

    // 5. Aggregate all data
    let result = accounts
        .into_iter()
        .map(|account| {
            let multi_addresses = announcements_by_account.get(&account.id).cloned().unwrap_or_default();

            // Convert chain_key to string for lookups
            let chain_key_str = address_to_string(&account.chain_key);

            // If no balance record exists in DB, return zero balance (non-nullable in GraphQL schema)
            let account_hopr_balance = hopr_balance_map
                .get(&chain_key_str)
                .cloned()
                .unwrap_or_else(|| hopr_balance_to_string(&[]));

            // If no balance record exists in DB, return zero balance (non-nullable in GraphQL schema)
            let account_native_balance = native_balance_map
                .get(&chain_key_str)
                .cloned()
                .unwrap_or_else(|| native_balance_to_string(&[]));

            // Convert safe_address to string if present
            let safe_address_str = None::<String>;

            let (safe_hopr_balance, safe_native_balance) = if let Some(ref safe_addr_str) = safe_address_str {
                (
                    hopr_balance_map.get(safe_addr_str).cloned(),
                    native_balance_map.get(safe_addr_str).cloned(),
                )
            } else {
                (None, None)
            };

            AggregatedAccount {
                keyid: account.id,
                chain_key: chain_key_str,
                packet_key: account.packet_key,
                account_hopr_balance,
                account_native_balance,
                safe_address: safe_address_str,
                safe_hopr_balance,
                safe_native_balance,
                multi_addresses,
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
/// Uses the same optimized batch loading approach: 4 queries total instead of N+1.
///
/// # Arguments
/// * `db` - Database connection
/// * `addresses` - List of chain_key addresses to filter by
///
/// # Returns
/// * `Result<Vec<AggregatedAccount>, sea_orm::DbErr>` - List of aggregated accounts matching the addresses
pub async fn fetch_accounts_with_balances_for_addresses(
    db: &DatabaseConnection,
    addresses: Vec<String>,
) -> Result<Vec<AggregatedAccount>, sea_orm::DbErr> {
    if addresses.is_empty() {
        return Ok(Vec::new());
    }

    // Convert string addresses to binary for query
    let binary_addresses: Vec<Vec<u8>> = addresses.iter().map(|a| string_to_address(a)).collect();

    // 1. Fetch accounts filtered by chain_key (1 query)
    let accounts = account::Entity::find()
        .filter(account::Column::ChainKey.is_in(binary_addresses))
        .all(db)
        .await?;

    if accounts.is_empty() {
        return Ok(Vec::new());
    }

    // Collect all account IDs and addresses
    let account_ids: Vec<i32> = accounts.iter().map(|a| a.id).collect();
    let all_addresses: Vec<Vec<u8>> = accounts.iter().map(|a| a.chain_key.clone()).collect();

    // Add safe addresses if they exist
    for _account in &accounts {
        // TODO(Phase 2-3): Query account_state for safe_address
        if false {}
    }

    // 2. Batch fetch all announcements (1 query)
    let announcements = announcement::Entity::find()
        .filter(announcement::Column::AccountId.is_in(account_ids))
        .all(db)
        .await?;

    // Group announcements by account_id
    let mut announcements_by_account: HashMap<i32, Vec<String>> = HashMap::new();
    for ann in announcements {
        announcements_by_account
            .entry(ann.account_id)
            .or_default()
            .push(ann.multiaddress);
    }

    // 3. Batch fetch all HOPR balances (1 query)
    let hopr_balances = hopr_balance::Entity::find()
        .filter(hopr_balance::Column::Address.is_in(all_addresses.clone()))
        .all(db)
        .await?;

    let hopr_balance_map: HashMap<String, String> = hopr_balances
        .into_iter()
        .map(|b| (address_to_string(&b.address), hopr_balance_to_string(&b.balance)))
        .collect();

    // 4. Batch fetch all native balances (1 query)
    let native_balances = native_balance::Entity::find()
        .filter(native_balance::Column::Address.is_in(all_addresses))
        .all(db)
        .await?;

    let native_balance_map: HashMap<String, String> = native_balances
        .into_iter()
        .map(|b| (address_to_string(&b.address), native_balance_to_string(&b.balance)))
        .collect();

    // 5. Aggregate all data
    let result = accounts
        .into_iter()
        .map(|account| {
            let multi_addresses = announcements_by_account.get(&account.id).cloned().unwrap_or_default();

            // Convert chain_key to string for lookups
            let chain_key_str = address_to_string(&account.chain_key);

            // If no balance record exists in DB, return zero balance (non-nullable in GraphQL schema)
            let account_hopr_balance = hopr_balance_map
                .get(&chain_key_str)
                .cloned()
                .unwrap_or_else(|| hopr_balance_to_string(&[]));

            // If no balance record exists in DB, return zero balance (non-nullable in GraphQL schema)
            let account_native_balance = native_balance_map
                .get(&chain_key_str)
                .cloned()
                .unwrap_or_else(|| native_balance_to_string(&[]));

            // Convert safe_address to string if present
            let safe_address_str = None::<String>;

            let (safe_hopr_balance, safe_native_balance) = if let Some(ref safe_addr_str) = safe_address_str {
                (
                    hopr_balance_map.get(safe_addr_str).cloned(),
                    native_balance_map.get(safe_addr_str).cloned(),
                )
            } else {
                (None, None)
            };

            AggregatedAccount {
                keyid: account.id,
                chain_key: chain_key_str,
                packet_key: account.packet_key,
                account_hopr_balance,
                account_native_balance,
                safe_address: safe_address_str,
                safe_hopr_balance,
                safe_native_balance,
                multi_addresses,
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
/// Uses the same optimized batch loading approach: 4 queries total instead of N+1.
///
/// # Arguments
/// * `db` - Database connection
/// * `keyids` - List of account IDs (keyids) to fetch
///
/// # Returns
/// * `Result<Vec<AggregatedAccount>, sea_orm::DbErr>` - List of aggregated accounts matching the keyids
pub async fn fetch_accounts_by_keyids(
    db: &DatabaseConnection,
    keyids: Vec<i32>,
) -> Result<Vec<AggregatedAccount>, sea_orm::DbErr> {
    if keyids.is_empty() {
        return Ok(Vec::new());
    }

    // 1. Fetch accounts filtered by id (1 query)
    let accounts = account::Entity::find()
        .filter(account::Column::Id.is_in(keyids))
        .all(db)
        .await?;

    if accounts.is_empty() {
        return Ok(Vec::new());
    }

    // Collect all account IDs and addresses
    let account_ids: Vec<i32> = accounts.iter().map(|a| a.id).collect();
    let all_addresses: Vec<Vec<u8>> = accounts.iter().map(|a| a.chain_key.clone()).collect();

    // Add safe addresses if they exist
    for _account in &accounts {
        // TODO(Phase 2-3): Query account_state for safe_address
        if false {}
    }

    // 2. Batch fetch all announcements (1 query)
    let announcements = announcement::Entity::find()
        .filter(announcement::Column::AccountId.is_in(account_ids))
        .all(db)
        .await?;

    // Group announcements by account_id
    let mut announcements_by_account: HashMap<i32, Vec<String>> = HashMap::new();
    for ann in announcements {
        announcements_by_account
            .entry(ann.account_id)
            .or_default()
            .push(ann.multiaddress);
    }

    // 3. Batch fetch all HOPR balances (1 query)
    let hopr_balances = hopr_balance::Entity::find()
        .filter(hopr_balance::Column::Address.is_in(all_addresses.clone()))
        .all(db)
        .await?;

    let hopr_balance_map: HashMap<String, String> = hopr_balances
        .into_iter()
        .map(|b| (address_to_string(&b.address), hopr_balance_to_string(&b.balance)))
        .collect();

    // 4. Batch fetch all native balances (1 query)
    let native_balances = native_balance::Entity::find()
        .filter(native_balance::Column::Address.is_in(all_addresses))
        .all(db)
        .await?;

    let native_balance_map: HashMap<String, String> = native_balances
        .into_iter()
        .map(|b| (address_to_string(&b.address), native_balance_to_string(&b.balance)))
        .collect();

    // 5. Aggregate all data
    let result = accounts
        .into_iter()
        .map(|account| {
            let multi_addresses = announcements_by_account.get(&account.id).cloned().unwrap_or_default();

            // Convert chain_key to string for lookups
            let chain_key_str = address_to_string(&account.chain_key);

            // If no balance record exists in DB, return zero balance (non-nullable in GraphQL schema)
            let account_hopr_balance = hopr_balance_map
                .get(&chain_key_str)
                .cloned()
                .unwrap_or_else(|| hopr_balance_to_string(&[]));

            // If no balance record exists in DB, return zero balance (non-nullable in GraphQL schema)
            let account_native_balance = native_balance_map
                .get(&chain_key_str)
                .cloned()
                .unwrap_or_else(|| native_balance_to_string(&[]));

            // Convert safe_address to string if present
            let safe_address_str = None::<String>;

            let (safe_hopr_balance, safe_native_balance) = if let Some(ref safe_addr_str) = safe_address_str {
                (
                    hopr_balance_map.get(safe_addr_str).cloned(),
                    native_balance_map.get(safe_addr_str).cloned(),
                )
            } else {
                (None, None)
            };

            AggregatedAccount {
                keyid: account.id,
                chain_key: chain_key_str,
                packet_key: account.packet_key,
                account_hopr_balance,
                account_native_balance,
                safe_address: safe_address_str,
                safe_hopr_balance,
                safe_native_balance,
                multi_addresses,
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
/// Uses the same optimized batch loading approach: 4 queries total instead of N+1.
///
/// # Arguments
/// * `db` - Database connection
/// * `keyid` - Optional filter by account ID (keyid)
/// * `packet_key` - Optional filter by packet key
/// * `chain_key` - Optional filter by chain key
///
/// # Returns
/// * `Result<Vec<AggregatedAccount>, sea_orm::DbErr>` - List of aggregated accounts matching the filters
pub async fn fetch_accounts_with_filters(
    db: &DatabaseConnection,
    keyid: Option<i32>,
    packet_key: Option<String>,
    chain_key: Option<String>,
) -> Result<Vec<AggregatedAccount>, sea_orm::DbErr> {
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

    let accounts = query.all(db).await?;

    if accounts.is_empty() {
        return Ok(Vec::new());
    }

    // Collect all account IDs and addresses
    let account_ids: Vec<i32> = accounts.iter().map(|a| a.id).collect();
    let all_addresses: Vec<Vec<u8>> = accounts.iter().map(|a| a.chain_key.clone()).collect();

    // Add safe addresses if they exist
    for _account in &accounts {
        // TODO(Phase 2-3): Query account_state for safe_address
        if false {}
    }

    // 2. Batch fetch all announcements (1 query)
    let announcements = announcement::Entity::find()
        .filter(announcement::Column::AccountId.is_in(account_ids))
        .all(db)
        .await?;

    // Group announcements by account_id
    let mut announcements_by_account: HashMap<i32, Vec<String>> = HashMap::new();
    for ann in announcements {
        announcements_by_account
            .entry(ann.account_id)
            .or_default()
            .push(ann.multiaddress);
    }

    // 3. Batch fetch all HOPR balances (1 query)
    let hopr_balances = hopr_balance::Entity::find()
        .filter(hopr_balance::Column::Address.is_in(all_addresses.clone()))
        .all(db)
        .await?;

    let hopr_balance_map: HashMap<String, String> = hopr_balances
        .into_iter()
        .map(|b| (address_to_string(&b.address), hopr_balance_to_string(&b.balance)))
        .collect();

    // 4. Batch fetch all native balances (1 query)
    let native_balances = native_balance::Entity::find()
        .filter(native_balance::Column::Address.is_in(all_addresses))
        .all(db)
        .await?;

    let native_balance_map: HashMap<String, String> = native_balances
        .into_iter()
        .map(|b| (address_to_string(&b.address), native_balance_to_string(&b.balance)))
        .collect();

    // 5. Aggregate all data
    let result = accounts
        .into_iter()
        .map(|account| {
            let multi_addresses = announcements_by_account.get(&account.id).cloned().unwrap_or_default();

            // Convert chain_key to string for lookups
            let chain_key_str = address_to_string(&account.chain_key);

            // If no balance record exists in DB, return zero balance (non-nullable in GraphQL schema)
            let account_hopr_balance = hopr_balance_map
                .get(&chain_key_str)
                .cloned()
                .unwrap_or_else(|| hopr_balance_to_string(&[]));

            // If no balance record exists in DB, return zero balance (non-nullable in GraphQL schema)
            let account_native_balance = native_balance_map
                .get(&chain_key_str)
                .cloned()
                .unwrap_or_else(|| native_balance_to_string(&[]));

            // Convert safe_address to string if present
            let safe_address_str = None::<String>;

            let (safe_hopr_balance, safe_native_balance) = if let Some(ref safe_addr_str) = safe_address_str {
                (
                    hopr_balance_map.get(safe_addr_str).cloned(),
                    native_balance_map.get(safe_addr_str).cloned(),
                )
            } else {
                (None, None)
            };

            AggregatedAccount {
                keyid: account.id,
                chain_key: chain_key_str,
                packet_key: account.packet_key,
                account_hopr_balance,
                account_native_balance,
                safe_address: safe_address_str,
                safe_hopr_balance,
                safe_native_balance,
                multi_addresses,
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
