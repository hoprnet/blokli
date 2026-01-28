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
/// - IndexerState injected as context data (for subscription coordination)
/// - Transaction executor and store injected as context data (for mutations and transaction queries)
/// - RPC operations injected as context data (for passthrough balance queries)
/// - Query depth limit (10 levels) to prevent excessive nesting
/// - Query complexity limit (100 points) to prevent expensive operations
#[allow(clippy::too_many_arguments)]
pub fn build_schema<R: HttpRequestor + 'static + Clone>(
    db: DatabaseConnection,
    chain_id: u64,
    network: String,
    contract_addresses: ContractAddresses,
    expected_block_time: u64,
    indexer_state: IndexerState,
    transaction_executor: Arc<RawTransactionExecutor<RpcAdapter<DefaultHttpRequestor>>>,
    transaction_store: Arc<TransactionStore>,
    rpc_operations: Arc<RpcOperations<R>>,
) -> Schema<QueryRoot, MutationRoot, SubscriptionRoot> {
    Schema::build(QueryRoot, MutationRoot, SubscriptionRoot)
        .limit_depth(10)
        .limit_complexity(100)
        .data(db)
        .data(chain_id)
        .data(network)
        .data(contract_addresses)
        .data(ExpectedBlockTime(expected_block_time))
        .data(indexer_state)
        .data(transaction_executor)
        .data(transaction_store)
        .data(rpc_operations)
        .finish()
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
        5, // Placeholder expected block time
        indexer_state,
        transaction_executor,
        transaction_store,
        rpc_operations,
    );

    schema.sdl()
}
