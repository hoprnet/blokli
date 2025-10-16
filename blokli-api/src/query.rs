//! GraphQL query root and resolver implementations

use async_graphql::{Context, Object, Result};
use sea_orm::{ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter};

use crate::types::{Account, Announcement, Channel, HoprBalance, NativeBalance};

/// Root query object for the GraphQL API
pub struct QueryRoot;

#[Object]
impl QueryRoot {
    /// Retrieve all accounts from the database
    ///
    /// Returns a complete list of all accounts. No filtering is available.
    async fn accounts(&self, ctx: &Context<'_>) -> Result<Vec<Account>> {
        let db = ctx.data::<DatabaseConnection>()?;

        let accounts = blokli_db_entity::account::Entity::find().all(db).await?;

        Ok(accounts.into_iter().map(Account::from).collect())
    }

    /// Retrieve all announcements from the database
    ///
    /// Returns a complete list of all announcements. No filtering is available.
    async fn announcements(&self, ctx: &Context<'_>) -> Result<Vec<Announcement>> {
        let db = ctx.data::<DatabaseConnection>()?;

        let announcements = blokli_db_entity::announcement::Entity::find().all(db).await?;

        Ok(announcements.into_iter().map(Announcement::from).collect())
    }

    /// Retrieve channels, optionally filtered by source and/or destination
    ///
    /// If neither source nor destination is provided, returns all channels.
    /// If source is provided, filters channels by source address.
    /// If destination is provided, filters channels by destination address.
    /// Both filters can be combined.
    async fn channels(
        &self,
        ctx: &Context<'_>,
        source: Option<String>,
        destination: Option<String>,
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
    /// The address parameter is required. Returns None if no balance exists for the address.
    async fn hopr_balance(&self, ctx: &Context<'_>, address: String) -> Result<Option<HoprBalance>> {
        let db = ctx.data::<DatabaseConnection>()?;

        let balance = blokli_db_entity::hopr_balance::Entity::find()
            .filter(blokli_db_entity::hopr_balance::Column::Address.eq(address))
            .one(db)
            .await?;

        Ok(balance.map(HoprBalance::from))
    }

    /// Retrieve native token balance for a specific address
    ///
    /// The address parameter is required. Returns None if no balance exists for the address.
    async fn native_balance(&self, ctx: &Context<'_>, address: String) -> Result<Option<NativeBalance>> {
        let db = ctx.data::<DatabaseConnection>()?;

        let balance = blokli_db_entity::native_balance::Entity::find()
            .filter(blokli_db_entity::native_balance::Column::Address.eq(address))
            .one(db)
            .await?;

        Ok(balance.map(NativeBalance::from))
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
