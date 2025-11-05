//! GraphQL schema builder for blokli API

use async_graphql::{EmptyMutation, Schema};
use blokli_chain_types::ContractAddresses;
use sea_orm::DatabaseConnection;

use crate::{query::QueryRoot, subscription::SubscriptionRoot};

/// Build the async-graphql schema with database connection, chain ID, and contract addresses
///
/// This creates a GraphQL schema with:
/// - Read-only queries for public entities (account, announcement, channel, balances)
/// - No mutations (EmptyMutation)
/// - Real-time subscriptions for balance and channel updates
///
/// The schema is configured with:
/// - Database connection injected as context data
/// - Chain ID injected as context data
/// - Contract addresses injected as context data
/// - Query and subscription access patterns
/// - Query depth limit (10 levels) to prevent excessive nesting
/// - Query complexity limit (100 points) to prevent expensive operations
pub fn build_schema(
    db: DatabaseConnection,
    chain_id: u64,
    contract_addresses: ContractAddresses,
) -> Schema<QueryRoot, EmptyMutation, SubscriptionRoot> {
    Schema::build(QueryRoot, EmptyMutation, SubscriptionRoot)
        .limit_depth(10)
        .limit_complexity(100)
        .data(db)
        .data(chain_id)
        .data(contract_addresses)
        .finish()
}

/// Export the GraphQL schema to SDL (Schema Definition Language) format
///
/// This generates a string representation of the GraphQL schema that can be used
/// for code generation, documentation, or schema validation tools.
pub fn export_schema_sdl(db: DatabaseConnection, chain_id: u64, contract_addresses: ContractAddresses) -> String {
    let schema = build_schema(db, chain_id, contract_addresses);
    schema.sdl()
}
