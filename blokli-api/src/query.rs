//! GraphQL query root and resolver implementations

use async_graphql::{Context, Object, Result};
use sea_orm::{ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter};

use crate::{
    types::{Account, ChainInfo, Channel, HoprBalance, NativeBalance, OpenedChannelsGraph, TokenValueString},
    validation::validate_eth_address,
};

/// Root query object for the GraphQL API
pub struct QueryRoot;

#[Object]
impl QueryRoot {
    /// Retrieve all accounts from the database
    ///
    /// Returns a complete list of all accounts. No filtering is available.
    async fn accounts(&self, ctx: &Context<'_>) -> Result<Vec<Account>> {
        use blokli_db_entity::conversions::account_aggregation::fetch_accounts_with_balances;

        let db = ctx.data::<DatabaseConnection>()?;

        // Fetch all accounts with optimized batch loading (4 queries instead of 1 + N*5)
        let aggregated_accounts = fetch_accounts_with_balances(db).await?;

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
        let channels: Vec<Channel> = channel_models.iter().map(|m| Channel::from(m.clone())).collect();

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

    /// Retrieve channels, optionally filtered by source and/or destination
    ///
    /// If neither source nor destination is provided, returns all channels.
    /// If source is provided, filters channels by source address.
    /// If destination is provided, filters channels by destination address.
    /// Both filters can be combined to find specific channels.
    async fn channels(
        &self,
        ctx: &Context<'_>,
        #[graphql(desc = "Filter by source node address (hexadecimal format)")] source: Option<String>,
        #[graphql(desc = "Filter by destination node address (hexadecimal format)")] destination: Option<String>,
    ) -> Result<Vec<Channel>> {
        let db = ctx.data::<DatabaseConnection>()?;

        let mut query = blokli_db_entity::channel::Entity::find();

        // Apply source filter if provided
        if let Some(src) = source {
            query = query.filter(blokli_db_entity::channel::Column::Source.eq(src));
        }

        // Apply destination filter if provided
        if let Some(dst) = destination {
            query = query.filter(blokli_db_entity::channel::Column::Destination.eq(dst));
        }

        let channels = query.all(db).await?;

        Ok(channels.into_iter().map(Channel::from).collect())
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
        // Validate address format
        validate_eth_address(&address)?;

        let db = ctx.data::<DatabaseConnection>()?;

        let balance = blokli_db_entity::hopr_balance::Entity::find()
            .filter(blokli_db_entity::hopr_balance::Column::Address.eq(address))
            .one(db)
            .await?;

        Ok(balance.map(HoprBalance::from))
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
        // Validate address format
        validate_eth_address(&address)?;

        let db = ctx.data::<DatabaseConnection>()?;

        let balance = blokli_db_entity::native_balance::Entity::find()
            .filter(blokli_db_entity::native_balance::Column::Address.eq(address))
            .one(db)
            .await?;

        Ok(balance.map(NativeBalance::from))
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

        // Convert last_indexed_block from 8-byte binary to u64, then to i32
        let block_number = if chain_info.last_indexed_block.len() == 8 {
            let bytes: [u8; 8] = chain_info.last_indexed_block.as_slice().try_into().unwrap_or([0u8; 8]);
            u64::from_be_bytes(bytes) as i32
        } else {
            0
        };

        Ok(ChainInfo {
            block_number,
            chain_id: *chain_id as i32,
            ticket_price,
            min_ticket_winning_probability: chain_info.min_incoming_ticket_win_prob as f64,
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
