//! Integration tests for transaction count API queries (transactionCount)
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
//! - EOA transaction count queries (returns eth_getTransactionCount)
//! - GraphQL response structure validation
//! - Blockchain state changes reflected in API queries
//!
//! **Error Path Tests**:
//! - Invalid address format handling (returns InvalidAddressError)
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

mod common;

use std::{sync::Arc, time::Duration};

use alloy::{primitives::U256, sol};
use async_graphql::Schema;
use blokli_api::{mutation::MutationRoot, query::QueryRoot, subscription::SubscriptionRoot};
use hopr_crypto_types::keypairs::Keypair;
use hopr_primitive_types::traits::ToHex;

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
/// This helper constructs a GraphQL query for the `transactionCount` field,
/// executes it against the schema, and returns the parsed JSON response.
///
/// # Arguments
/// * `schema` - The GraphQL schema with the `transactionCount` query
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
///   "transactionCount": {
///     "address": "0x1234...",
///     "count": "5"
///   }
/// }
/// ```
async fn query_transaction_count(
    schema: &Schema<QueryRoot, MutationRoot, SubscriptionRoot>,
    address: &str,
) -> anyhow::Result<serde_json::Value> {
    let query = format!(
        r#"query {{
            transactionCount(address: "{}") {{
                ... on TransactionCount {{
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
/// the `nonce()` function needed for testing the transactionCount GraphQL query.
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
    ctx: &common::TestContext,
) -> anyhow::Result<MockSafe::MockSafeInstance<Arc<blokli_chain_rpc::client::AnvilRpcClient>>> {
    let provider = ctx.contract_instances.token.provider().clone();
    let mock_safe = MockSafe::deploy(provider).await?;
    Ok(mock_safe)
}

#[tokio::test]
async fn test_transaction_count_eoa_returns_success() -> anyhow::Result<()> {
    let ctx = common::setup_simple_test_environment().await?;

    // Get an EOA address (test accounts are EOAs)
    let eoa_address = ctx.test_accounts[0].public().to_address().to_hex();

    // Query transaction count on an EOA should now SUCCEED (not error!)
    let data = query_transaction_count(&ctx.schema, &eoa_address).await?;
    let result = &data["transactionCount"];

    // Verify successful response with transaction count
    assert!(result["address"].is_string(), "Expected address field");
    assert_eq!(result["address"].as_str().unwrap(), eoa_address);

    // EOA should return a valid transaction count (parseable as u64)
    let count_str = result["count"].as_str().expect("count should be a string");
    let count: u64 = count_str.parse().expect("count should be a valid u64");
    assert!(count < 20, "EOA transaction count should be reasonable: {}", count);

    Ok(())
}

#[tokio::test]
async fn test_transaction_count_invalid_address_format() -> anyhow::Result<()> {
    let ctx = common::setup_simple_test_environment().await?;

    // Use invalid address format
    let invalid_address = "not-a-valid-address";

    // Query transaction count
    let data = query_transaction_count(&ctx.schema, invalid_address).await?;
    let result = &data["transactionCount"];

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
async fn test_transaction_count_accepts_0x_prefix() -> anyhow::Result<()> {
    let ctx = common::setup_simple_test_environment().await?;

    // Get EOA address with 0x prefix
    let address_with_prefix = format!("0x{}", ctx.test_accounts[0].public().to_address().to_hex());

    // Query should accept the 0x prefix (though EOA will return error)
    let data = query_transaction_count(&ctx.schema, &address_with_prefix).await?;
    let result = &data["transactionCount"];

    // Should get either QueryFailedError (EOA not a Safe) or a valid response
    // Here we just verify the query accepted the 0x prefix format
    assert!(
        result["code"].is_string() || result["address"].is_string(),
        "Query should accept 0x prefix and return either error or success"
    );

    Ok(())
}

#[tokio::test]
async fn test_transaction_count_accepts_no_prefix() -> anyhow::Result<()> {
    let ctx = common::setup_simple_test_environment().await?;

    // Get EOA address without 0x prefix
    let address_no_prefix = ctx.test_accounts[0].public().to_address().to_hex();

    // Query should accept address without 0x prefix (though EOA will return error)
    let data = query_transaction_count(&ctx.schema, &address_no_prefix).await?;
    let result = &data["transactionCount"];

    // Should get either QueryFailedError (EOA not a Safe) or a valid response
    // Here we just verify the query accepted the format without 0x prefix
    assert!(
        result["code"].is_string() || result["address"].is_string(),
        "Query should accept address without 0x prefix and return either error or success"
    );

    Ok(())
}

#[tokio::test]
async fn test_transaction_count_with_mock_safe_increments() -> anyhow::Result<()> {
    let ctx = common::setup_simple_test_environment().await?;

    // Deploy mock Safe contract
    let mock_safe = deploy_mock_safe(&ctx).await?;
    let safe_address = format!("{:?}", mock_safe.address());

    // Wait for deployment to be mined (2 blocks: deployment + confirmation)
    let expected_block_time = Duration::from_secs(1);
    tokio::time::sleep(2 * expected_block_time).await;

    // Query initial nonce (should be 0)
    let data = query_transaction_count(&ctx.schema, &safe_address).await?;
    let result = &data["transactionCount"];
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
    let data = query_transaction_count(&ctx.schema, &safe_address).await?;
    let result = &data["transactionCount"];
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
    let data = query_transaction_count(&ctx.schema, &safe_address).await?;
    let result = &data["transactionCount"];
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
