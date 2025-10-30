//! GraphQL subscription root and resolver implementations

use std::time::Duration;

use async_graphql::{Context, Result, Subscription};
use async_stream::stream;
use blokli_api_types::{Account, Channel, HoprBalance, NativeBalance, OpenedChannelsGraph, TokenValueString};
use blokli_db_entity::conversions::balances::{
    address_to_string, hopr_balance_to_string, native_balance_to_string, string_to_address,
};
use futures::Stream;
use sea_orm::DatabaseConnection;
use tokio::time::sleep;

use crate::conversions::{hopr_balance_from_model, native_balance_from_model};

/// Root subscription type providing real-time updates via Server-Sent Events (SSE)
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
    /// Optional filters can be applied to only receive updates for specific channels.
    #[graphql(name = "channelUpdated")]
    async fn channel_updated(
        &self,
        ctx: &Context<'_>,
        #[graphql(desc = "Filter by source node keyid")] source_key_id: Option<i32>,
        #[graphql(desc = "Filter by destination node keyid")] destination_key_id: Option<i32>,
        #[graphql(desc = "Filter by concrete channel ID (hexadecimal format)")] concrete_channel_id: Option<String>,
        #[graphql(desc = "Filter by channel status")] status: Option<blokli_api_types::ChannelStatus>,
    ) -> Result<impl Stream<Item = Channel>> {
        let db = ctx.data::<DatabaseConnection>()?.clone();

        Ok(stream! {
            loop {
                // TODO: Replace with actual database change notifications
                // For now, poll the database periodically
                sleep(Duration::from_secs(1)).await;

                // Query the latest channels with filters
                if let Ok(channels) = Self::fetch_filtered_channels(&db, source_key_id, destination_key_id, concrete_channel_id.clone(), status).await {
                    for channel in channels {
                        yield channel;
                    }
                }
            }
        })
    }

    /// Subscribe to a full stream of existing channels and channel updates.
    ///
    /// Provides channel information on all open channels along with the accounts that participate in those channels.
    /// This provides a complete view of the active payment channel network.
    #[graphql(name = "openedChannelGraphUpdated")]
    async fn opened_channel_graph_updated(&self, ctx: &Context<'_>) -> Result<impl Stream<Item = OpenedChannelsGraph>> {
        let db = ctx.data::<DatabaseConnection>()?.clone();

        Ok(stream! {
            loop {
                // TODO: Replace with actual database change notifications
                // For now, poll the database periodically
                sleep(Duration::from_secs(1)).await;

                // Query the opened channels graph
                if let Ok(graph) = Self::fetch_opened_channels_graph(&db).await {
                    yield graph;
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

        // Convert hex string address to binary for database query
        let binary_address = string_to_address(address);

        let balance = blokli_db_entity::native_balance::Entity::find()
            .filter(blokli_db_entity::native_balance::Column::Address.eq(binary_address))
            .one(db)
            .await?;

        Ok(balance.map(native_balance_from_model))
    }

    async fn fetch_hopr_balance(db: &DatabaseConnection, address: &str) -> Result<Option<HoprBalance>, sea_orm::DbErr> {
        use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};

        // Convert hex string address to binary for database query
        let binary_address = string_to_address(address);

        let balance = blokli_db_entity::hopr_balance::Entity::find()
            .filter(blokli_db_entity::hopr_balance::Column::Address.eq(binary_address))
            .one(db)
            .await?;

        Ok(balance.map(hopr_balance_from_model))
    }

    async fn fetch_filtered_channels(
        db: &DatabaseConnection,
        source_key_id: Option<i32>,
        destination_key_id: Option<i32>,
        concrete_channel_id: Option<String>,
        status: Option<blokli_api_types::ChannelStatus>,
    ) -> Result<Vec<Channel>, sea_orm::DbErr> {
        use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};

        // Build query with filters
        let mut query = blokli_db_entity::channel::Entity::find();

        if let Some(src_keyid) = source_key_id {
            query = query.filter(blokli_db_entity::channel::Column::Source.eq(src_keyid));
        }

        if let Some(dst_keyid) = destination_key_id {
            query = query.filter(blokli_db_entity::channel::Column::Destination.eq(dst_keyid));
        }

        if let Some(channel_id) = concrete_channel_id {
            query = query.filter(blokli_db_entity::channel::Column::ConcreteChannelId.eq(channel_id));
        }

        // TODO(Phase 2-3): Status filtering requires querying channel_state table
        if let Some(_status_filter) = status {
            return Err(sea_orm::DbErr::Custom(
                "Channel status filtering temporarily unavailable during schema migration".to_string(),
            ));
        }

        let _channels = query.all(db).await?;

        // TODO(Phase 2-3): Cannot use channel_from_model until we implement channel_state lookup
        Err(sea_orm::DbErr::Custom(
            "Channel subscription temporarily unavailable during schema migration".to_string(),
        ))
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
            // Convert hex string address to binary for database query
            let binary_chain_key = string_to_address(&ck);
            query = query.filter(blokli_db_entity::account::Column::ChainKey.eq(binary_chain_key));
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
                .filter(blokli_db_entity::hopr_balance::Column::Address.eq(account_model.chain_key.clone()))
                .one(db)
                .await?
                .map(|b| hopr_balance_to_string(&b.balance))
                .unwrap_or_else(|| hopr_balance_to_string(&[]));

            // Fetch Native balance for account's chain_key
            // Returns zero balance if no balance record exists (native_balance_to_string(&[]) returns "0")
            let native_balance_value = blokli_db_entity::native_balance::Entity::find()
                .filter(blokli_db_entity::native_balance::Column::Address.eq(account_model.chain_key.clone()))
                .one(db)
                .await?
                .map(|b| native_balance_to_string(&b.balance))
                .unwrap_or_else(|| native_balance_to_string(&[]));

            // Convert addresses to hex strings for GraphQL response
            let chain_key_str = address_to_string(&account_model.chain_key);

            // TODO(Phase 2-3): Query account_state table for safe_address
            // safe_address has been moved to account_state table
            let safe_address_str = None::<String>;

            // TODO(Phase 2-3): Fetch safe balances once safe_address is available from account_state
            let (safe_hopr_balance, safe_native_balance) = (None, None);

            result.push(Account {
                keyid: account_model.id,
                chain_key: chain_key_str,
                packet_key: account_model.packet_key,
                account_hopr_balance: TokenValueString(hopr_balance_value),
                account_native_balance: TokenValueString(native_balance_value),
                safe_address: safe_address_str,
                safe_hopr_balance: safe_hopr_balance.map(TokenValueString),
                safe_native_balance: safe_native_balance.map(TokenValueString),
                multi_addresses,
            });
        }

        Ok(result)
    }

    async fn fetch_opened_channels_graph(_db: &DatabaseConnection) -> Result<OpenedChannelsGraph, sea_orm::DbErr> {
        // TODO(Phase 2-3): This function requires querying channel_state table for status
        // For now, return empty graph until we implement the channel_state lookup
        // Original logic: Fetch all OPEN channels (status = 1) and their participating accounts

        // Temporary: return empty graph
        let channels: Vec<Channel> = Vec::new();
        let accounts: Vec<Account> = Vec::new();

        Ok(OpenedChannelsGraph { channels, accounts })
    }
}
