//! GraphQL schema builder for blokli API

use std::sync::Arc;

use async_graphql::Schema;
use blokli_chain_api::{
    DefaultHttpRequestor, rpc_adapter::RpcAdapter, transaction_executor::RawTransactionExecutor,
    transaction_store::TransactionStore,
};
use blokli_chain_indexer::IndexerState;
use sea_orm::DatabaseConnection;

use crate::{mutation::MutationRoot, query::QueryRoot, subscription::SubscriptionRoot};

/// Build the async-graphql schema with database connection, chain ID, indexer state, and transaction components
///
/// This creates a GraphQL schema with:
/// - Read-only queries for public entities (account, announcement, channel, balances)
/// - Mutations for transaction submission (sendTransaction, sendTransactionAsync, sendTransactionSync)
/// - Real-time subscriptions for balance and channel updates
///
/// The schema is configured with:
/// - Database connection injected as context data
/// - Chain ID injected as context data
/// - IndexerState injected as context data (for subscription coordination)
/// - Transaction executor and store injected as context data (for mutations and transaction queries)
/// - Query depth limit (10 levels) to prevent excessive nesting
/// - Query complexity limit (100 points) to prevent expensive operations
pub fn build_schema(
    db: DatabaseConnection,
    chain_id: u64,
    indexer_state: IndexerState,
    transaction_executor: Arc<RawTransactionExecutor<RpcAdapter<DefaultHttpRequestor>>>,
    transaction_store: Arc<TransactionStore>,
) -> Schema<QueryRoot, MutationRoot, SubscriptionRoot> {
    Schema::build(QueryRoot, MutationRoot, SubscriptionRoot)
        .limit_depth(10)
        .limit_complexity(100)
        .data(db)
        .data(chain_id)
        .data(indexer_state)
        .data(transaction_executor)
        .data(transaction_store)
        .finish()
}

/// Export the GraphQL schema to SDL (Schema Definition Language) format
///
/// This generates a string representation of the GraphQL schema that can be used
/// for code generation, documentation, or schema validation tools.
pub fn export_schema_sdl(
    db: DatabaseConnection,
    chain_id: u64,
    indexer_state: IndexerState,
    transaction_executor: Arc<RawTransactionExecutor<RpcAdapter<DefaultHttpRequestor>>>,
    transaction_store: Arc<TransactionStore>,
) -> String {
    let schema = build_schema(db, chain_id, indexer_state, transaction_executor, transaction_store);
    schema.sdl()
}
