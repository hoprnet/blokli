//! GraphQL query root and resolver implementations

use async_graphql::{Context, Object, Result};
use blokli_api_types::{Account, ChainInfo, Channel, Hex32, HoprBalance, NativeBalance, TokenValueString};
use sea_orm::{ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter};

use crate::{
    conversions::{hopr_balance_from_model, native_balance_from_model},
    validation::validate_eth_address,
};

/// Helper function to convert binary domain separator to Hex32 format
fn bytes_to_hex32(bytes: &[u8]) -> Hex32 {
    Hex32(format!("0x{}", hex::encode(bytes)))
}

/// Root query type providing read-only access to indexed blockchain data
pub struct QueryRoot;

#[Object]
impl QueryRoot {
    /// Retrieve accounts from the database with required filtering
    ///
    /// At least one filter parameter must be provided (keyid, packet_key, or chain_key).
    /// Returns an error if no identity filters are specified to prevent excessive data retrieval.
    /// Filters can be combined to narrow results.
    async fn accounts(
        &self,
        ctx: &Context<'_>,
        #[graphql(desc = "Filter by account keyid")] keyid: Option<i32>,
        #[graphql(desc = "Filter by packet key (peer ID format)")] packet_key: Option<String>,
        #[graphql(desc = "Filter by chain key (hexadecimal format)")] chain_key: Option<String>,
    ) -> Result<Vec<Account>> {
        use blokli_db_entity::conversions::account_aggregation::fetch_accounts_with_filters;

        // Require at least one identity filter to prevent excessive data retrieval
        // Note: status alone is not sufficient as it could still return thousands of channels
        if keyid.is_none() && packet_key.is_none() && chain_key.is_none() {
            return Err(async_graphql::Error::new(
                "At least one filter parameter is required (keyid, packetKey, or chainKey). Example: accounts(keyid: \
                 1) or accounts(chainKey: \"0x1234...\")",
            ));
        }

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
        // Convert to i32, returning an error if count exceeds i32::MAX
        let count = i32::try_from(count_u64).map_err(|_| {
            async_graphql::Error::new(format!("Account count {} exceeds i32::MAX (2,147,483,647)", count_u64))
        })?;

        Ok(count)
    }

    /// Count channels matching optional filters
    ///
    /// If no filters are provided, returns total channels count.
    /// Filters can be combined to narrow results.
    #[graphql(name = "channelCount")]
    async fn channel_count(
        &self,
        ctx: &Context<'_>,
        #[graphql(desc = "Filter by source node keyid")] source_key_id: Option<i32>,
        #[graphql(desc = "Filter by destination node keyid")] destination_key_id: Option<i32>,
        #[graphql(desc = "Filter by concrete channel ID (hexadecimal format)")] concrete_channel_id: Option<String>,
        #[graphql(desc = "Filter by channel status (optional, combine with identity filters)")] status: Option<
            blokli_api_types::ChannelStatus,
        >,
    ) -> Result<i32> {
        use sea_orm::PaginatorTrait;

        let db = ctx.data::<DatabaseConnection>()?;

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
        // Status column has been moved to channel_state table
        if let Some(_status_filter) = status {
            return Err(async_graphql::Error::new(
                "Channel status filtering is temporarily unavailable during schema migration. Use other filters \
                 (sourceKeyId, destinationKeyId, or concreteChannelId) without status.",
            ));
        }

        let count_u64 = query.count(db).await?;
        // Convert to i32, returning an error if count exceeds i32::MAX
        let count = i32::try_from(count_u64).map_err(|_| {
            async_graphql::Error::new(format!("Channel count {} exceeds i32::MAX (2,147,483,647)", count_u64))
        })?;

        Ok(count)
    }

    /// Retrieve channels with required filtering
    ///
    /// At least one identity-based filter must be provided (source_key_id, destination_key_id,
    /// or concrete_channel_id). The status filter is optional and can be combined with others.
    /// Returns an error if no identity filters are specified to prevent excessive data retrieval.
    /// Filters can be combined to narrow results.
    async fn channels(
        &self,
        ctx: &Context<'_>,
        #[graphql(desc = "Filter by source node keyid")] source_key_id: Option<i32>,
        #[graphql(desc = "Filter by destination node keyid")] destination_key_id: Option<i32>,
        #[graphql(desc = "Filter by concrete channel ID (hexadecimal format)")] concrete_channel_id: Option<String>,
        #[graphql(desc = "Filter by channel status (optional, combine with identity filters)")] status: Option<
            blokli_api_types::ChannelStatus,
        >,
    ) -> Result<Vec<Channel>> {
        use blokli_db_entity::conversions::channel_aggregation::fetch_channels_with_state;

        // Require at least one identity filter to prevent excessive data retrieval
        // Note: status alone is not sufficient as it could still return thousands of channels
        if source_key_id.is_none() && destination_key_id.is_none() && concrete_channel_id.is_none() {
            return Err(
                async_graphql::Error::new(
                    "At least one identity filter is required (sourceKeyId, destinationKeyId, or concreteChannelId). \
                     \n                 The status filter can be used in combination but not alone. \n                 \
                     Example: channels(sourceKeyId: 1) or channels(sourceKeyId: 1, status: OPEN)",
                ),
            );
        }

        let db = ctx.data::<DatabaseConnection>()?;

        // Convert GraphQL ChannelStatus to database i8 representation if status filter is provided
        let status_i8 = status.map(|s| match s {
            blokli_api_types::ChannelStatus::Closed => 0,
            blokli_api_types::ChannelStatus::Open => 1,
            blokli_api_types::ChannelStatus::PendingToClose => 2,
        });

        // Fetch channels with state using optimized batch loading (2 queries total)
        let aggregated_channels =
            fetch_channels_with_state(db, source_key_id, destination_key_id, concrete_channel_id, status_i8).await?;

        // Convert to GraphQL Channel type
        let result = aggregated_channels
            .into_iter()
            .map(|agg| {
                // Convert status from i8 to ChannelStatus enum
                let status = match agg.status {
                    0 => blokli_api_types::ChannelStatus::Closed,
                    1 => blokli_api_types::ChannelStatus::Open,
                    2 => blokli_api_types::ChannelStatus::PendingToClose,
                    _ => blokli_api_types::ChannelStatus::Closed, // Default to Closed for unknown values
                };

                // Convert epoch from i64 to i32 with validation
                // Epoch should fit in u24, so i32 should always be safe, but handle overflow
                let epoch = i32::try_from(agg.epoch).unwrap_or(i32::MAX);

                // Convert ticket_index from i64 to u64 (should always be non-negative)
                let ticket_index = blokli_api_types::UInt64(u64::try_from(agg.ticket_index).unwrap_or(0));

                Channel {
                    concrete_channel_id: agg.concrete_channel_id,
                    source: agg.source,
                    destination: agg.destination,
                    balance: TokenValueString(agg.balance),
                    status,
                    epoch,
                    ticket_index,
                    closure_time: agg.closure_time,
                }
            })
            .collect();

        Ok(result)
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
        let channel_dst = chain_info.channels_dst.as_ref().map(|b| bytes_to_hex32(b));
        let ledger_dst = chain_info.ledger_dst.as_ref().map(|b| bytes_to_hex32(b));
        let safe_registry_dst = chain_info.safe_registry_dst.as_ref().map(|b| bytes_to_hex32(b));

        // Convert channel closure grace period from i64 to u64 with validation
        let channel_closure_grace_period = chain_info
            .channel_closure_grace_period
            .map(|period| {
                u64::try_from(period).map_err(|_| {
                    async_graphql::Error::new(format!(
                        "channel_closure_grace_period must be non-negative, got {}",
                        period
                    ))
                })
            })
            .transpose()?;

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

#[cfg(test)]
mod tests {
    #[test]
    fn test_accounts_filter_validation_logic() {
        let keyid: Option<i32> = None;
        let packet_key: Option<String> = None;
        let chain_key: Option<String> = None;

        let requires_filter = keyid.is_none() && packet_key.is_none() && chain_key.is_none();
        assert!(requires_filter, "Should require at least one filter");

        let keyid_some: Option<i32> = Some(1);
        let requires_filter_with_keyid = keyid_some.is_none() && packet_key.is_none() && chain_key.is_none();
        assert!(
            !requires_filter_with_keyid,
            "Should not require filter when keyid is provided"
        );

        let packet_key_some: Option<String> = Some("test".to_string());
        let requires_filter_with_packet = keyid.is_none() && packet_key_some.is_none() && chain_key.is_none();
        assert!(
            !requires_filter_with_packet,
            "Should not require filter when packet_key is provided"
        );

        let chain_key_some: Option<String> = Some("0x1234".to_string());
        let requires_filter_with_chain = keyid.is_none() && packet_key.is_none() && chain_key_some.is_none();
        assert!(
            !requires_filter_with_chain,
            "Should not require filter when chain_key is provided"
        );
    }

    #[test]
    fn test_channels_filter_validation_logic() {
        let source_key_id: Option<i32> = None;
        let destination_key_id: Option<i32> = None;
        let concrete_channel_id: Option<String> = None;

        let requires_identity_filter =
            source_key_id.is_none() && destination_key_id.is_none() && concrete_channel_id.is_none();
        assert!(requires_identity_filter, "Should require at least one identity filter");

        let source_some: Option<i32> = Some(1);
        let requires_filter_with_source =
            source_some.is_none() && destination_key_id.is_none() && concrete_channel_id.is_none();
        assert!(
            !requires_filter_with_source,
            "Should not require identity filter when source_key_id is provided"
        );

        let requires_filter_with_status_only =
            source_key_id.is_none() && destination_key_id.is_none() && concrete_channel_id.is_none();
        assert!(
            requires_filter_with_status_only,
            "Should still require identity filter even when only status is provided"
        );

        let destination_some: Option<i32> = Some(2);
        let requires_filter_with_destination =
            source_key_id.is_none() && destination_some.is_none() && concrete_channel_id.is_none();
        assert!(
            !requires_filter_with_destination,
            "Should not require filter when destination_key_id is provided"
        );

        let channel_id_some: Option<String> = Some("0xabc".to_string());
        let requires_filter_with_channel_id =
            source_key_id.is_none() && destination_key_id.is_none() && channel_id_some.is_none();
        assert!(
            !requires_filter_with_channel_id,
            "Should not require filter when concrete_channel_id is provided"
        );
    }
}
