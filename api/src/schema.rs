//! GraphQL schema builder for blokli API

use std::sync::Arc;

use async_graphql::Schema;
use blokli_chain_api::{
    DefaultHttpRequestor, rpc_adapter::RpcAdapter, transaction_executor::RawTransactionExecutor,
    transaction_store::TransactionStore,
};
use blokli_chain_indexer::IndexerState;
use blokli_chain_rpc::{rpc::RpcOperations, transport::HttpRequestor};
use blokli_chain_types::ContractAddresses;
use sea_orm::DatabaseConnection;

use crate::{mutation::MutationRoot, query::QueryRoot, subscription::SubscriptionRoot};

/// Wrapper type for expected block time to avoid type confusion in context
#[derive(Debug, Clone, Copy)]
pub struct ExpectedBlockTime(pub u64);

/// Wrapper type for finality (block confirmations) to avoid type confusion in context
#[derive(Debug, Clone, Copy)]
pub struct Finality(pub u16);

/// Wrapper type for gas multiplier to avoid type confusion in context
#[derive(Debug, Clone, Copy)]
pub struct GasMultiplier(pub f64);

/// Wrapper type for chain ID to avoid type confusion in context
#[derive(Debug, Clone, Copy)]
pub struct ChainId(pub u64);

/// Wrapper type for network name to avoid type confusion in context
#[derive(Debug, Clone)]
pub struct NetworkName(pub String);

/// Wrapper type describing whether the server indexes Safe events
#[derive(Debug, Clone, Copy)]
pub struct SafeEventIndexingEnabled(pub bool);

/// Build the async-graphql schema with database connection, chain ID, network, indexer state, and transaction
/// components
///
/// This creates a GraphQL schema with:
/// - Read-only queries for public entities (account, announcement, channel, balances)
/// - Mutations for transaction submission (sendTransaction, sendTransactionAsync, sendTransactionSync)
/// - Real-time subscriptions for balance and channel updates
///
/// The schema is configured with:
/// - Database connection injected as context data
/// - Chain ID injected as context data
/// - Network name injected as context data
/// - Contract addresses injected as context data
/// - Expected block time injected as context data
/// - Finality (block confirmations) injected as context data
/// - IndexerState injected as context data (for subscription coordination)
/// - Transaction executor and store injected as context data (for mutations and transaction queries)
/// - RPC operations injected as context data (for passthrough balance queries)
/// - Query depth limit (8 levels) to prevent excessive nesting
/// - Query complexity limit (500 points) to throttle expensive RPC fan-out
///
/// Set `enforce_limits` to `false` to build a schema without depth/complexity limits,
/// used for introspection queries that exceed the standard limits by design.
#[allow(clippy::too_many_arguments)]
pub fn build_schema<R: HttpRequestor + 'static + Clone>(
    db: DatabaseConnection,
    chain_id: u64,
    network: String,
    contract_addresses: ContractAddresses,
    expected_block_time: u64,
    finality: u16,
    gas_multiplier: f64,
    indexes_safe_events: bool,
    indexer_state: IndexerState,
    transaction_executor: Arc<RawTransactionExecutor<RpcAdapter<DefaultHttpRequestor>>>,
    transaction_store: Arc<TransactionStore>,
    rpc_operations: Arc<RpcOperations<R>>,
    enforce_limits: bool,
) -> Schema<QueryRoot, MutationRoot, SubscriptionRoot> {
    let mut builder = Schema::build(QueryRoot, MutationRoot, SubscriptionRoot)
        .data(db)
        .data(ChainId(chain_id))
        .data(NetworkName(network))
        .data(contract_addresses)
        .data(ExpectedBlockTime(expected_block_time))
        .data(Finality(finality))
        .data(GasMultiplier(gas_multiplier))
        .data(SafeEventIndexingEnabled(indexes_safe_events))
        .data(indexer_state)
        .data(transaction_executor)
        .data(transaction_store)
        .data(rpc_operations);

    if enforce_limits {
        // Depth guards against pathological nesting (real queries reach depth ~4 at most).
        // Complexity throttles request cost: cheap scalar/DB fields cost 1 each, while
        // RPC-backed resolvers carry per-field weights (see `complexity` attributes in
        // query.rs). A 500 budget comfortably fits any real query yet caps RPC fan-out
        // (e.g. aliased `nativeBalance` calls) to ~10 per request.
        //
        // Introspection queries are routed to a separate, unlimited schema
        // (see `enforce_limits = false`), so these limits never block schema loading.
        builder = builder.limit_depth(8).limit_complexity(500);
    }

    builder.finish()
}

/// Export the GraphQL schema to SDL (Schema Definition Language) format
///
/// This generates a string representation of the GraphQL schema that can be used
/// for code generation, documentation, or schema validation tools.
#[allow(clippy::too_many_arguments)]
pub fn export_schema_sdl<R: HttpRequestor + 'static + Clone>(
    db: DatabaseConnection,
    chain_id: u64,
    contract_addresses: ContractAddresses,
    indexes_safe_events: bool,
    indexer_state: IndexerState,
    transaction_executor: Arc<RawTransactionExecutor<RpcAdapter<DefaultHttpRequestor>>>,
    transaction_store: Arc<TransactionStore>,
    rpc_operations: Arc<RpcOperations<R>>,
) -> String {
    let schema = build_schema(
        db,
        chain_id,
        "PLACEHOLDER".to_string(),
        contract_addresses,
        5,   // Placeholder expected block time
        8,   // Placeholder finality
        1.0, // Placeholder gas multiplier
        indexes_safe_events,
        indexer_state,
        transaction_executor,
        transaction_store,
        rpc_operations,
        false,
    );

    schema.sdl()
}
