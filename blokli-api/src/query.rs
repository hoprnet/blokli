//! GraphQL query root and resolver implementations

use async_graphql::{Context, Object, Result};
use blokli_api_types::{
    Account, ChainInfo, Channel, HoprBalance, NativeBalance, OpenedChannelsGraph, TokenValueString,
};
use sea_orm::{ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter};

use crate::{
    conversions::{channel_from_model, hopr_balance_from_model, native_balance_from_model},
    validation::validate_eth_address,
};

/// Root query object for the GraphQL API
pub struct QueryRoot;

#[Object]
impl QueryRoot {
    /// Retrieve accounts from the database, optionally filtered
    ///
    /// If no filters are provided, returns all accounts.
    /// Filters can be combined to narrow results.
    async fn accounts(
        &self,
        ctx: &Context<'_>,
        #[graphql(desc = "Filter by account keyid")] keyid: Option<i32>,
        #[graphql(desc = "Filter by packet key (peer ID format)")] packet_key: Option<String>,
        #[graphql(desc = "Filter by chain key (hexadecimal format)")] chain_key: Option<String>,
    ) -> Result<Vec<Account>> {
        use blokli_db_entity::conversions::account_aggregation::fetch_accounts_with_filters;

        // Validate chain_key before DB access
        if let Some(ref ck) = chain_key {
            validate_eth_address(ck)?;
        }

        let db = ctx.data::<DatabaseConnection>()?;

        // Fetch accounts with optional filters using optimized batch loading (4 queries)
        let aggregated_accounts = fetch_accounts_with_filters(db, keyid, packet_key, chain_key).await?;

        // Convert to GraphQL Account type
        let result = aggregated_accounts
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
                multi_addresses: agg.multi_addresses,
            })
            .collect();

        Ok(result)
    }

    /// Count accounts matching optional filters
    ///
    /// If no filters are provided, returns total account count.
    /// Filters can be combined to narrow results.
    #[graphql(name = "accountCount")]
    async fn account_count(
        &self,
        ctx: &Context<'_>,
        #[graphql(desc = "Filter by account keyid")] keyid: Option<i32>,
        #[graphql(desc = "Filter by packet key (peer ID format)")] packet_key: Option<String>,
        #[graphql(desc = "Filter by chain key (hexadecimal format)")] chain_key: Option<String>,
    ) -> Result<i32> {
        use blokli_db_entity::conversions::balances::string_to_address;
        use sea_orm::PaginatorTrait;

        // Validate chain_key before DB access
        if let Some(ref ck) = chain_key {
            validate_eth_address(ck)?;
        }

        let db = ctx.data::<DatabaseConnection>()?;

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

        // Get count efficiently using SeaORM's paginator
        let count_u64 = query.count(db).await?;
        // If count exceeds i32::MAX, clamp to i32::MAX (GraphQL Int type limitation)
        let count = i32::try_from(count_u64).unwrap_or(i32::MAX);

        Ok(count)
    }

    /// Retrieve the opened channels graph
    ///
    /// Returns all open channels along with the accounts that participate in those channels.
    /// This provides a complete view of the active payment channel network.
    #[graphql(name = "openedChannelsGraph")]
    async fn opened_channels_graph(&self, ctx: &Context<'_>) -> Result<OpenedChannelsGraph> {
        use std::collections::HashSet;

        use blokli_db_entity::conversions::account_aggregation::fetch_accounts_by_keyids;

        let db = ctx.data::<DatabaseConnection>()?;

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
                multi_addresses: agg.multi_addresses,
            })
            .collect();

        Ok(OpenedChannelsGraph { channels, accounts })
    }

    /// Retrieve channels, optionally filtered
    ///
    /// If no filters are provided, returns all channels.
    /// Filters can be combined to narrow results.
    async fn channels(
        &self,
        ctx: &Context<'_>,
        #[graphql(desc = "Filter by source node keyid")] source_key_id: Option<i32>,
        #[graphql(desc = "Filter by destination node keyid")] destination_key_id: Option<i32>,
        #[graphql(desc = "Filter by concrete channel ID (hexadecimal format)")] concrete_channel_id: Option<String>,
    ) -> Result<Vec<Channel>> {
        let db = ctx.data::<DatabaseConnection>()?;

        let mut query = blokli_db_entity::channel::Entity::find();

        // Apply source filter if provided
        if let Some(src_keyid) = source_key_id {
            query = query.filter(blokli_db_entity::channel::Column::Source.eq(src_keyid));
        }

        // Apply destination filter if provided
        if let Some(dst_keyid) = destination_key_id {
            query = query.filter(blokli_db_entity::channel::Column::Destination.eq(dst_keyid));
        }

        // Apply concrete channel ID filter if provided
        if let Some(channel_id) = concrete_channel_id {
            query = query.filter(blokli_db_entity::channel::Column::ConcreteChannelId.eq(channel_id));
        }

        let channels = query.all(db).await?;

        Ok(channels.into_iter().map(channel_from_model).collect())
    }

    /// Retrieve HOPR token balance for a specific address
    ///
    /// Returns None if no balance exists for the address.
    #[graphql(name = "hoprBalance")]
    async fn hopr_balance(
        &self,
        ctx: &Context<'_>,
        #[graphql(desc = "On-chain address to query (hexadecimal format)")] address: String,
    ) -> Result<Option<HoprBalance>> {
        use blokli_db_entity::conversions::balances::string_to_address;

        // Validate address format
        validate_eth_address(&address)?;

        let db = ctx.data::<DatabaseConnection>()?;

        // Convert hex string address to binary for database query
        let binary_address = string_to_address(&address);

        let balance = blokli_db_entity::hopr_balance::Entity::find()
            .filter(blokli_db_entity::hopr_balance::Column::Address.eq(binary_address))
            .one(db)
            .await?;

        Ok(balance.map(hopr_balance_from_model))
    }

    /// Retrieve native token balance for a specific address
    ///
    /// Returns None if no balance exists for the address.
    #[graphql(name = "nativeBalance")]
    async fn native_balance(
        &self,
        ctx: &Context<'_>,
        #[graphql(desc = "On-chain address to query (hexadecimal format)")] address: String,
    ) -> Result<Option<NativeBalance>> {
        use blokli_db_entity::conversions::balances::string_to_address;

        // Validate address format
        validate_eth_address(&address)?;

        let db = ctx.data::<DatabaseConnection>()?;

        // Convert hex string address to binary for database query
        let binary_address = string_to_address(&address);

        let balance = blokli_db_entity::native_balance::Entity::find()
            .filter(blokli_db_entity::native_balance::Column::Address.eq(binary_address))
            .one(db)
            .await?;

        Ok(balance.map(native_balance_from_model))
    }

    /// Retrieve chain information
    #[graphql(name = "chainInfo")]
    async fn chain_info(&self, ctx: &Context<'_>) -> Result<ChainInfo> {
        use blokli_db_entity::conversions::balances::hopr_balance_to_string;

        let db = ctx.data::<DatabaseConnection>()?;
        let chain_id = ctx.data::<u64>()?;

        // Fetch chain_info from database (assuming single row with id=1)
        let chain_info = blokli_db_entity::chain_info::Entity::find_by_id(1)
            .one(db)
            .await?
            .ok_or_else(|| async_graphql::Error::new("Chain info not found"))?;

        // Convert ticket_price from 12-byte binary to human-readable string
        let ticket_price = chain_info
            .ticket_price
            .as_ref()
            .map(|bytes| TokenValueString(hopr_balance_to_string(bytes)))
            .unwrap_or_else(|| TokenValueString(hopr_balance_to_string(&[])));

        // Convert last_indexed_block from i64 to i32 with validation
        let block_number = i32::try_from(chain_info.last_indexed_block).map_err(|_| {
            async_graphql::Error::new(format!(
                "block number {} exceeds i32::MAX",
                chain_info.last_indexed_block
            ))
        })?;

        // Convert chain_id from u64 to i32 with validation
        let chain_id_i32 = i32::try_from(*chain_id)
            .map_err(|_| async_graphql::Error::new(format!("chain ID {} exceeds i32::MAX", chain_id)))?;

        // f32 -> f64 is widening, always safe
        #[allow(clippy::cast_lossless)]
        let min_ticket_winning_probability = chain_info.min_incoming_ticket_win_prob as f64;

        // Convert domain separators from binary to hex strings
        let channel_dst = chain_info
            .channels_dst
            .as_ref()
            .map(|bytes| format!("0x{}", hex::encode(bytes)));

        let ledger_dst = chain_info
            .ledger_dst
            .as_ref()
            .map(|bytes| format!("0x{}", hex::encode(bytes)));

        let safe_registry_dst = chain_info
            .safe_registry_dst
            .as_ref()
            .map(|bytes| format!("0x{}", hex::encode(bytes)));

        // Channel closure grace period - for now it will be None until the migration and indexer populate it
        // TODO: This will be populated once the indexer stores the value from the chain
        let channel_closure_grace_period = None; // Will be: chain_info.channel_closure_grace_period

        Ok(ChainInfo {
            block_number,
            chain_id: chain_id_i32,
            ticket_price,
            min_ticket_winning_probability,
            channel_dst,
            ledger_dst,
            safe_registry_dst,
            channel_closure_grace_period,
        })
    }

    /// Health check endpoint
    ///
    /// Returns "ok" to indicate the service is running
    async fn health(&self) -> &str {
        "ok"
    }

    /// API version information
    ///
    /// Returns the current version of the blokli-api package
    async fn version(&self) -> &str {
        env!("CARGO_PKG_VERSION")
    }
}
