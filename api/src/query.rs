//! GraphQL query root and resolver implementations

use std::sync::Arc;

use async_graphql::{Context, Object, Result};
use blokli_api_types::{
    Account, ChainInfo, Channel, ContractAddressMap, Hex32, HoprBalance, InvalidTransactionIdError, NativeBalance,
    SafeHoprAllowance, TokenValueString, Transaction, TransactionStatus,
};
use blokli_chain_api::transaction_store::TransactionStore;
use blokli_chain_rpc::{HoprIndexerRpcOperations, rpc::RpcOperations};
use blokli_chain_types::ContractAddresses;
use blokli_db_entity::conversions::balances::{hopr_balance_to_string, string_to_address};
use sea_orm::{ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter};

use crate::{mutation::TransactionResult, validation::validate_eth_address};

/// Helper function to convert binary domain separator to Hex32 format
fn bytes_to_hex32(bytes: &[u8]) -> Hex32 {
    Hex32(format!("0x{}", hex::encode(bytes)))
}

/// Helper function to convert Hash to Hex32
fn hash_to_hex32(hash: hopr_crypto_types::types::Hash) -> Hex32 {
    Hex32(format!("0x{}", hex::encode(hash.as_ref())))
}

/// Convert store TransactionStatus to GraphQL TransactionStatus
fn store_status_to_graphql(status: blokli_chain_api::transaction_store::TransactionStatus) -> TransactionStatus {
    use blokli_chain_api::transaction_store::TransactionStatus as StoreStatus;

    match status {
        StoreStatus::Pending => TransactionStatus::Pending,
        StoreStatus::Submitted => TransactionStatus::Submitted,
        StoreStatus::Confirmed => TransactionStatus::Confirmed,
        StoreStatus::Reverted => TransactionStatus::Reverted,
        StoreStatus::Timeout => TransactionStatus::Timeout,
        StoreStatus::ValidationFailed => TransactionStatus::ValidationFailed,
        StoreStatus::SubmissionFailed => TransactionStatus::SubmissionFailed,
    }
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
                safe_address: agg.safe_address,
                multi_addresses: agg.multi_addresses,
                safe_transaction_count: blokli_api_types::UInt64(agg.safe_transaction_count),
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
        #[graphql(desc = "Filter by source node keyid")] source_key_id: Option<i64>,
        #[graphql(desc = "Filter by destination node keyid")] destination_key_id: Option<i64>,
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
    /// This query makes a direct RPC call to the blockchain to get the current HOPR token balance.
    /// No database storage is used - balance is fetched directly from the chain.
    #[graphql(name = "hoprBalance")]
    async fn hopr_balance(
        &self,
        ctx: &Context<'_>,
        #[graphql(desc = "On-chain address to query (hexadecimal format)")] address: String,
    ) -> Result<Option<HoprBalance>> {
        use hopr_primitive_types::primitives::Address;

        // Validate address format
        validate_eth_address(&address)?;

        // Convert hex string address to Address type
        let parsed_address = Address::new(&string_to_address(&address));

        // Get RPC operations from context - using ReqwestClient as the concrete type
        let rpc = ctx.data::<Arc<RpcOperations<blokli_chain_rpc::ReqwestClient>>>()?;

        // Make RPC call to get balance from blockchain
        match rpc.get_hopr_balance(parsed_address).await {
            Ok(balance) => Ok(Some(HoprBalance {
                address,
                balance: TokenValueString(balance.to_string()),
            })),
            Err(e) => Err(async_graphql::Error::new(format!(
                "Failed to query HOPR balance from RPC: {}",
                e
            ))),
        }
    }

    /// Retrieve native token balance for a specific address
    ///
    /// This query makes a direct RPC call to the blockchain to get the current native token (xDAI) balance.
    /// No database storage is used - balance is fetched directly from the chain.
    #[graphql(name = "nativeBalance")]
    async fn native_balance(
        &self,
        ctx: &Context<'_>,
        #[graphql(desc = "On-chain address to query (hexadecimal format)")] address: String,
    ) -> Result<Option<NativeBalance>> {
        use hopr_primitive_types::primitives::Address;

        // Validate address format
        validate_eth_address(&address)?;

        // Convert hex string address to Address type
        let parsed_address = Address::new(&string_to_address(&address));

        // Get RPC operations from context - using ReqwestClient as the concrete type
        let rpc = ctx.data::<Arc<RpcOperations<blokli_chain_rpc::ReqwestClient>>>()?;

        // Make RPC call to get native balance from blockchain
        match rpc.get_xdai_balance(parsed_address).await {
            Ok(balance) => Ok(Some(NativeBalance {
                address,
                balance: TokenValueString(balance.to_string()),
            })),
            Err(e) => Err(async_graphql::Error::new(format!(
                "Failed to query native balance from RPC: {}",
                e
            ))),
        }
    }

    /// Retrieve Safe HOPR token allowance for a specific Safe address
    ///
    /// Returns the wxHOPR token allowance that the specified Safe contract has granted
    /// to the HOPR channels contract.
    ///
    /// **Note:** Currently returns None as indexer/database support is not yet implemented.
    /// Returns None if no allowance data exists for the address.
    #[graphql(name = "safeHoprAllowance")]
    async fn safe_hopr_allowance(
        &self,
        ctx: &Context<'_>,
        #[graphql(desc = "Safe contract address to query (hexadecimal format)")] address: String,
    ) -> Result<Option<SafeHoprAllowance>> {
        // Validate address format
        validate_eth_address(&address)?;

        // Prevent unused variable warning
        let _db = ctx.data::<DatabaseConnection>()?;

        // FIXME: Implement allowance fetching once indexer and database support is added
        // The indexer needs to be extended to fetch and store Safe allowances for arbitrary addresses
        // Database schema needs a table to store allowance data indexed by Safe address
        //
        // Implementation should:
        // 1. Query allowance table by Safe address
        // 2. Return SafeHoprAllowance { address, allowance } if data exists
        // 3. Return None if no allowance data exists for the address

        Ok(None)
    }

    /// Retrieve chain information
    #[graphql(name = "chainInfo")]
    async fn chain_info(&self, ctx: &Context<'_>) -> Result<ChainInfo> {
        let db = ctx.data::<DatabaseConnection>()?;
        let chain_id = ctx.data::<u64>()?;
        let network = ctx.data::<String>()?;
        let contract_addresses = ctx.data::<ContractAddresses>()?;

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
            network: network.clone(),
            ticket_price,
            min_ticket_winning_probability,
            channel_dst,
            contract_addresses: ContractAddressMap::from(contract_addresses),
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

    /// Retrieve transaction status by ID
    ///
    /// Returns the current status of a previously submitted transaction.
    /// Returns Error with code INVALID_TRANSACTION_ID if ID format is invalid.
    /// Returns None if transaction ID is not found.
    async fn transaction(&self, ctx: &Context<'_>, id: async_graphql::ID) -> Result<Option<TransactionResult>> {
        // Parse UUID from ID string
        let uuid = match uuid::Uuid::parse_str(id.as_str()) {
            Ok(uuid) => uuid,
            Err(_) => {
                return Ok(Some(TransactionResult::InvalidId(InvalidTransactionIdError {
                    code: "INVALID_TRANSACTION_ID".to_string(),
                    message: format!("Invalid transaction ID format: {}", id.as_str()),
                    transaction_id: id.to_string(),
                })));
            }
        };

        // Get transaction store from context
        let store = ctx.data::<Arc<TransactionStore>>()?;

        // Try to retrieve the transaction
        match store.get(uuid) {
            Ok(record) => {
                // Convert to GraphQL Transaction type
                let transaction = Transaction {
                    id: record.id.to_string(),
                    status: store_status_to_graphql(record.status),
                    submitted_at: record.submitted_at,
                    transaction_hash: record.transaction_hash.map(hash_to_hex32),
                };
                Ok(Some(TransactionResult::Transaction(transaction)))
            }
            Err(_) => Ok(None), // Transaction not found
        }
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
