//! Common test utilities and setup functions for API integration tests
//!
//! This module provides shared test setup functionality to reduce duplication
//! across integration tests. The main function `setup_test_environment` handles:
//! - Starting Anvil with HOPR contracts
//! - Deploying contracts
//! - Waiting for finality
//! - Creating RPC operations
//! - Setting up database connections
//! - Creating test accounts
//!
//! Tests can use this shared setup to ensure consistency and reduce maintenance burden.

#[cfg(test)]
mod test_common;

use std::{sync::Arc, time::Duration};

use alloy::{
    node_bindings::AnvilInstance,
    rpc::client::ClientBuilder,
    transports::{http::ReqwestTransport, layers::RetryBackoffLayer},
};
use async_graphql::Schema;
use blokli_api::{mutation::MutationRoot, query::QueryRoot, schema::build_schema, subscription::SubscriptionRoot};
use blokli_chain_api::{
    rpc_adapter::RpcAdapter,
    transaction_executor::{RawTransactionExecutor, RawTransactionExecutorConfig},
    transaction_store::TransactionStore,
    transaction_validator::TransactionValidator,
};
use blokli_chain_indexer::IndexerState;
use blokli_chain_rpc::{
    client::{DefaultRetryPolicy, create_rpc_client_to_anvil},
    rpc::{RpcOperations, RpcOperationsConfig},
    transport::ReqwestClient,
};
use blokli_chain_types::{ContractAddresses, ContractInstances, utils::create_anvil};
use hopr_crypto_types::keypairs::{ChainKeypair, Keypair};
use sea_orm::DatabaseConnection;

/// Test environment configuration options
#[derive(Debug, Clone)]
pub struct TestEnvironmentConfig {
    /// Expected block time for Anvil (default: 1 second)
    pub expected_block_time: Duration,
    /// Number of test accounts to create (default: 3)
    pub num_test_accounts: usize,
    /// Whether to run database migrations (default: false for balance tests)
    pub run_migrations: bool,
}

impl Default for TestEnvironmentConfig {
    fn default() -> Self {
        Self {
            expected_block_time: Duration::from_secs(1),
            num_test_accounts: 3,
            run_migrations: false,
        }
    }
}

/// Test context containing all components needed for API testing
pub struct TestContext {
    /// Anvil instance (must be kept alive)
    pub anvil: AnvilInstance,
    /// Deployed contract instances
    pub contract_instances: ContractInstances<Arc<blokli_chain_rpc::client::AnvilRpcClient>>,
    /// Test accounts with known private keys
    pub test_accounts: Vec<ChainKeypair>,
    /// RPC operations for blockchain interactions
    pub rpc_operations: Arc<RpcOperations<ReqwestClient>>,
    /// Database connection (if migrations were run)
    pub db: Option<DatabaseConnection>,
    /// Chain ID (Anvil default: 31337)
    pub chain_id: u64,
    /// Contract addresses
    pub contract_addrs: ContractAddresses,
    /// GraphQL schema with all resolvers
    pub schema: Schema<QueryRoot, MutationRoot, SubscriptionRoot>,
    /// Transaction store
    pub transaction_store: Arc<TransactionStore>,
    /// Transaction executor
    pub transaction_executor: Arc<RawTransactionExecutor<RpcAdapter<ReqwestClient>>>,
}

/// Setup test environment with Anvil, contracts, RPC operations, and GraphQL schema.
///
/// This function handles the common setup pattern used across all API integration tests:
/// 1. Starts Anvil with configurable block time
/// 2. Creates test accounts from Anvil's deterministic keys
/// 3. Deploys HOPR contracts
/// 4. Waits for contract deployment finality
/// 5. Creates RPC operations instance
/// 6. Sets up in-memory SQLite database
/// 7. Creates transaction components (executor, store, validator)
/// 8. Builds GraphQL schema with all dependencies
///
/// # Arguments
///
/// * `config` - Configuration options for the test environment
///
/// # Returns
///
/// Returns a `TestContext` containing all the components needed for testing.
///
/// # Examples
///
/// ```no_run
/// use api::tests::common::{setup_test_environment, TestEnvironmentConfig};
/// 
/// let ctx = setup_test_environment(TestEnvironmentConfig::default()).await?;
/// // Use ctx.anvil, ctx.rpc_operations, etc. in your tests
/// ```
pub async fn setup_test_environment(config: TestEnvironmentConfig) -> anyhow::Result<TestContext> {
    let _ = env_logger::builder().is_test(true).try_init();

    // Start Anvil with configurable block time
    let anvil = create_anvil(Some(config.expected_block_time));

    // Create test accounts from Anvil's deterministic keys
    let test_accounts: Vec<ChainKeypair> = (0..config.num_test_accounts)
        .map(|i| {
            ChainKeypair::from_secret(anvil.keys()[i].to_bytes().as_ref())
                .expect("Failed to create test account {i}")
        })
        .collect();

    // Deploy HOPR contracts
    let contract_instances = {
        let client = create_rpc_client_to_anvil(&anvil, &test_accounts[0]);
        ContractInstances::deploy_for_testing(client, &test_accounts[0])
            .await
            .expect("Failed to deploy contracts")
    };

    let contract_addrs = ContractAddresses::from(&contract_instances);

    // Wait for contract deployments to be final
    // We wait for 1 block (initial deployment) + 2 blocks (finality) * expected_block_time
    tokio::time::sleep((1 + 2) * config.expected_block_time).await;

    // Create RPC operations for blockchain interactions
    let transport_client = ReqwestTransport::new(anvil.endpoint_url());
    let rpc_client = ClientBuilder::default()
        .layer(RetryBackoffLayer::new_with_policy(
            2,
            100,
            100,
            DefaultRetryPolicy::default(),
        ))
        .transport(transport_client.clone(), transport_client.guess_local());

    let chain_id = 31337; // Anvil default chain ID

    let rpc_operations = Arc::new(
        RpcOperations::new(
            rpc_client.clone(),
            ReqwestClient::new(),
            RpcOperationsConfig {
                chain_id,
                contract_addrs: contract_addrs.clone(),
                expected_block_time: config.expected_block_time,
                ..Default::default()
            },
            None,
        )
        .expect("Failed to create RPC operations"),
    );

    // Set up database connection (always create for GraphQL schema)
    let db = sea_orm::Database::connect("sqlite::memory:")
        .await
        .expect("Failed to create test database");

    // Create transaction components for GraphQL API
    let transaction_store = Arc::new(TransactionStore::new());
    let transaction_validator = Arc::new(TransactionValidator::new());
    let rpc_adapter = Arc::new(RpcAdapter::new(
        RpcOperations::new(
            rpc_client,
            ReqwestClient::new(),
            RpcOperationsConfig {
                chain_id,
                contract_addrs: contract_addrs.clone(),
                expected_block_time: config.expected_block_time,
                ..Default::default()
            },
            None,
        )
        .expect("Failed to create RPC adapter operations"),
    ));

    let transaction_executor = Arc::new(RawTransactionExecutor::with_shared_dependencies(
        rpc_adapter,
        transaction_store.clone(),
        transaction_validator,
        RawTransactionExecutorConfig::default(),
    ));

    // Create IndexerState for subscriptions
    let indexer_state = IndexerState::new(1, 1);

    // Build GraphQL schema with all dependencies
    let schema = build_schema(
        db.clone(),
        chain_id,
        "test-network".to_string(),
        contract_addrs.clone(),
        indexer_state,
        transaction_executor.clone(),
        transaction_store.clone(),
        rpc_operations.clone(),
        None, // No SQLite notification manager for tests
    );

    Ok(TestContext {
        anvil,
        contract_instances,
        test_accounts,
        rpc_operations,
        db: Some(db),
        chain_id,
        contract_addrs,
        schema,
        transaction_store,
        transaction_executor,
    })
}

/// Helper function to create a simple test context with default configuration
///
/// This is a convenience wrapper around `setup_test_environment` for tests
/// that don't need custom configuration.
pub async fn setup_simple_test_environment() -> anyhow::Result<TestContext> {
    setup_test_environment(TestEnvironmentConfig::default()).await
}