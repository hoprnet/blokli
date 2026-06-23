//! GraphQL schema builder for blokli API

use std::{collections::HashMap, future::Future, pin::Pin, sync::Arc};

use async_graphql::{ObjectType, Schema, SubscriptionType};
use blokli_chain_api::{
    DefaultHttpRequestor, rpc_adapter::RpcAdapter, transaction_executor::RawTransactionExecutor,
    transaction_store::TransactionStore,
};
use blokli_chain_indexer::IndexerState;
use blokli_chain_rpc::{rpc::RpcOperations, transport::HttpRequestor};
use blokli_chain_types::ContractAddresses;
use futures::Stream;
use sea_orm::DatabaseConnection;

use crate::{mutation::MutationRoot, query::QueryRoot, readiness::ReadinessChecker, subscription::SubscriptionRoot};

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

/// Build the registry of all supported versioned schemas.
///
/// Each entry maps a schema version number to its type-erased schema handle.
/// Currently only v1 exists; when a breaking change ships, add v<n+1> here
/// alongside v1 so old clients continue to work during the transition window.
#[allow(clippy::too_many_arguments)]
pub fn build_version_registry<R: HttpRequestor + 'static + Clone>(
    db: DatabaseConnection,
    chain_id: u64,
    network: String,
    contract_addresses: ContractAddresses,
    expected_block_time: u64,
    finality: u16,
    gas_multiplier: f64,
    indexer_state: IndexerState,
    transaction_executor: Arc<RawTransactionExecutor<RpcAdapter<DefaultHttpRequestor>>>,
    transaction_store: Arc<TransactionStore>,
    rpc_operations: Arc<RpcOperations<R>>,
    readiness_checker: ReadinessChecker,
    limits: Option<(usize, usize)>,
) -> HashMap<u32, Arc<dyn ErasedSchema>> {
    let v1: Arc<dyn ErasedSchema> = Arc::new(build_schema(
        db,
        chain_id,
        network,
        contract_addresses,
        expected_block_time,
        finality,
        gas_multiplier,
        indexer_state,
        transaction_executor,
        transaction_store,
        rpc_operations,
        readiness_checker,
        limits,
    ));
    HashMap::from([(1, v1)])
}

/// The latest supported schema version. Clients that omit `X-Blokli-Schema-Version` are routed here.
pub const LATEST_SCHEMA_VERSION: u32 = 1;

/// Type-erased GraphQL schema handle.
///
/// Each supported schema version wraps its typed `Schema<Q, M, S>` behind this trait so
/// they can all live in the same `HashMap` regardless of their concrete root types.
pub trait ErasedSchema: Send + Sync {
    fn execute<'a>(
        &'a self,
        request: async_graphql::Request,
    ) -> Pin<Box<dyn Future<Output = async_graphql::Response> + Send + 'a>>;
    fn execute_stream(
        &self,
        request: async_graphql::Request,
    ) -> Pin<Box<dyn Stream<Item = async_graphql::Response> + Send>>;
    fn sdl(&self) -> String;
}

impl<Q, M, S> ErasedSchema for Schema<Q, M, S>
where
    Q: ObjectType + 'static,
    M: ObjectType + 'static,
    S: SubscriptionType + 'static,
{
    fn execute<'a>(
        &'a self,
        request: async_graphql::Request,
    ) -> Pin<Box<dyn Future<Output = async_graphql::Response> + Send + 'a>> {
        Box::pin(Schema::execute(self, request))
    }

    fn execute_stream(
        &self,
        request: async_graphql::Request,
    ) -> Pin<Box<dyn Stream<Item = async_graphql::Response> + Send>> {
        Box::pin(Schema::execute_stream(self, request))
    }

    fn sdl(&self) -> String {
        Schema::sdl(self)
    }
}

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
/// - Query depth limit to prevent excessive nesting
/// - Query complexity limit to throttle expensive RPC fan-out
///
/// Pass `limits` as `Some((max_depth, max_complexity))` to enforce limits, or `None` to
/// build an unlimited schema (used for introspection queries).
#[allow(clippy::too_many_arguments)]
pub fn build_schema<R: HttpRequestor + 'static + Clone>(
    db: DatabaseConnection,
    chain_id: u64,
    network: String,
    contract_addresses: ContractAddresses,
    expected_block_time: u64,
    finality: u16,
    gas_multiplier: f64,
    indexer_state: IndexerState,
    transaction_executor: Arc<RawTransactionExecutor<RpcAdapter<DefaultHttpRequestor>>>,
    transaction_store: Arc<TransactionStore>,
    rpc_operations: Arc<RpcOperations<R>>,
    readiness_checker: ReadinessChecker,
    limits: Option<(usize, usize)>,
) -> Schema<QueryRoot, MutationRoot, SubscriptionRoot> {
    let mut builder = Schema::build(QueryRoot, MutationRoot, SubscriptionRoot)
        .data(db)
        .data(ChainId(chain_id))
        .data(NetworkName(network))
        .data(contract_addresses)
        .data(ExpectedBlockTime(expected_block_time))
        .data(Finality(finality))
        .data(GasMultiplier(gas_multiplier))
        .data(indexer_state)
        .data(transaction_executor)
        .data(transaction_store)
        .data(rpc_operations)
        .data(readiness_checker);

    if let Some((max_depth, max_complexity)) = limits {
        builder = builder.limit_depth(max_depth).limit_complexity(max_complexity);
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
    indexer_state: IndexerState,
    transaction_executor: Arc<RawTransactionExecutor<RpcAdapter<DefaultHttpRequestor>>>,
    transaction_store: Arc<TransactionStore>,
    rpc_operations: Arc<RpcOperations<R>>,
    readiness_checker: ReadinessChecker,
) -> String {
    let schema = build_schema(
        db,
        chain_id,
        "PLACEHOLDER".to_string(),
        contract_addresses,
        5,   // Placeholder expected block time
        8,   // Placeholder finality
        1.0, // Placeholder gas multiplier
        indexer_state,
        transaction_executor,
        transaction_store,
        rpc_operations,
        readiness_checker,
        None,
    );

    schema.sdl()
}
