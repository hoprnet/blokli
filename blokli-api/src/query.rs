//! GraphQL query root and resolver implementations

use async_graphql::{Context, Object, Result};
use sea_orm::{ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter};

use crate::types::{Account, ChainInfo, Channel, HoprBalance, NativeBalance};

/// Root query object for the GraphQL API
pub struct QueryRoot;

#[Object]
impl QueryRoot {
    /// Retrieve all accounts from the database
    ///
    /// Returns a complete list of all accounts. No filtering is available.
    async fn accounts(&self, ctx: &Context<'_>) -> Result<Vec<Account>> {
        let db = ctx.data::<DatabaseConnection>()?;

        // Fetch all accounts
        let accounts = blokli_db_entity::account::Entity::find().all(db).await?;

        let mut result = Vec::new();

        for account_model in accounts {
            // Fetch announcements for this account
            let announcements = blokli_db_entity::announcement::Entity::find()
                .filter(blokli_db_entity::announcement::Column::AccountId.eq(account_model.id))
                .all(db)
                .await?;

            let multi_addresses: Vec<String> = announcements.into_iter().map(|a| a.multiaddress).collect();

            // Fetch HOPR balance for account's chain_key
            let hopr_balance_value = blokli_db_entity::hopr_balance::Entity::find()
                .filter(blokli_db_entity::hopr_balance::Column::Address.eq(&account_model.chain_key))
                .one(db)
                .await?
                .map(|b| {
                    if b.balance.len() == 12 {
                        let mut bytes = [0u8; 16];
                        bytes[4..].copy_from_slice(&b.balance);
                        u128::from_be_bytes(bytes) as f64
                    } else {
                        0.0
                    }
                })
                .unwrap_or(0.0);

            // Fetch Native balance for account's chain_key
            let native_balance_value = blokli_db_entity::native_balance::Entity::find()
                .filter(blokli_db_entity::native_balance::Column::Address.eq(&account_model.chain_key))
                .one(db)
                .await?
                .map(|b| {
                    if b.balance.len() == 12 {
                        let mut bytes = [0u8; 16];
                        bytes[4..].copy_from_slice(&b.balance);
                        u128::from_be_bytes(bytes) as f64
                    } else {
                        0.0
                    }
                })
                .unwrap_or(0.0);

            // Fetch safe balances if safe_address exists
            let (safe_hopr_balance, safe_native_balance) = if let Some(ref safe_addr) = account_model.safe_address {
                let safe_hopr = blokli_db_entity::hopr_balance::Entity::find()
                    .filter(blokli_db_entity::hopr_balance::Column::Address.eq(safe_addr))
                    .one(db)
                    .await?
                    .map(|b| {
                        if b.balance.len() == 12 {
                            let mut bytes = [0u8; 16];
                            bytes[4..].copy_from_slice(&b.balance);
                            u128::from_be_bytes(bytes) as f64
                        } else {
                            0.0
                        }
                    });

                let safe_native = blokli_db_entity::native_balance::Entity::find()
                    .filter(blokli_db_entity::native_balance::Column::Address.eq(safe_addr))
                    .one(db)
                    .await?
                    .map(|b| {
                        if b.balance.len() == 12 {
                            let mut bytes = [0u8; 16];
                            bytes[4..].copy_from_slice(&b.balance);
                            u128::from_be_bytes(bytes) as f64
                        } else {
                            0.0
                        }
                    });

                (safe_hopr, safe_native)
            } else {
                (None, None)
            };

            result.push(Account {
                chain_key: account_model.chain_key,
                packet_key: account_model.packet_key,
                account_hopr_balance: hopr_balance_value,
                account_native_balance: native_balance_value,
                safe_address: account_model.safe_address,
                safe_hopr_balance,
                safe_native_balance,
                multi_addresses,
            });
        }

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
        let db = ctx.data::<DatabaseConnection>()?;

        // Fetch chain_info from database (assuming single row with id=1)
        let chain_info = blokli_db_entity::chain_info::Entity::find_by_id(1)
            .one(db)
            .await?
            .ok_or_else(|| async_graphql::Error::new("Chain info not found"))?;

        // Convert ticket_price from 12-byte binary to f64
        let ticket_price = chain_info
            .ticket_price
            .as_ref()
            .and_then(|bytes| {
                if bytes.len() == 12 {
                    let mut price_bytes = [0u8; 16];
                    price_bytes[4..].copy_from_slice(bytes);
                    Some(u128::from_be_bytes(price_bytes) as f64)
                } else {
                    None
                }
            })
            .unwrap_or(0.0);

        Ok(ChainInfo {
            block_number: 0, // TODO: Get from indexer state or RPC
            chain_id: 0,     // TODO: Get from chain config or RPC
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
