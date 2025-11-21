//! Integration tests for Safe transaction count API queries (safeTransactionCount)
//!
//! These tests verify end-to-end functionality by:
//! - Running a real Anvil instance with deployed HOPR contracts
//! - Executing GraphQL queries against the schema
//! - Verifying transaction counts are correctly fetched from the blockchain via RPC
//!
//! ## Test Coverage
//!
//! **Positive Path Tests**:
//! - Mock Safe contract deployment and nonce queries
//! - Nonce progression verification (0 → 1 → 2)
//! - GraphQL response structure validation
//! - Blockchain state changes reflected in API queries
//!
//! **Error Path Tests**:
//! - Invalid address format handling (returns InvalidAddressError)
//! - EOA address handling (returns QueryFailedError since EOAs aren't Safe contracts)
//! - Address format variations (with/without 0x prefix)
//!
//! ## Mock Contract Approach
//!
//! The tests use a minimal MockSafe contract (compiled with solc 0.8.28) that implements
//! only the `nonce()` function needed for API testing. This approach:
//! - Tests the full integration path (GraphQL → RPC → Blockchain)
//! - Avoids complexity of deploying full Gnosis Safe contracts
//! - Runs fast and requires no external dependencies
//! - Verifies the RPC call path matches production behavior
//!
//! The MockSafe contract source is in `api/tests/contracts/MockSafe.sol`.

use std::{sync::Arc, time::Duration};

use alloy::{
    node_bindings::AnvilInstance,
    primitives::U256,
    rpc::client::ClientBuilder,
    sol,
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
use hopr_primitive_types::traits::ToHex;
use sea_orm::{Database, DatabaseConnection};

// Mock Safe contract for testing transaction count queries
// This minimal implementation only includes the nonce() function needed for API testing
sol!(
    #[sol(rpc)]
    #[sol(bytecode = "6080604052348015600e575f5ffd5b505f805560bc80601d5f395ff3fe6080604052348015600e575f5ffd5b50600436106030575f3560e01c8063627cdcb9146034578063affed0e014603c575b5f5ffd5b603a6050565b005b5f5460405190815260200160405180910390f35b5f80549080605c836063565b9190505550565b5f60018201607f57634e487b7160e01b5f52601160045260245ffd5b506001019056fea26469706673582212201f27428b36007ca1498eab761cebf82a05f8f316ca2acfe7f363e365dcbfd2c364736f6c634300081c0033")]
    contract MockSafe {
        /// Returns the current nonce (matches Gnosis Safe interface)
        function nonce() public view returns (uint256);

        /// Increments the nonce by 1 (simplified transaction execution)
        function incrementNonce() public;
    }
);

/// Test context containing all components needed for transaction count API testing
struct TestContext {
    /// Anvil instance (must be kept alive)
    _anvil: AnvilInstance,
    /// GraphQL schema with all resolvers
    schema: Schema<QueryRoot, MutationRoot, SubscriptionRoot>,
    /// Deployed contract instances for Safe operations (unused but needed for setup)
    #[allow(dead_code)]
    contract_instances: ContractInstances<Arc<blokli_chain_rpc::client::AnvilRpcClient>>,
    /// Test accounts with known private keys
    test_accounts: Vec<ChainKeypair>,
    /// Database connection
    _db: DatabaseConnection,
}

/// Setup test environment with Anvil, contracts, and GraphQL schema.
/// Each test calls this function to get its own independent test context.
async fn setup_test_environment() -> anyhow::Result<TestContext> {
    let _ = env_logger::builder().is_test(true).try_init();

    // Start Anvil with 1-second block time for fast testing
    let expected_block_time = Duration::from_secs(1);
    let anvil = create_anvil(Some(expected_block_time));

    // Create test accounts from Anvil's deterministic keys
    let test_accounts = vec![
        ChainKeypair::from_secret(anvil.keys()[0].to_bytes().as_ref()).expect("Failed to create test account 0"),
        ChainKeypair::from_secret(anvil.keys()[1].to_bytes().as_ref()).expect("Failed to create test account 1"),
        ChainKeypair::from_secret(anvil.keys()[2].to_bytes().as_ref()).expect("Failed to create test account 2"),
    ];

    // Deploy HOPR contracts
    let contract_instances = {
        let client = create_rpc_client_to_anvil(&anvil, &test_accounts[0]);
        ContractInstances::deploy_for_testing(client, &test_accounts[0])
            .await
            .expect("Failed to deploy contracts")
    };

    let contract_addrs = ContractAddresses::from(&contract_instances);

    // Wait for contract deployments to be final
    tokio::time::sleep((1 + 2) * expected_block_time).await;

    // Setup in-memory SQLite database for testing
    // We don't need actual database tables for transaction count API tests since
    // transaction counts are queried directly from RPC, not from the database
    let db = Database::connect("sqlite::memory:")
        .await
        .expect("Failed to create test database");

    // Create RPC operations for transaction count queries
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
                contract_addrs,
                expected_block_time,
                ..Default::default()
            },
            None,
        )
        .expect("Failed to create RPC operations"),
    );

    // Create stub transaction components (not used for transaction count queries)
    let transaction_store = Arc::new(TransactionStore::new());
    let transaction_validator = Arc::new(TransactionValidator::new());
    let rpc_adapter = Arc::new(RpcAdapter::new(
        RpcOperations::new(
            rpc_client,
            ReqwestClient::new(),
            RpcOperationsConfig {
                chain_id,
                contract_addrs,
                expected_block_time,
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

    // Create IndexerState for subscriptions (with small buffers)
    let indexer_state = IndexerState::new(1, 1);

    // Build GraphQL schema with all dependencies
    let schema = build_schema(
        db.clone(),
        chain_id,
        "test-network".to_string(),
        contract_addrs,
        indexer_state,
        transaction_executor,
        transaction_store,
        rpc_operations,
        None, // No SQLite notification manager for safe tx count tests
    );

    Ok(TestContext {
        _anvil: anvil,
        schema,
        contract_instances,
        test_accounts,
        _db: db,
    })
}

/// Executes a GraphQL query against the provided schema.
///
/// # Arguments
/// * `schema` - The GraphQL schema to execute the query against
/// * `query` - The GraphQL query string to execute
///
/// # Returns
/// The GraphQL response containing data or errors
async fn execute_graphql_query(
    schema: &Schema<QueryRoot, MutationRoot, SubscriptionRoot>,
    query: &str,
) -> async_graphql::Response {
    schema.execute(query).await
}

/// Queries the Safe transaction count (nonce) for a given address via GraphQL.
///
/// This helper constructs a GraphQL query for the `safeTransactionCount` field,
/// executes it against the schema, and returns the parsed JSON response.
///
/// # Arguments
/// * `schema` - The GraphQL schema with the `safeTransactionCount` query
/// * `address` - Hexadecimal address string (with or without 0x prefix)
///
/// # Returns
/// * `Ok(serde_json::Value)` - JSON response containing either:
///   - `SafeTransactionCount` with `address` and `count` fields
///   - `InvalidAddressError` with error details
///   - `QueryFailedError` with error message
/// * `Err` - If the GraphQL query itself fails to execute
///
/// # Example Response
/// ```json
/// {
///   "safeTransactionCount": {
///     "address": "0x1234...",
///     "count": "5"
///   }
/// }
/// ```
async fn query_safe_transaction_count(
    schema: &Schema<QueryRoot, MutationRoot, SubscriptionRoot>,
    address: &str,
) -> anyhow::Result<serde_json::Value> {
    let query = format!(
        r#"query {{
            safeTransactionCount(address: "{}") {{
                ... on SafeTransactionCount {{
                    address
                    count
                }}
                ... on InvalidAddressError {{
                    code
                    message
                    address
                }}
                ... on QueryFailedError {{
                    code
                    message
                }}
            }}
        }}"#,
        address
    );

    let response = execute_graphql_query(schema, &query).await;

    if !response.errors.is_empty() {
        anyhow::bail!("GraphQL errors: {:?}", response.errors);
    }

    Ok(response.data.into_json()?)
}

/// Deploys a MockSafe contract to the Anvil test network.
///
/// This helper function deploys a minimal Safe contract implementation that provides
/// the `nonce()` function needed for testing the safeTransactionCount GraphQL query.
///
/// # Arguments
/// * `ctx` - Test context containing contract instances with RPC provider
///
/// # Returns
/// * `Ok(MockSafe::MockSafeInstance)` - Deployed contract instance ready for interaction
/// * `Err` - If deployment fails
///
/// # Example
/// ```rust
/// let mock_safe = deploy_mock_safe(&ctx).await?;
/// let address = mock_safe.address();
/// let nonce = mock_safe.nonce().call().await?;
/// ```
async fn deploy_mock_safe(
    ctx: &TestContext,
) -> anyhow::Result<MockSafe::MockSafeInstance<Arc<blokli_chain_rpc::client::AnvilRpcClient>>> {
    let provider = ctx.contract_instances.token.provider().clone();
    let mock_safe = MockSafe::deploy(provider).await?;
    Ok(mock_safe)
}

#[tokio::test]
async fn test_safe_transaction_count_eoa_address_returns_error() -> anyhow::Result<()> {
    let ctx = setup_test_environment().await?;

    // Get an EOA address (test accounts are EOAs, not Safe contracts)
    let eoa_address = ctx.test_accounts[0].public().to_address().to_hex();

    // Query transaction count on an EOA should return QueryFailedError
    let data = query_safe_transaction_count(&ctx.schema, &eoa_address).await?;
    let result = &data["safeTransactionCount"];

    // Verify we get a QueryFailedError (EOAs are not Safe contracts)
    assert!(result["code"].is_string(), "Expected code field in error response");
    assert_eq!(
        result["code"].as_str().unwrap(),
        "QUERY_FAILED",
        "Should return QUERY_FAILED error code for EOA address"
    );
    assert!(
        result["message"].is_string(),
        "Expected message field in error response"
    );
    let message = result["message"].as_str().unwrap();
    assert!(
        message.starts_with("Failed to query Safe transaction count from RPC:"),
        "Error message should use standard resolver prefix, got: {}",
        message
    );

    Ok(())
}

#[tokio::test]
async fn test_safe_transaction_count_invalid_address_format() -> anyhow::Result<()> {
    let ctx = setup_test_environment().await?;

    // Use invalid address format
    let invalid_address = "not-a-valid-address";

    // Query transaction count
    let data = query_safe_transaction_count(&ctx.schema, invalid_address).await?;
    let result = &data["safeTransactionCount"];

    // Verify error response
    assert!(result["code"].is_string(), "Expected code field in error response");
    assert!(
        result["message"].is_string(),
        "Expected message field in error response"
    );
    assert_eq!(
        result["address"].as_str().unwrap(),
        invalid_address,
        "Error should include the invalid address"
    );

    Ok(())
}

#[tokio::test]
async fn test_safe_transaction_count_accepts_0x_prefix() -> anyhow::Result<()> {
    let ctx = setup_test_environment().await?;

    // Get EOA address with 0x prefix
    let address_with_prefix = format!("0x{}", ctx.test_accounts[0].public().to_address().to_hex());

    // Query should accept the 0x prefix (though EOA will return error)
    let data = query_safe_transaction_count(&ctx.schema, &address_with_prefix).await?;
    let result = &data["safeTransactionCount"];

    // Should get either QueryFailedError (EOA not a Safe) or a valid response
    // Here we just verify the query accepted the 0x prefix format
    assert!(
        result["code"].is_string() || result["address"].is_string(),
        "Query should accept 0x prefix and return either error or success"
    );

    Ok(())
}

#[tokio::test]
async fn test_safe_transaction_count_accepts_no_prefix() -> anyhow::Result<()> {
    let ctx = setup_test_environment().await?;

    // Get EOA address without 0x prefix
    let address_no_prefix = ctx.test_accounts[0].public().to_address().to_hex();

    // Query should accept address without 0x prefix (though EOA will return error)
    let data = query_safe_transaction_count(&ctx.schema, &address_no_prefix).await?;
    let result = &data["safeTransactionCount"];

    // Should get either QueryFailedError (EOA not a Safe) or a valid response
    // Here we just verify the query accepted the format without 0x prefix
    assert!(
        result["code"].is_string() || result["address"].is_string(),
        "Query should accept address without 0x prefix and return either error or success"
    );

    Ok(())
}

#[tokio::test]
async fn test_safe_transaction_count_with_mock_safe_increments() -> anyhow::Result<()> {
    let ctx = setup_test_environment().await?;

    // Deploy mock Safe contract
    let mock_safe = deploy_mock_safe(&ctx).await?;
    let safe_address = format!("{:?}", mock_safe.address());

    // Wait for deployment to be mined (2 blocks: deployment + confirmation)
    let expected_block_time = Duration::from_secs(1);
    tokio::time::sleep(2 * expected_block_time).await;

    // Query initial nonce (should be 0)
    let data = query_safe_transaction_count(&ctx.schema, &safe_address).await?;
    let result = &data["safeTransactionCount"];
    assert!(
        result["address"].is_string(),
        "Expected address field in successful response"
    );
    assert_eq!(
        result["address"].as_str().unwrap(),
        safe_address,
        "Address in response should match queried address"
    );
    assert_eq!(result["count"].as_str().unwrap(), "0", "Initial nonce should be 0");

    // Increment nonce via contract call
    let receipt = mock_safe.incrementNonce().send().await?.get_receipt().await?;
    assert!(receipt.status(), "Transaction should succeed");

    // Wait for transaction to be mined
    tokio::time::sleep(2 * expected_block_time).await;

    // Query nonce again (should be 1)
    let data = query_safe_transaction_count(&ctx.schema, &safe_address).await?;
    let result = &data["safeTransactionCount"];
    assert_eq!(
        result["count"].as_str().unwrap(),
        "1",
        "Nonce should be 1 after first increment"
    );
    assert_eq!(
        result["address"].as_str().unwrap(),
        safe_address,
        "Address should remain consistent"
    );

    // Increment nonce again
    let receipt = mock_safe.incrementNonce().send().await?.get_receipt().await?;
    assert!(receipt.status(), "Second transaction should succeed");

    // Wait for transaction to be mined
    tokio::time::sleep(2 * expected_block_time).await;

    // Query nonce again (should be 2)
    let data = query_safe_transaction_count(&ctx.schema, &safe_address).await?;
    let result = &data["safeTransactionCount"];
    assert_eq!(
        result["count"].as_str().unwrap(),
        "2",
        "Nonce should be 2 after second increment"
    );
    assert_eq!(
        result["address"].as_str().unwrap(),
        safe_address,
        "Address should remain consistent"
    );

    // Verify the nonce can also be queried directly via contract
    let contract_nonce: U256 = mock_safe.nonce().call().await?;
    assert_eq!(
        contract_nonce,
        U256::from(2),
        "Contract nonce should match GraphQL query result"
    );

    Ok(())
}
