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

use async_graphql::Schema;
use axum::Router;
use blokli_api::{
    config::{ApiConfig, HealthConfig, SseKeepAliveConfig},
    mutation::MutationRoot,
    query::QueryRoot,
    schema::build_schema,
    server::build_app,
    subscription::SubscriptionRoot,
};
use blokli_chain_api::{
    rpc_adapter::RpcAdapter,
    transaction_executor::{RawTransactionExecutor, RawTransactionExecutorConfig},
    transaction_monitor::{TransactionMonitor, TransactionMonitorConfig},
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
use hopr_bindings::exports::alloy::{
    node_bindings::AnvilInstance,
    rpc::client::ClientBuilder,
    transports::{http::ReqwestTransport, layers::RetryBackoffLayer},
};
use hopr_crypto_types::keypairs::{ChainKeypair, Keypair};
use migration::{Migrator, MigratorTrait, SafeDataOrigin};
use sea_orm::DatabaseConnection;
use tokio::task::AbortHandle;

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
#[allow(unused)]
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
/// use api::tests::common::{TestEnvironmentConfig, setup_test_environment};
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
                .expect(&format!("Failed to create test account {}", i))
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

    // Run migrations if configured
    if config.run_migrations {
        Migrator::<{ SafeDataOrigin::NoData as u8 }>::up(&db, None)
            .await
            .expect("Failed to run database migrations");
    }

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
        config.expected_block_time.as_secs(),
        indexer_state,
        transaction_executor.clone(),
        transaction_store.clone(),
        rpc_operations.clone(),
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

/// Test context for HTTP endpoint tests (health checks, readiness gates)
#[allow(unused)]
pub struct HttpTestContext {
    /// Anvil instance (must be kept alive)
    pub anvil: AnvilInstance,
    /// Axum router with HTTP endpoints
    pub app: Router,
    /// Database connection for test data manipulation
    pub db: DatabaseConnection,
    /// RPC operations for blockchain queries
    pub rpc_operations: Arc<RpcOperations<ReqwestClient>>,
}

/// Setup test environment for HTTP endpoint testing.
///
/// This helper creates an environment for testing HTTP endpoints like `/healthz` and `/readyz`.
/// It builds a complete Axum router with health checks configured.
///
/// # Returns
///
/// Returns an `HttpTestContext` containing the HTTP router and database connection.
///
/// # Examples
///
/// ```no_run
/// use api::tests::common::setup_http_test_environment;
///
/// let ctx = setup_http_test_environment().await?;
/// // Make HTTP requests to ctx.app
/// ```
pub async fn setup_http_test_environment() -> anyhow::Result<HttpTestContext> {
    // Use custom config to enable migrations and only 1 test account
    let mut config = TestEnvironmentConfig::default();
    config.run_migrations = true;
    config.num_test_accounts = 1;
    let expected_block_time = config.expected_block_time.as_secs();

    let ctx = setup_test_environment(config).await?;

    // Database connection with migrations already applied (via config.run_migrations)
    let db = ctx.db.as_ref().expect("Database should be present");

    // Create IndexerState for subscriptions (with small buffers)
    let indexer_state = IndexerState::new(1, 1);

    // Create API config with health settings
    let api_config = ApiConfig {
        playground_enabled: false,
        chain_id: ctx.chain_id,
        contract_addresses: ctx.contract_addrs.clone(),
        sse_keepalive: SseKeepAliveConfig {
            enabled: true,
            interval: Duration::from_millis(50),
            text: "keepalive-test".to_string(),
        },
        health: HealthConfig {
            max_indexer_lag: 10,
            timeout: Duration::from_millis(5000),
            readiness_check_interval: Duration::from_millis(100), // Fast updates for tests
        },
        ..Default::default()
    };

    // Build HTTP router
    let app = build_app(
        db.clone(),
        "test-network".to_string(),
        api_config,
        expected_block_time,
        indexer_state,
        ctx.transaction_executor.clone(),
        ctx.transaction_store.clone(),
        ctx.rpc_operations.clone(),
    )
    .await
    .expect("Failed to build app");

    Ok(HttpTestContext {
        anvil: ctx.anvil,
        app,
        db: db.clone(),
        rpc_operations: ctx.rpc_operations,
    })
}

/// Test context for transaction-related tests (mutations, queries, subscriptions)
#[allow(unused)]
pub struct TransactionTestContext {
    /// Anvil instance (must be kept alive)
    pub anvil: AnvilInstance,
    /// Test account with known private key
    pub chain_key: ChainKeypair,
    /// Transaction store for tracking transaction state
    pub store: Arc<TransactionStore>,
    /// Transaction monitor for polling transaction status
    pub monitor: Arc<TransactionMonitor<RpcAdapter<ReqwestClient>>>,
    /// Monitor task handle for cleanup
    pub monitor_handle: Option<AbortHandle>,
    /// Transaction executor
    pub executor: Arc<RawTransactionExecutor<RpcAdapter<ReqwestClient>>>,
    /// RPC adapter
    pub rpc_adapter: Arc<RpcAdapter<ReqwestClient>>,
}

impl Drop for TransactionTestContext {
    fn drop(&mut self) {
        if let Some(handle) = self.monitor_handle.take() {
            handle.abort();
        }
    }
}

/// Setup test environment for transaction testing.
///
/// This helper creates an environment for testing transaction mutations, queries, and subscriptions.
/// It configures custom RPC settings optimized for testing and starts a transaction monitor.
///
/// # Arguments
///
/// * `block_time` - Expected block time for Anvil
/// * `poll_interval` - How often to poll for transaction updates
/// * `finality` - Number of confirmations required
/// * `executor_config` - Optional custom executor configuration (uses default if None)
///
/// # Returns
///
/// Returns a `TransactionTestContext` with transaction monitoring infrastructure.
///
/// # Examples
///
/// ```no_run
/// use std::time::Duration;
///
/// use api::tests::common::setup_transaction_test_environment;
///
/// let ctx = setup_transaction_test_environment(Duration::from_secs(1), Duration::from_millis(100), 2, None).await?;
/// ```
pub async fn setup_transaction_test_environment(
    block_time: Duration,
    poll_interval: Duration,
    finality: u32,
    executor_config: Option<RawTransactionExecutorConfig>,
) -> anyhow::Result<TransactionTestContext> {
    let _ = env_logger::builder().is_test(true).try_init();

    let mut config = TestEnvironmentConfig::default();
    config.expected_block_time = block_time;
    config.num_test_accounts = 1;

    let ctx = setup_test_environment(config).await?;

    let rpc_config = RpcOperationsConfig {
        chain_id: ctx.chain_id,
        tx_polling_interval: poll_interval,
        expected_block_time: block_time,
        finality,
        gas_oracle_url: None,
        contract_addrs: ctx.contract_addrs.clone(),
        ..Default::default()
    };

    let transport_client = ReqwestTransport::new(ctx.anvil.endpoint_url());
    let rpc_client = ClientBuilder::default()
        .layer(RetryBackoffLayer::new_with_policy(
            2,
            100,
            100,
            DefaultRetryPolicy::default(),
        ))
        .transport(transport_client.clone(), transport_client.guess_local());

    let rpc_operations = RpcOperations::new(rpc_client, ReqwestClient::new(), rpc_config, None)?;
    let rpc_adapter = Arc::new(RpcAdapter::new(rpc_operations));

    let transaction_store = Arc::new(TransactionStore::new());
    let transaction_validator = Arc::new(TransactionValidator::new());

    let transaction_executor = Arc::new(RawTransactionExecutor::with_shared_dependencies(
        rpc_adapter.clone(),
        transaction_store.clone(),
        transaction_validator.clone(),
        executor_config.unwrap_or_default(),
    ));

    let monitor_config = TransactionMonitorConfig {
        poll_interval,
        timeout: Duration::from_secs(30),
        per_transaction_delay: Duration::from_millis(10),
    };

    let transaction_monitor = Arc::new(TransactionMonitor::new(
        transaction_store.clone(),
        (*rpc_adapter).clone(),
        monitor_config,
        None,
    ));

    let monitor_handle = Some(
        tokio::spawn({
            let monitor = transaction_monitor.clone();
            async move {
                monitor.start().await;
            }
        })
        .abort_handle(),
    );

    Ok(TransactionTestContext {
        anvil: ctx.anvil,
        chain_key: ctx.test_accounts[0].clone(),
        store: transaction_store,
        monitor: transaction_monitor,
        monitor_handle,
        executor: transaction_executor,
        rpc_adapter,
    })
}
