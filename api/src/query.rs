//! GraphQL query root and resolver implementations

use std::sync::Arc;

use async_graphql::{Context, ID, Object, Result, SimpleObject, Union};
use blokli_api_types::{
    Account, AccountsList, AccountsResult, ChainInfo, ChainInfoResult, Channel, ChannelsList, ChannelsResult,
    ContractAddressMap, CountResult, HoprBalance, InvalidAddressError, ModuleAddress, NativeBalance, QueryFailedError,
    Safe, SafeHoprAllowance, SafeTransactionCount, TokenValueString, Transaction, UInt64,
};
use blokli_chain_api::transaction_store::TransactionStore;
use blokli_chain_rpc::{HoprIndexerRpcOperations, rpc::RpcOperations};
use blokli_chain_types::ContractAddresses;
use blokli_db_entity::conversions::{
    account_aggregation::fetch_accounts_with_filters, channel_aggregation::fetch_channels_with_state,
};
use hopr_crypto_types::prelude::Hash;
use hopr_primitive_types::{
    prelude::HoprBalance as PrimitiveHoprBalance,
    primitives::Address,
    traits::{IntoEndian, ToHex},
};
use sea_orm::{ColumnTrait, DatabaseConnection, EntityTrait, PaginatorTrait, QueryFilter};

use crate::{errors, mutation::TransactionResult, validation::validate_eth_address};

/// Result type for HOPR balance queries
#[derive(Union)]
pub enum HoprBalanceResult {
    Balance(HoprBalance),
    InvalidAddress(InvalidAddressError),
    QueryFailed(QueryFailedError),
}

/// Result type for native balance queries
#[derive(Union)]
pub enum NativeBalanceResult {
    Balance(NativeBalance),
    InvalidAddress(InvalidAddressError),
    QueryFailed(QueryFailedError),
}

/// Result type for Safe HOPR allowance queries
#[derive(Union)]
pub enum SafeHoprAllowanceResult {
    Allowance(SafeHoprAllowance),
    InvalidAddress(InvalidAddressError),
    QueryFailed(QueryFailedError),
}

/// Result type for Safe transaction count queries
#[derive(Union)]
pub enum SafeTransactionCountResult {
    TransactionCount(SafeTransactionCount),
    InvalidAddress(InvalidAddressError),
    QueryFailed(QueryFailedError),
}

/// Result type for single safe queries
#[derive(Union)]
pub enum SafeResult {
    Safe(Safe),
    InvalidAddress(InvalidAddressError),
    QueryFailed(QueryFailedError),
}

/// Success response for safes list query
#[derive(SimpleObject)]
pub struct SafesList {
    /// List of safes
    pub safes: Vec<Safe>,
}

/// Result type for safes list query
#[derive(Union)]
pub enum SafesResult {
    Safes(SafesList),
    QueryFailed(QueryFailedError),
}

/// Result type for module address calculation
#[derive(Union)]
pub enum CalculateModuleAddressResult {
    ModuleAddress(ModuleAddress),
    InvalidAddress(InvalidAddressError),
    QueryFailed(QueryFailedError),
}

/// Validate an Ethereum hex address and return its 20-byte binary form.
///
/// Parses and validates `address` (expected as a hex string, e.g. starting with `0x`); on success returns the address
/// bytes suitable for database queries, otherwise returns `SafeResult::InvalidAddress` describing the validation error.
///
/// # Examples
///
/// ```ignore
/// let res = parse_safe_address("0x0123456789abcdef0123456789abcdef01234567".to_string());
/// assert!(res.is_ok());
/// let bytes = res.unwrap();
/// assert_eq!(bytes.len(), 20);
/// ```
fn parse_safe_address(address: String) -> std::result::Result<Vec<u8>, SafeResult> {
    // Validate address format
    if let Err(e) = validate_eth_address(&address) {
        return Err(SafeResult::InvalidAddress(errors::invalid_address_from_message(
            address, e.message,
        )));
    }

    // Convert hex string to Address and then to binary
    Address::from_hex(&address)
        .map(|addr| addr.as_ref().to_vec())
        .map_err(|e| SafeResult::InvalidAddress(errors::invalid_address_error(address, e)))
}

/// Helper function to convert database Safe model to GraphQL Safe type
///
/// Validates that all address fields in the database are exactly 20 bytes.
/// Returns an error message if any address field has an invalid length.
fn safe_from_db_model(safe: blokli_db_entity::hopr_safe_contract::Model) -> std::result::Result<Safe, String> {
    let address = Address::try_from(&safe.address[..]).map_err(|_| {
        format!(
            "Invalid address length in database: expected 20 bytes, got {}",
            safe.address.len()
        )
    })?;

    let module_address = Address::try_from(&safe.module_address[..]).map_err(|_| {
        format!(
            "Invalid module address length in database: expected 20 bytes, got {}",
            safe.module_address.len()
        )
    })?;

    let chain_key = Address::try_from(&safe.chain_key[..]).map_err(|_| {
        format!(
            "Invalid chain key length in database: expected 20 bytes, got {}",
            safe.chain_key.len()
        )
    })?;

    Ok(Safe {
        address: address.to_hex(),
        module_address: module_address.to_hex(),
        chain_key: chain_key.to_hex(),
    })
}

/// Root query type providing read-only access to indexed blockchain data
pub struct QueryRoot;

#[Object]
impl QueryRoot {
    /// Retrieve accounts from the database with required filtering
    ///
    /// At least one filter parameter must be provided (keyid, packet_key, or chain_key).
    /// Returns a union type indicating success or specific error conditions.
    /// Filters can be combined to narrow results.
    async fn accounts(
        &self,
        ctx: &Context<'_>,
        #[graphql(desc = "Filter by account keyid")] keyid: Option<i64>,
        #[graphql(desc = "Filter by packet key (peer ID format)")] packet_key: Option<String>,
        #[graphql(desc = "Filter by chain key (hexadecimal format)")] chain_key: Option<String>,
    ) -> AccountsResult {
        // Require at least one identity filter to prevent excessive data retrieval
        if keyid.is_none() && packet_key.is_none() && chain_key.is_none() {
            return AccountsResult::MissingFilter(errors::missing_filter_error(
                "keyid, packetKey, or chainKey",
                "accounts query. Example: accounts(keyid: 1) or accounts(chainKey: \"0x1234...\")",
            ));
        }

        // Validate chain_key before DB access
        if let Some(ref ck) = chain_key
            && let Err(e) = validate_eth_address(ck)
        {
            return AccountsResult::QueryFailed(errors::invalid_address_query_failed(e.message));
        }

        let db = match ctx.data::<DatabaseConnection>() {
            Ok(db) => db,
            Err(e) => {
                return AccountsResult::QueryFailed(errors::db_connection_error(format!("{:?}", e)));
            }
        };

        // Fetch accounts with optional filters using optimized batch loading (4 queries)
        let aggregated_accounts = match fetch_accounts_with_filters(db, keyid, packet_key, chain_key).await {
            Ok(accounts) => accounts,
            Err(e) => {
                return AccountsResult::QueryFailed(errors::query_failed("fetch accounts", e));
            }
        };

        // Convert to GraphQL Account type
        let accounts = aggregated_accounts
            .into_iter()
            .map(|agg| Account {
                keyid: agg.keyid,
                chain_key: agg.chain_key,
                packet_key: agg.packet_key,
                safe_address: agg.safe_address,
                multi_addresses: agg.multi_addresses,
            })
            .collect();

        AccountsResult::Accounts(AccountsList { accounts })
    }

    /// Count accounts matching optional filters
    ///
    /// If no filters are provided, returns total account count.
    /// Filters can be combined to narrow results.
    #[graphql(name = "accountCount")]
    async fn account_count(
        &self,
        ctx: &Context<'_>,
        #[graphql(desc = "Filter by account keyid")] keyid: Option<i64>,
        #[graphql(desc = "Filter by packet key (peer ID format)")] packet_key: Option<String>,
        #[graphql(desc = "Filter by chain key (hexadecimal format)")] chain_key: Option<String>,
    ) -> CountResult {
        // Validate chain_key before DB access
        if let Some(ref ck) = chain_key
            && let Err(e) = validate_eth_address(ck)
        {
            return CountResult::QueryFailed(errors::invalid_address_query_failed(e.message));
        }

        let db = match ctx.data::<DatabaseConnection>() {
            Ok(db) => db,
            Err(e) => {
                return CountResult::QueryFailed(errors::db_connection_error(format!("{:?}", e)));
            }
        };

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
            let binary_chain_key = match Address::from_hex(&ck) {
                Ok(addr) => addr.as_ref().to_vec(),
                Err(e) => {
                    return CountResult::QueryFailed(errors::invalid_address_query_failed(format!(
                        "Invalid address: {}",
                        e
                    )));
                }
            };
            query = query.filter(blokli_db_entity::account::Column::ChainKey.eq(binary_chain_key));
        }

        // Get count efficiently using SeaORM's paginator
        let count_u64 = match query.count(db).await {
            Ok(count) => count,
            Err(e) => {
                return CountResult::QueryFailed(errors::query_failed("count accounts", e));
            }
        };

        // Convert to i32, returning an error if count exceeds i32::MAX
        let count = match i32::try_from(count_u64) {
            Ok(c) => c,
            Err(_) => {
                return CountResult::QueryFailed(errors::overflow_error("account count", count_u64.to_string()));
            }
        };

        CountResult::Count(blokli_api_types::Count { count })
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
    ) -> CountResult {
        let db = match ctx.data::<DatabaseConnection>() {
            Ok(db) => db,
            Err(e) => {
                return CountResult::QueryFailed(errors::db_connection_error(format!("{:?}", e)));
            }
        };

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
            return CountResult::QueryFailed(errors::not_implemented(
                "Channel status filtering during schema migration",
            ));
        }

        let count_u64 = match query.count(db).await {
            Ok(count) => count,
            Err(e) => {
                return CountResult::QueryFailed(errors::query_failed("count channels", e));
            }
        };

        // Convert to i32, returning an error if count exceeds i32::MAX
        let count = match i32::try_from(count_u64) {
            Ok(c) => c,
            Err(_) => {
                return CountResult::QueryFailed(errors::overflow_error("channel count", count_u64.to_string()));
            }
        };

        CountResult::Count(blokli_api_types::Count { count })
    }

    /// Retrieve channels with required filtering
    ///
    /// At least one identity-based filter must be provided (source_key_id, destination_key_id,
    /// or concrete_channel_id). The status filter is optional and can be combined with others.
    /// Returns a union type indicating success or specific error conditions.
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
    ) -> ChannelsResult {
        // Require at least one identity filter to prevent excessive data retrieval
        // Note: status alone is not sufficient as it could still return thousands of channels
        if source_key_id.is_none() && destination_key_id.is_none() && concrete_channel_id.is_none() {
            return ChannelsResult::MissingFilter(errors::missing_filter_error(
                "sourceKeyId, destinationKeyId, or concreteChannelId",
                "channels query. The status filter can be used in combination but not alone. Example: \
                 channels(sourceKeyId: 1) or channels(sourceKeyId: 1, status: OPEN)",
            ));
        }

        let db = match ctx.data::<DatabaseConnection>() {
            Ok(db) => db,
            Err(e) => {
                return ChannelsResult::QueryFailed(errors::db_connection_error(format!("{:?}", e)));
            }
        };

        // Convert GraphQL ChannelStatus to database i8 representation if status filter is provided
        let status_i8 = status.map(|s| match s {
            blokli_api_types::ChannelStatus::Closed => 0,
            blokli_api_types::ChannelStatus::Open => 1,
            blokli_api_types::ChannelStatus::PendingToClose => 2,
        });

        // Fetch channels with state using optimized batch loading (2 queries total)
        let aggregated_channels = match fetch_channels_with_state(
            db,
            source_key_id,
            destination_key_id,
            concrete_channel_id,
            status_i8,
        )
        .await
        {
            Ok(channels) => channels,
            Err(e) => {
                return ChannelsResult::QueryFailed(errors::query_failed("fetch channels", e));
            }
        };

        // Convert to GraphQL Channel type
        let channels: Vec<Channel> = aggregated_channels
            .into_iter()
            .filter_map(|agg| {
                // Convert status from i8 to ChannelStatus enum
                let status = match agg.status {
                    0 => blokli_api_types::ChannelStatus::Closed,
                    1 => blokli_api_types::ChannelStatus::Open,
                    2 => blokli_api_types::ChannelStatus::PendingToClose,
                    _ => {
                        // Skip invalid status values
                        return None;
                    }
                };

                // Convert epoch from i64 to i32 with validation
                let epoch = match i32::try_from(agg.epoch) {
                    Ok(e) => e,
                    Err(_) => {
                        // Skip channels with out-of-range epochs
                        return None;
                    }
                };

                // Convert ticket_index from i64 to u64 (should always be non-negative)
                let ticket_index = match u64::try_from(agg.ticket_index) {
                    Ok(ti) => blokli_api_types::UInt64(ti),
                    Err(_) => {
                        // Skip channels with negative ticket indices
                        return None;
                    }
                };

                Some(Channel {
                    concrete_channel_id: agg.concrete_channel_id,
                    source: agg.source,
                    destination: agg.destination,
                    balance: TokenValueString(agg.balance),
                    status,
                    epoch,
                    ticket_index,
                    closure_time: agg.closure_time,
                })
            })
            .collect();

        ChannelsResult::Channels(ChannelsList { channels })
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
    ) -> Result<HoprBalanceResult> {
        // Validate address format
        if let Err(e) = validate_eth_address(&address) {
            return Ok(HoprBalanceResult::InvalidAddress(errors::invalid_address_from_message(
                address, e.message,
            )));
        }

        // Convert hex string address to Address type
        let parsed_address =
            Address::from_hex(&address).map_err(|e| async_graphql::Error::new(format!("Invalid address: {}", e)))?;

        // Get RPC operations from context - using ReqwestClient as the concrete type
        let rpc = ctx.data::<Arc<RpcOperations<blokli_chain_rpc::ReqwestClient>>>()?;

        // Make RPC call to get balance from blockchain
        match rpc.get_hopr_balance(parsed_address).await {
            Ok(balance) => Ok(HoprBalanceResult::Balance(HoprBalance {
                address,
                balance: TokenValueString(balance.to_string()),
            })),
            Err(e) => Ok(HoprBalanceResult::QueryFailed(errors::rpc_query_failed(
                "query HOPR balance",
                e,
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
    ) -> Result<NativeBalanceResult> {
        // Validate address format
        if let Err(e) = validate_eth_address(&address) {
            return Ok(NativeBalanceResult::InvalidAddress(
                errors::invalid_address_from_message(address, e.message),
            ));
        }

        // Convert hex string address to Address type
        let parsed_address =
            Address::from_hex(&address).map_err(|e| async_graphql::Error::new(format!("Invalid address: {}", e)))?;

        // Get RPC operations from context - using ReqwestClient as the concrete type
        let rpc = ctx.data::<Arc<RpcOperations<blokli_chain_rpc::ReqwestClient>>>()?;

        // Make RPC call to get native balance from blockchain
        match rpc.get_xdai_balance(parsed_address).await {
            Ok(balance) => Ok(NativeBalanceResult::Balance(NativeBalance {
                address,
                balance: TokenValueString(balance.to_string()),
            })),
            Err(e) => Ok(NativeBalanceResult::QueryFailed(errors::rpc_query_failed(
                "query native balance",
                e,
            ))),
        }
    }

    /// Retrieve Safe HOPR token allowance for a specific Safe address
    ///
    /// Returns the wxHOPR token allowance that the specified Safe contract has granted
    /// to the HOPR channels contract.
    ///
    /// This query makes a direct RPC call to the blockchain to get the current allowance.
    /// No database storage is used - allowance is fetched directly from the chain.
    #[graphql(name = "safeHoprAllowance")]
    async fn safe_hopr_allowance(
        &self,
        ctx: &Context<'_>,
        #[graphql(desc = "Safe contract address to query (hexadecimal format)")] address: String,
    ) -> Result<SafeHoprAllowanceResult> {
        // Validate address format
        if let Err(e) = validate_eth_address(&address) {
            return Ok(SafeHoprAllowanceResult::InvalidAddress(
                errors::invalid_address_from_message(address, e.message),
            ));
        }

        // Convert hex string address to Address type
        let safe_address =
            Address::from_hex(&address).map_err(|e| async_graphql::Error::new(format!("Invalid address: {}", e)))?;

        // Get contract addresses from context to access channels contract
        let contract_addresses = ctx.data::<ContractAddresses>()?;
        let channels_address = contract_addresses.channels;

        // Get RPC operations from context - using ReqwestClient as the concrete type
        let rpc = ctx.data::<Arc<RpcOperations<blokli_chain_rpc::ReqwestClient>>>()?;

        // Make RPC call to get allowance from blockchain
        match rpc.get_hopr_allowance(safe_address, channels_address).await {
            Ok(allowance) => Ok(SafeHoprAllowanceResult::Allowance(SafeHoprAllowance {
                address,
                allowance: TokenValueString(allowance.to_string()),
            })),
            Err(e) => Ok(SafeHoprAllowanceResult::QueryFailed(errors::rpc_query_failed(
                "query HOPR allowance",
                e,
            ))),
        }
    }

    /// Fetches the current transaction count (nonce) for a Safe contract address from the blockchain.
    ///
    /// The `address` must be a hexadecimal Ethereum address. The resolver validates the address format,
    /// queries the blockchain RPC for the Safe's transaction count, and returns a `SafeTransactionCountResult`
    /// that indicates success, an invalid address error, or a query failure.
    ///
    /// # Returns
    ///
    /// - `SafeTransactionCountResult::TransactionCount` containing the queried `address` and the `count` on success.
    /// - `SafeTransactionCountResult::InvalidAddress` if the provided address is not a valid hexadecimal Ethereum
    ///   address.
    /// - `SafeTransactionCountResult::QueryFailed` if the RPC call fails.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// # use api::query::SafeTransactionCountResult;
    /// # use api::query::SafeTransactionCount;
    /// # use api::query::UInt64;
    /// // Suppose `res` is the value returned by `safe_transaction_count`.
    /// let res: SafeTransactionCountResult = SafeTransactionCountResult::TransactionCount(SafeTransactionCount {
    ///     address: "0x0000000000000000000000000000000000000000".to_string(),
    ///     count: UInt64(42),
    /// });
    ///
    /// match res {
    ///     SafeTransactionCountResult::TransactionCount(tc) => {
    ///         assert_eq!(tc.count.0, 42);
    ///         assert_eq!(tc.address, "0x0000000000000000000000000000000000000000");
    ///     }
    ///     SafeTransactionCountResult::InvalidAddress(err) => panic!("invalid address: {}", err.message),
    ///     SafeTransactionCountResult::QueryFailed(err) => panic!("query failed: {}", err.message),
    /// }
    /// ```
    async fn safe_transaction_count(
        &self,
        ctx: &Context<'_>,
        #[graphql(desc = "Safe contract address to query (hexadecimal format)")] address: String,
    ) -> Result<SafeTransactionCountResult> {
        // Validate address format
        if let Err(e) = validate_eth_address(&address) {
            return Ok(SafeTransactionCountResult::InvalidAddress(
                errors::invalid_address_from_message(address, e.message),
            ));
        }

        // Convert hex string address to Address type
        let safe_address =
            Address::from_hex(&address).map_err(|e| async_graphql::Error::new(format!("Invalid address: {}", e)))?;

        // Get RPC operations from context - using ReqwestClient as the concrete type
        let rpc = ctx.data::<Arc<RpcOperations<blokli_chain_rpc::ReqwestClient>>>()?;

        // Make RPC call to get transaction count from blockchain
        match rpc.get_safe_transaction_count(safe_address).await {
            Ok(count) => Ok(SafeTransactionCountResult::TransactionCount(SafeTransactionCount {
                address,
                count: UInt64(count),
            })),
            Err(e) => Ok(SafeTransactionCountResult::QueryFailed(errors::rpc_query_failed(
                "query Safe transaction count",
                e,
            ))),
        }
    }

    /// Fetches a Safe by its contract address.
    ///
    /// Validates the provided hexadecimal address, queries the database for a matching safe contract,
    /// and returns a GraphQL-safe result wrapper indicating success, validation failure, or query failure.
    /// The function returns `None` when no safe with the given address exists.
    ///
    /// # Returns
    ///
    /// - `Some(SafeResult::Safe)` with the found safe on success.
    /// - `Some(SafeResult::InvalidAddress)` when the address format is invalid.
    /// - `Some(SafeResult::QueryFailed)` when the database query fails.
    /// - `None` when no safe is found for the given address.
    ///
    /// # Examples
    ///
    /// ```
    /// // Example usage (executed in an async context with a prepared `ctx`):
    /// // let res = query_root.safe(&ctx, "0x0123...abcd".to_string()).await?;
    /// // match res {
    /// //     Some(SafeResult::Safe(s)) => println!("Found safe: {}", s.address),
    /// //     Some(SafeResult::InvalidAddress(err)) => eprintln!("Invalid address: {}", err.message),
    /// //     Some(SafeResult::QueryFailed(err)) => eprintln!("Query failed: {}", err.message),
    /// //     None => println!("Safe not found"),
    /// // }
    /// ```
    async fn safe(
        &self,
        ctx: &Context<'_>,
        #[graphql(desc = "Safe contract address to query (hexadecimal format)")] address: String,
    ) -> Result<Option<SafeResult>> {
        let safe_address = match parse_safe_address(address) {
            Ok(addr) => addr,
            Err(error_result) => return Ok(Some(error_result)),
        };

        let db = ctx.data::<DatabaseConnection>()?;

        match blokli_db_entity::hopr_safe_contract::Entity::find()
            .filter(blokli_db_entity::hopr_safe_contract::Column::Address.eq(safe_address))
            .one(db)
            .await
        {
            Ok(Some(safe)) => match safe_from_db_model(safe) {
                Ok(safe_data) => Ok(Some(SafeResult::Safe(safe_data))),
                Err(e) => Ok(Some(SafeResult::QueryFailed(errors::invalid_db_data(
                    "safe addresses",
                    &e,
                )))),
            },
            Ok(None) => Ok(None),
            Err(e) => Ok(Some(SafeResult::QueryFailed(errors::query_failed("fetch safe", e)))),
        }
    }

    /// Finds a Safe by its chain key (owner address) given as a hexadecimal string.
    ///
    /// The function validates the provided `chain_key` as an Ethereum-style hex address and returns one of the GraphQL
    /// union variants describing the outcome:
    /// - `Some(SafeResult::Safe(...))` when a matching safe is found,
    /// - `None` when no safe exists for the given chain key,
    /// - `Some(SafeResult::InvalidAddress(...))` when the `chain_key` is not a valid hex address,
    /// - `Some(SafeResult::QueryFailed(...))` when the database query fails.
    ///
    /// # Parameters
    ///
    /// - `chain_key`: Chain key to query (hexadecimal format).
    ///
    /// # Returns
    ///
    /// `Some(SafeResult::Safe)` with the found `Safe` if a record exists; `None` if no record exists;
    /// `Some(SafeResult::InvalidAddress)` if the chain key format is invalid; `Some(SafeResult::QueryFailed)` if the
    /// database query fails.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// // Given a prepared `query_root` and GraphQL `ctx`:
    /// let res = futures::executor::block_on(query_root.safe_by_chain_key(&ctx, "0x0123...".to_string())).unwrap();
    /// match res {
    ///     Some(SafeResult::Safe(s)) => println!("Found safe: {}", s.address),
    ///     Some(SafeResult::InvalidAddress(_)) => println!("Invalid chain key"),
    ///     Some(SafeResult::QueryFailed(_)) => println!("Query failed"),
    ///     None => println!("No safe for that chain key"),
    /// }
    /// ```
    #[graphql(name = "safeByChainKey")]
    async fn safe_by_chain_key(
        &self,
        ctx: &Context<'_>,
        #[graphql(desc = "Chain key to query (hexadecimal format)")] chain_key: String,
    ) -> Result<Option<SafeResult>> {
        let chain_key_address = match parse_safe_address(chain_key) {
            Ok(addr) => addr,
            Err(error_result) => return Ok(Some(error_result)),
        };

        let db = ctx.data::<DatabaseConnection>()?;

        match blokli_db_entity::hopr_safe_contract::Entity::find()
            .filter(blokli_db_entity::hopr_safe_contract::Column::ChainKey.eq(chain_key_address))
            .one(db)
            .await
        {
            Ok(Some(safe)) => match safe_from_db_model(safe) {
                Ok(safe_data) => Ok(Some(SafeResult::Safe(safe_data))),
                Err(e) => Ok(Some(SafeResult::QueryFailed(errors::invalid_db_data(
                    "safe addresses",
                    &e,
                )))),
            },
            Ok(None) => Ok(None),
            Err(e) => Ok(Some(SafeResult::QueryFailed(errors::query_failed(
                "fetch safe by chain key",
                e,
            )))),
        }
    }

    /// Fetches all indexed Safe contracts.
    ///
    /// On success returns `SafesResult::Safes` containing a `SafesList` with each safe's
    /// `address`, `module_address`, and `chain_key` encoded as hex strings. If the database
    /// query fails, returns `SafesResult::QueryFailed` with code `"QUERY_FAILED"` and a message.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// # use async_graphql::Context;
    /// # use crate::api::QueryRoot;
    /// # async fn doc_example(ctx: &Context<'_>) {
    /// let query = QueryRoot;
    /// let res = query.safes(ctx).await.unwrap();
    /// match res {
    ///     crate::api::SafesResult::Safes(list) => {
    ///         for safe in list.safes {
    ///             println!("safe: {}", safe.address);
    ///         }
    ///     }
    ///     crate::api::SafesResult::QueryFailed(err) => {
    ///         eprintln!("query failed: {}", err.message);
    ///     }
    /// }
    /// # }
    /// ```
    async fn safes(&self, ctx: &Context<'_>) -> Result<SafesResult> {
        let db = ctx.data::<DatabaseConnection>()?;

        match blokli_db_entity::hopr_safe_contract::Entity::find().all(db).await {
            Ok(safes) => {
                let safe_results: std::result::Result<Vec<Safe>, String> =
                    safes.into_iter().map(safe_from_db_model).collect();

                match safe_results {
                    Ok(safe_list) => Ok(SafesResult::Safes(SafesList { safes: safe_list })),
                    Err(e) => Ok(SafesResult::QueryFailed(errors::invalid_db_data("safe addresses", &e))),
                }
            }
            Err(e) => Ok(SafesResult::QueryFailed(errors::query_failed("fetch safes", e))),
        }
    }

    /// Returns the current chain configuration and runtime state exposed by the API.
    ///
    /// The returned `ChainInfo` contains the last indexed block number, the configured chain ID
    /// and network name, human-readable token values for ticket price and key binding fee,
    /// minimum incoming ticket winning probability, optional 32-byte domain separator hashes
    /// for channels/ledger/safe registry as `Hex32`, a map of contract addresses, and an optional
    /// channel closure grace period in seconds.
    ///
    /// # Examples
    ///
    /// ```
    /// # async fn doc_example() {
    /// // Query the GraphQL API for chain information
    /// let resp = /* execute GraphQL query `{ chainInfo { blockNumber chainId network } }` */ unimplemented!();
    /// // Inspect returned `ChainInfo` in the GraphQL response
    /// # }
    /// ```
    async fn chain_info(&self, ctx: &Context<'_>) -> ChainInfoResult {
        let db = match ctx.data::<DatabaseConnection>() {
            Ok(db) => db,
            Err(e) => {
                return ChainInfoResult::QueryFailed(errors::db_connection_error(format!("{:?}", e)));
            }
        };
        let chain_id = match ctx.data::<u64>() {
            Ok(id) => id,
            Err(e) => {
                return ChainInfoResult::QueryFailed(errors::context_error("chain ID", format!("{:?}", e)));
            }
        };
        let network = match ctx.data::<String>() {
            Ok(net) => net,
            Err(e) => {
                return ChainInfoResult::QueryFailed(errors::context_error("network name", format!("{:?}", e)));
            }
        };
        let contract_addresses = match ctx.data::<ContractAddresses>() {
            Ok(addrs) => addrs,
            Err(e) => {
                return ChainInfoResult::QueryFailed(errors::context_error("contract addresses", format!("{:?}", e)));
            }
        };

        // Fetch chain_info from database (assuming single row with id=1)
        let chain_info = match blokli_db_entity::chain_info::Entity::find_by_id(1).one(db).await {
            Ok(Some(info)) => info,
            Ok(None) => {
                return ChainInfoResult::QueryFailed(errors::not_found("chain info", "database"));
            }
            Err(e) => {
                return ChainInfoResult::QueryFailed(errors::query_failed("fetch chain info", e));
            }
        };

        // Convert ticket_price from 12-byte binary to human-readable string
        let ticket_price = chain_info
            .ticket_price
            .as_ref()
            .map(|bytes| TokenValueString(PrimitiveHoprBalance::from_be_bytes(bytes).amount().to_string()))
            .unwrap_or_else(|| TokenValueString(PrimitiveHoprBalance::zero().amount().to_string()));

        // Convert key_binding_fee from binary to human-readable string
        let key_binding_fee = chain_info
            .key_binding_fee
            .as_ref()
            .map(|bytes| TokenValueString(PrimitiveHoprBalance::from_be_bytes(bytes).amount().to_string()))
            .unwrap_or_else(|| TokenValueString(PrimitiveHoprBalance::zero().amount().to_string()));

        // Convert last_indexed_block from i64 to i32 with validation
        let block_number = match i32::try_from(chain_info.last_indexed_block) {
            Ok(bn) => bn,
            Err(_) => {
                return ChainInfoResult::QueryFailed(errors::conversion_error(
                    "i64",
                    "i32",
                    chain_info.last_indexed_block.to_string(),
                ));
            }
        };

        // Convert chain_id from u64 to i32 with validation
        let chain_id_i32 = match i32::try_from(*chain_id) {
            Ok(id) => id,
            Err(_) => {
                return ChainInfoResult::QueryFailed(errors::conversion_error("u64", "i32", chain_id.to_string()));
            }
        };

        // f32 -> f64 is widening, always safe
        #[allow(clippy::cast_lossless)]
        let min_ticket_winning_probability = chain_info.min_incoming_ticket_win_prob as f64;

        // Convert domain separators from binary to hex strings
        let channel_dst = match chain_info.channels_dst.as_ref() {
            Some(b) => {
                let bytes: &[u8; 32] = match b.as_slice().try_into() {
                    Ok(bytes) => bytes,
                    Err(_) => {
                        return ChainInfoResult::QueryFailed(errors::invalid_db_data(
                            "channels_dst",
                            &format!("must be 32 bytes, got {} bytes", b.len()),
                        ));
                    }
                };
                Some(Hash::from(*bytes).to_hex())
            }
            None => None,
        };
        let ledger_dst = match chain_info.ledger_dst.as_ref() {
            Some(b) => {
                let bytes: &[u8; 32] = match b.as_slice().try_into() {
                    Ok(bytes) => bytes,
                    Err(_) => {
                        return ChainInfoResult::QueryFailed(errors::invalid_db_data(
                            "ledger_dst",
                            &format!("must be 32 bytes, got {} bytes", b.len()),
                        ));
                    }
                };
                Some(Hash::from(*bytes).to_hex())
            }
            None => None,
        };
        let safe_registry_dst = match chain_info.safe_registry_dst.as_ref() {
            Some(b) => {
                let bytes: &[u8; 32] = match b.as_slice().try_into() {
                    Ok(bytes) => bytes,
                    Err(_) => {
                        return ChainInfoResult::QueryFailed(errors::invalid_db_data(
                            "safe_registry_dst",
                            &format!("must be 32 bytes, got {} bytes", b.len()),
                        ));
                    }
                };
                Some(Hash::from(*bytes).to_hex())
            }
            None => None,
        };

        // Convert channel closure grace period from i64 to UInt64 with validation
        let channel_closure_grace_period = match chain_info.channel_closure_grace_period {
            Some(period) => match u64::try_from(period) {
                Ok(p) => Some(UInt64(p)),
                Err(_) => {
                    return ChainInfoResult::QueryFailed(errors::conversion_error("i64", "u64", period.to_string()));
                }
            },
            None => None,
        };

        ChainInfoResult::ChainInfo(ChainInfo {
            block_number,
            chain_id: chain_id_i32,
            network: network.clone(),
            ticket_price,
            key_binding_fee,
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

    /// Calculate the predicted module address for a Safe deployment
    ///
    /// Calls the HoprNodeStakeFactory.predictModuleAddress_1 function to compute
    /// the deterministic CREATE2 address for a HOPR node management module.
    async fn calculate_module_address(
        &self,
        ctx: &Context<'_>,
        #[graphql(desc = "Safe owner address (hexadecimal format)")] owner: String,
        #[graphql(desc = "Safe deployment nonce")] nonce: UInt64,
        #[graphql(desc = "Safe contract address (hexadecimal format)")] safe_address: String,
    ) -> Result<CalculateModuleAddressResult> {
        // Validate owner address
        if let Err(e) = validate_eth_address(&owner) {
            return Ok(CalculateModuleAddressResult::InvalidAddress(
                errors::invalid_address_from_message(owner, e.message),
            ));
        }

        // Validate safe_address
        if let Err(e) = validate_eth_address(&safe_address) {
            return Ok(CalculateModuleAddressResult::InvalidAddress(
                errors::invalid_address_from_message(safe_address, e.message),
            ));
        }

        // Parse addresses
        let owner_addr = Address::from_hex(&owner)
            .map_err(|e| async_graphql::Error::new(format!("Invalid owner address: {}", e)))?;
        let safe_addr = Address::from_hex(&safe_address)
            .map_err(|e| async_graphql::Error::new(format!("Invalid safe address: {}", e)))?;

        // Get RPC operations from context
        let rpc = ctx.data::<Arc<RpcOperations<blokli_chain_rpc::ReqwestClient>>>()?;

        // Call RPC to calculate module address
        match blokli_chain_rpc::HoprRpcOperations::calculate_module_address(&**rpc, owner_addr, nonce.0, safe_addr)
            .await
        {
            Ok(module_address) => Ok(CalculateModuleAddressResult::ModuleAddress(ModuleAddress {
                module_address: module_address.to_hex(),
            })),
            Err(e) => Ok(CalculateModuleAddressResult::QueryFailed(errors::rpc_query_failed(
                "calculate module address",
                e,
            ))),
        }
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
    async fn transaction(&self, ctx: &Context<'_>, id: ID) -> Result<Option<TransactionResult>> {
        // Parse UUID from ID string
        let uuid = match uuid::Uuid::parse_str(id.as_str()) {
            Ok(uuid) => uuid,
            Err(_) => {
                return Ok(Some(TransactionResult::InvalidId(errors::invalid_transaction_id(
                    id.to_string(),
                ))));
            }
        };

        // Get transaction store from context
        let store = ctx.data::<Arc<TransactionStore>>()?;

        // Try to retrieve the transaction
        match store.get(uuid) {
            Ok(record) => {
                // Convert to GraphQL Transaction type
                let transaction = Transaction {
                    id: ID::from(record.id.to_string()),
                    status: crate::conversions::store_status_to_graphql(record.status),
                    submitted_at: record.submitted_at,
                    transaction_hash: record.transaction_hash.into(),
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
