//! Account aggregation utilities with optimized batch loading

use std::collections::HashMap;

use sea_orm::{ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter};

use super::balances::{hopr_balance_to_string, native_balance_to_string};
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
    let mut all_addresses: Vec<String> = accounts.iter().map(|a| a.chain_key.clone()).collect();

    // Add safe addresses if they exist
    for account in &accounts {
        if let Some(ref safe_addr) = account.safe_address {
            all_addresses.push(safe_addr.clone());
        }
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
        .map(|b| (b.address.clone(), hopr_balance_to_string(&b.balance)))
        .collect();

    // 4. Batch fetch all native balances (1 query)
    let native_balances = native_balance::Entity::find()
        .filter(native_balance::Column::Address.is_in(all_addresses))
        .all(db)
        .await?;

    let native_balance_map: HashMap<String, String> = native_balances
        .into_iter()
        .map(|b| (b.address.clone(), native_balance_to_string(&b.balance)))
        .collect();

    // 5. Aggregate all data
    let result = accounts
        .into_iter()
        .map(|account| {
            let multi_addresses = announcements_by_account.get(&account.id).cloned().unwrap_or_default();

            let account_hopr_balance = hopr_balance_map
                .get(&account.chain_key)
                .cloned()
                .unwrap_or_else(|| hopr_primitive_types::prelude::HoprBalance::zero().to_string());

            let account_native_balance = native_balance_map
                .get(&account.chain_key)
                .cloned()
                .unwrap_or_else(|| hopr_primitive_types::prelude::XDaiBalance::zero().to_string());

            let (safe_hopr_balance, safe_native_balance) = if let Some(ref safe_addr) = account.safe_address {
                (
                    hopr_balance_map.get(safe_addr).cloned(),
                    native_balance_map.get(safe_addr).cloned(),
                )
            } else {
                (None, None)
            };

            AggregatedAccount {
                keyid: account.id,
                chain_key: account.chain_key,
                packet_key: account.packet_key,
                account_hopr_balance,
                account_native_balance,
                safe_address: account.safe_address,
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

    // 1. Fetch accounts filtered by chain_key (1 query)
    let accounts = account::Entity::find()
        .filter(account::Column::ChainKey.is_in(addresses))
        .all(db)
        .await?;

    if accounts.is_empty() {
        return Ok(Vec::new());
    }

    // Collect all account IDs and addresses
    let account_ids: Vec<i32> = accounts.iter().map(|a| a.id).collect();
    let mut all_addresses: Vec<String> = accounts.iter().map(|a| a.chain_key.clone()).collect();

    // Add safe addresses if they exist
    for account in &accounts {
        if let Some(ref safe_addr) = account.safe_address {
            all_addresses.push(safe_addr.clone());
        }
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
        .map(|b| (b.address.clone(), hopr_balance_to_string(&b.balance)))
        .collect();

    // 4. Batch fetch all native balances (1 query)
    let native_balances = native_balance::Entity::find()
        .filter(native_balance::Column::Address.is_in(all_addresses))
        .all(db)
        .await?;

    let native_balance_map: HashMap<String, String> = native_balances
        .into_iter()
        .map(|b| (b.address.clone(), native_balance_to_string(&b.balance)))
        .collect();

    // 5. Aggregate all data
    let result = accounts
        .into_iter()
        .map(|account| {
            let multi_addresses = announcements_by_account.get(&account.id).cloned().unwrap_or_default();

            let account_hopr_balance = hopr_balance_map
                .get(&account.chain_key)
                .cloned()
                .unwrap_or_else(|| hopr_primitive_types::prelude::HoprBalance::zero().to_string());

            let account_native_balance = native_balance_map
                .get(&account.chain_key)
                .cloned()
                .unwrap_or_else(|| hopr_primitive_types::prelude::XDaiBalance::zero().to_string());

            let (safe_hopr_balance, safe_native_balance) = if let Some(ref safe_addr) = account.safe_address {
                (
                    hopr_balance_map.get(safe_addr).cloned(),
                    native_balance_map.get(safe_addr).cloned(),
                )
            } else {
                (None, None)
            };

            AggregatedAccount {
                keyid: account.id,
                chain_key: account.chain_key,
                packet_key: account.packet_key,
                account_hopr_balance,
                account_native_balance,
                safe_address: account.safe_address,
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
    let mut all_addresses: Vec<String> = accounts.iter().map(|a| a.chain_key.clone()).collect();

    // Add safe addresses if they exist
    for account in &accounts {
        if let Some(ref safe_addr) = account.safe_address {
            all_addresses.push(safe_addr.clone());
        }
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
        .map(|b| (b.address.clone(), hopr_balance_to_string(&b.balance)))
        .collect();

    // 4. Batch fetch all native balances (1 query)
    let native_balances = native_balance::Entity::find()
        .filter(native_balance::Column::Address.is_in(all_addresses))
        .all(db)
        .await?;

    let native_balance_map: HashMap<String, String> = native_balances
        .into_iter()
        .map(|b| (b.address.clone(), native_balance_to_string(&b.balance)))
        .collect();

    // 5. Aggregate all data
    let result = accounts
        .into_iter()
        .map(|account| {
            let multi_addresses = announcements_by_account.get(&account.id).cloned().unwrap_or_default();

            let account_hopr_balance = hopr_balance_map
                .get(&account.chain_key)
                .cloned()
                .unwrap_or_else(|| hopr_primitive_types::prelude::HoprBalance::zero().to_string());

            let account_native_balance = native_balance_map
                .get(&account.chain_key)
                .cloned()
                .unwrap_or_else(|| hopr_primitive_types::prelude::XDaiBalance::zero().to_string());

            let (safe_hopr_balance, safe_native_balance) = if let Some(ref safe_addr) = account.safe_address {
                (
                    hopr_balance_map.get(safe_addr).cloned(),
                    native_balance_map.get(safe_addr).cloned(),
                )
            } else {
                (None, None)
            };

            AggregatedAccount {
                keyid: account.id,
                chain_key: account.chain_key,
                packet_key: account.packet_key,
                account_hopr_balance,
                account_native_balance,
                safe_address: account.safe_address,
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
