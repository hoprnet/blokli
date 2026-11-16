//! Batch loaders that serve lookups made at the same moment with one database round trip
//!
//! A GraphQL document can repeat a root field under many aliases, and sibling root fields resolve
//! concurrently. Without batching, every alias runs its own statements on its own pooled
//! connection.

use std::{collections::HashMap, sync::Arc};

use async_graphql::dataloader::{DataLoader, Loader};
use blokli_db_entity::conversions::account_aggregation::{
    AggregatedAccount, fetch_accounts_with_balances_for_addresses,
};
use hopr_types::primitive::{primitives::Address, traits::ToHex};
use sea_orm::{DatabaseConnection, DbErr};

/// Most chain keys one batch may hold; lookups beyond it go into a further batch.
const MAX_ACCOUNTS_BATCH_SIZE: usize = 500;

/// Loads accounts by chain key, for all keys of a batch at once.
///
/// A failed batch fails every lookup in it.
pub struct AccountsByChainKey {
    db: DatabaseConnection,
}

impl Loader<Address> for AccountsByChainKey {
    type Error = Arc<DbErr>;
    type Value = Vec<AggregatedAccount>;

    async fn load(&self, keys: &[Address]) -> Result<HashMap<Address, Self::Value>, Self::Error> {
        let addresses = keys.iter().map(ToHex::to_hex).collect();
        let accounts = fetch_accounts_with_balances_for_addresses(&self.db, addresses)
            .await
            .map_err(Arc::new)?;

        let mut accounts_by_chain_key: HashMap<Address, Self::Value> = HashMap::new();
        for account in accounts {
            let chain_key = Address::from_hex(&account.chain_key)
                .map_err(|error| Arc::new(DbErr::Custom(format!("Invalid address: {error}"))))?;
            accounts_by_chain_key.entry(chain_key).or_default().push(account);
        }

        Ok(accounts_by_chain_key)
    }
}

/// Batch loader for `accounts(chainKey:)` lookups, as stored in the schema context.
///
/// It keeps no results between batches, so every lookup reads the current database state.
pub type AccountsByChainKeyLoader = DataLoader<AccountsByChainKey>;

/// Create the batch loader for `accounts(chainKey:)` lookups on the given database.
pub fn accounts_by_chain_key_loader(db: DatabaseConnection) -> AccountsByChainKeyLoader {
    DataLoader::new(AccountsByChainKey { db }, tokio::spawn).max_batch_size(MAX_ACCOUNTS_BATCH_SIZE)
}
