//! GraphQL subscription root and resolver implementations

use std::time::Duration;

use async_graphql::{Context, Result, Subscription};
use async_stream::stream;
use blokli_db_entity::conversions::balances::{hopr_balance_to_string, native_balance_to_string};
use futures::Stream;
use sea_orm::DatabaseConnection;
use tokio::time::sleep;

use crate::types::{Account, Channel, HoprBalance, NativeBalance, TokenValueString};

/// Root subscription object for the GraphQL API
pub struct SubscriptionRoot;

#[Subscription]
impl SubscriptionRoot {
    /// Subscribe to real-time updates of native balances for a specific address
    ///
    /// Provides updates whenever there is a change in the native token balance for the specified account.
    /// Updates are sent immediately when balance changes occur on-chain.
    #[graphql(name = "nativeBalanceUpdated")]
    async fn native_balance_updated(
        &self,
        ctx: &Context<'_>,
        #[graphql(desc = "On-chain address to monitor for balance changes (hexadecimal format)")] address: String,
    ) -> Result<impl Stream<Item = NativeBalance>> {
        let db = ctx.data::<DatabaseConnection>()?.clone();
        let addr = address.clone();

        Ok(stream! {
            loop {
                // TODO: Replace with actual database change notifications
                // For now, poll the database periodically
                sleep(Duration::from_secs(1)).await;

                // Query the latest balance
                if let Ok(Some(balance)) = Self::fetch_native_balance(&db, &addr).await {
                    yield balance;
                }
            }
        })
    }

    /// Subscribe to real-time updates of HOPR balances for a specific address
    ///
    /// Provides updates whenever there is a change in the HOPR token balance for the specified account.
    /// Updates are sent immediately when balance changes occur on-chain.
    #[graphql(name = "hoprBalanceUpdated")]
    async fn hopr_balance_updated(
        &self,
        ctx: &Context<'_>,
        #[graphql(desc = "On-chain address to monitor for balance changes (hexadecimal format)")] address: String,
    ) -> Result<impl Stream<Item = HoprBalance>> {
        let db = ctx.data::<DatabaseConnection>()?.clone();
        let addr = address.clone();

        Ok(stream! {
            loop {
                // TODO: Replace with actual database change notifications
                // For now, poll the database periodically
                sleep(Duration::from_secs(1)).await;

                // Query the latest balance
                if let Ok(Some(balance)) = Self::fetch_hopr_balance(&db, &addr).await {
                    yield balance;
                }
            }
        })
    }

    /// Subscribe to real-time updates of payment channels
    ///
    /// Provides updates whenever there is a change in the state of any payment channel,
    /// including channel opening, balance updates, status changes, and channel closure.
    #[graphql(name = "channelUpdated")]
    async fn channel_updated(&self, ctx: &Context<'_>) -> Result<impl Stream<Item = Channel>> {
        let db = ctx.data::<DatabaseConnection>()?.clone();

        Ok(stream! {
            loop {
                // TODO: Replace with actual database change notifications
                // For now, poll the database periodically
                sleep(Duration::from_secs(1)).await;

                // Query the latest channels
                if let Ok(channels) = Self::fetch_all_channels(&db).await {
                    for channel in channels {
                        yield channel;
                    }
                }
            }
        })
    }

    /// Subscribe to real-time updates of account information
    ///
    /// Provides updates whenever there is a change in account information, including
    /// balance changes, Safe address linking, and multiaddress announcements.
    /// Optional filters can be applied to only receive updates for specific accounts.
    #[graphql(name = "accountUpdated")]
    async fn account_updated(
        &self,
        ctx: &Context<'_>,
        #[graphql(desc = "Filter by account keyid")] keyid: Option<i32>,
        #[graphql(desc = "Filter by packet key (peer ID format)")] packet_key: Option<String>,
        #[graphql(desc = "Filter by chain key (hexadecimal format)")] chain_key: Option<String>,
    ) -> Result<impl Stream<Item = Account>> {
        let db = ctx.data::<DatabaseConnection>()?.clone();

        Ok(stream! {
            loop {
                // TODO: Replace with actual database change notifications
                // For now, poll the database periodically
                sleep(Duration::from_secs(1)).await;

                // Query the latest accounts with filters
                if let Ok(accounts) = Self::fetch_filtered_accounts(&db, keyid, packet_key.clone(), chain_key.clone()).await {
                    for account in accounts {
                        yield account;
                    }
                }
            }
        })
    }
}

// Helper methods for fetching data
impl SubscriptionRoot {
    async fn fetch_native_balance(
        db: &DatabaseConnection,
        address: &str,
    ) -> Result<Option<NativeBalance>, sea_orm::DbErr> {
        use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};

        let balance = blokli_db_entity::native_balance::Entity::find()
            .filter(blokli_db_entity::native_balance::Column::Address.eq(address))
            .one(db)
            .await?;

        Ok(balance.map(NativeBalance::from))
    }

    async fn fetch_hopr_balance(db: &DatabaseConnection, address: &str) -> Result<Option<HoprBalance>, sea_orm::DbErr> {
        use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};

        let balance = blokli_db_entity::hopr_balance::Entity::find()
            .filter(blokli_db_entity::hopr_balance::Column::Address.eq(address))
            .one(db)
            .await?;

        Ok(balance.map(HoprBalance::from))
    }

    async fn fetch_all_channels(db: &DatabaseConnection) -> Result<Vec<Channel>, sea_orm::DbErr> {
        use sea_orm::EntityTrait;

        let channels = blokli_db_entity::channel::Entity::find().all(db).await?;

        Ok(channels.into_iter().map(Channel::from).collect())
    }

    async fn fetch_filtered_accounts(
        db: &DatabaseConnection,
        keyid: Option<i32>,
        packet_key: Option<String>,
        chain_key: Option<String>,
    ) -> Result<Vec<Account>, sea_orm::DbErr> {
        use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};

        // Build query with filters
        let mut query = blokli_db_entity::account::Entity::find();

        if let Some(id) = keyid {
            query = query.filter(blokli_db_entity::account::Column::Id.eq(id));
        }

        if let Some(pk) = packet_key {
            query = query.filter(blokli_db_entity::account::Column::PacketKey.eq(pk));
        }

        if let Some(ck) = chain_key {
            query = query.filter(blokli_db_entity::account::Column::ChainKey.eq(ck));
        }

        let accounts = query.all(db).await?;

        let mut result = Vec::new();

        for account_model in accounts {
            // Fetch announcements for this account
            let announcements = blokli_db_entity::announcement::Entity::find()
                .filter(blokli_db_entity::announcement::Column::AccountId.eq(account_model.id))
                .all(db)
                .await?;

            let multi_addresses: Vec<String> = announcements.into_iter().map(|a| a.multiaddress).collect();

            // Fetch HOPR balance for account's chain_key
            // Returns zero balance if no balance record exists (hopr_balance_to_string(&[]) returns "0")
            let hopr_balance_value = blokli_db_entity::hopr_balance::Entity::find()
                .filter(blokli_db_entity::hopr_balance::Column::Address.eq(&account_model.chain_key))
                .one(db)
                .await?
                .map(|b| hopr_balance_to_string(&b.balance))
                .unwrap_or_else(|| hopr_balance_to_string(&[]));

            // Fetch Native balance for account's chain_key
            // Returns zero balance if no balance record exists (native_balance_to_string(&[]) returns "0")
            let native_balance_value = blokli_db_entity::native_balance::Entity::find()
                .filter(blokli_db_entity::native_balance::Column::Address.eq(&account_model.chain_key))
                .one(db)
                .await?
                .map(|b| native_balance_to_string(&b.balance))
                .unwrap_or_else(|| native_balance_to_string(&[]));

            // Fetch safe balances if safe_address exists
            let (safe_hopr_balance, safe_native_balance) = if let Some(ref safe_addr) = account_model.safe_address {
                let safe_hopr = blokli_db_entity::hopr_balance::Entity::find()
                    .filter(blokli_db_entity::hopr_balance::Column::Address.eq(safe_addr))
                    .one(db)
                    .await?
                    .map(|b| hopr_balance_to_string(&b.balance));

                let safe_native = blokli_db_entity::native_balance::Entity::find()
                    .filter(blokli_db_entity::native_balance::Column::Address.eq(safe_addr))
                    .one(db)
                    .await?
                    .map(|b| native_balance_to_string(&b.balance));

                (safe_hopr, safe_native)
            } else {
                (None, None)
            };

            result.push(Account {
                keyid: account_model.id,
                chain_key: account_model.chain_key,
                packet_key: account_model.packet_key,
                account_hopr_balance: TokenValueString(hopr_balance_value),
                account_native_balance: TokenValueString(native_balance_value),
                safe_address: account_model.safe_address,
                safe_hopr_balance: safe_hopr_balance.map(TokenValueString),
                safe_native_balance: safe_native_balance.map(TokenValueString),
                multi_addresses,
            });
        }

        Ok(result)
    }
}
