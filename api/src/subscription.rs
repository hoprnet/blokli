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

use crate::conversions::{
    channel_from_model, channel_status_to_i8, hopr_balance_from_model, native_balance_from_model,
};

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

        if let Some(status_filter) = status {
            query = query.filter(blokli_db_entity::channel::Column::Status.eq(channel_status_to_i8(status_filter)));
        }

        let channels = query.all(db).await?;

        Ok(channels.into_iter().map(channel_from_model).collect())
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

        // Fetch node_info to get the node's safe address and allowance
        let node_info = blokli_db_entity::node_info::Entity::find_by_id(1).one(db).await?;
        let (node_safe_address, node_safe_allowance) = if let Some(info) = node_info {
            (
                info.safe_address.as_ref().map(|addr| address_to_string(addr)),
                Some(hopr_balance_to_string(&info.safe_allowance)),
            )
        } else {
            (None, None)
        };

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
            let safe_address_str = account_model.safe_address.as_ref().map(|addr| address_to_string(addr));

            // Fetch safe balances and allowance if safe_address exists
            let (safe_hopr_balance, safe_native_balance, safe_allowance) = if let Some(ref safe_addr) = account_model.safe_address {
                let safe_hopr = blokli_db_entity::hopr_balance::Entity::find()
                    .filter(blokli_db_entity::hopr_balance::Column::Address.eq(safe_addr.clone()))
                    .one(db)
                    .await?
                    .map(|b| hopr_balance_to_string(&b.balance));

                let safe_native = blokli_db_entity::native_balance::Entity::find()
                    .filter(blokli_db_entity::native_balance::Column::Address.eq(safe_addr.clone()))
                    .one(db)
                    .await?
                    .map(|b| native_balance_to_string(&b.balance));

                // Check if this is the node's own safe address
                let allowance = if node_safe_address.as_ref() == Some(safe_address_str.as_ref().unwrap()) {
                    node_safe_allowance.clone()
                } else {
                    None
                };

                (safe_hopr, safe_native, allowance)
            } else {
                (None, None, None)
            };

            result.push(Account {
                keyid: account_model.id,
                chain_key: chain_key_str,
                packet_key: account_model.packet_key,
                account_hopr_balance: TokenValueString(hopr_balance_value),
                account_native_balance: TokenValueString(native_balance_value),
                safe_address: safe_address_str,
                safe_hopr_balance: safe_hopr_balance.map(TokenValueString),
                safe_native_balance: safe_native_balance.map(TokenValueString),
                safe_allowance: safe_allowance.map(TokenValueString),
                multi_addresses,
            });
        }

        Ok(result)
    }

    async fn fetch_opened_channels_graph(db: &DatabaseConnection) -> Result<OpenedChannelsGraph, sea_orm::DbErr> {
        use std::collections::HashSet;

        use blokli_db_entity::conversions::account_aggregation::fetch_accounts_by_keyids;
        use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};

        // 1. Fetch all OPEN channels (status = 1)
        let channel_models = blokli_db_entity::channel::Entity::find()
            .filter(blokli_db_entity::channel::Column::Status.eq(1))
            .all(db)
            .await?;

        // Convert to GraphQL Channel type
        let channels: Vec<Channel> = channel_models.iter().map(|m| channel_from_model(m.clone())).collect();

        // 2. Collect unique keyids from source and destination
        let mut keyids = HashSet::new();
        for channel in &channel_models {
            keyids.insert(channel.source);
            keyids.insert(channel.destination);
        }

        // 3. Fetch accounts for those keyids with optimized batch loading
        let keyid_vec: Vec<i32> = keyids.into_iter().collect();
        let aggregated_accounts = fetch_accounts_by_keyids(db, keyid_vec).await?;

        // Convert to GraphQL Account type
        let accounts = aggregated_accounts
            .into_iter()
            .map(|agg| Account {
                keyid: agg.keyid,
                chain_key: agg.chain_key,
                packet_key: agg.packet_key,
                account_hopr_balance: TokenValueString(agg.account_hopr_balance),
                account_native_balance: TokenValueString(agg.account_native_balance),
                safe_address: agg.safe_address,
                safe_hopr_balance: agg.safe_hopr_balance.map(TokenValueString),
                safe_native_balance: agg.safe_native_balance.map(TokenValueString),
                safe_allowance: agg.safe_allowance.map(TokenValueString),
                multi_addresses: agg.multi_addresses,
            })
            .collect();

        Ok(OpenedChannelsGraph { channels, accounts })
    }
}
