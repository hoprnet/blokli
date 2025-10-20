//! GraphQL query root and resolver implementations

use async_graphql::{Context, Object, Result};
use sea_orm::{ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter};

use crate::{
    types::{Account, ChainInfo, Channel, HoprBalance, NativeBalance},
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
                chain_key: agg.chain_key,
                packet_key: agg.packet_key,
                account_hopr_balance: agg.account_hopr_balance,
                account_native_balance: agg.account_native_balance,
                safe_address: agg.safe_address,
                safe_hopr_balance: agg.safe_hopr_balance,
                safe_native_balance: agg.safe_native_balance,
                multi_addresses: agg.multi_addresses,
            })
            .collect();

        Ok(result)
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
        use blokli_db_entity::conversions::balances::balance_to_f64;

        let db = ctx.data::<DatabaseConnection>()?;
        let chain_id = ctx.data::<u64>()?;

        // Fetch chain_info from database (assuming single row with id=1)
        let chain_info = blokli_db_entity::chain_info::Entity::find_by_id(1)
            .one(db)
            .await?
            .ok_or_else(|| async_graphql::Error::new("Chain info not found"))?;

        // Convert ticket_price from 12-byte binary to f64
        let ticket_price = chain_info
            .ticket_price
            .as_ref()
            .map(|bytes| balance_to_f64(bytes))
            .unwrap_or(0.0);

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
