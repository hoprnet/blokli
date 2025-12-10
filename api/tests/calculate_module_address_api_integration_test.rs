//! Integration tests for calculateModuleAddress GraphQL API query
//!
//! These tests verify end-to-end functionality by:
//! - Running a real Anvil instance with deployed HOPR contracts
//! - Executing GraphQL queries against the schema
//! - Verifying module addresses are correctly calculated via RPC
//!
//! ## Test Coverage
//!
//! **Positive Path Tests**:
//! - Successful query with valid inputs
//! - Address format variations (with/without 0x prefix)
//! - Different nonce values (0, large numbers)
//! - Deterministic calculation verification
//! - Direct contract call comparison
//!
//! **Error Path Tests**:
//! - Invalid owner address format (returns InvalidAddressError)
//! - Invalid safe address format (returns InvalidAddressError)
//! - Empty addresses (returns InvalidAddressError)
//! - Address format validation
//!
//! ## Architecture
//!
//! The tests use the real HoprNodeStakeFactory contract deployed via
//! `ContractInstances::deploy_for_testing`. This approach:
//! - Tests the full integration path (GraphQL → RPC → Blockchain)
//! - Verifies production contract behavior
//! - Runs fast with no external dependencies
//! - Ensures deterministic CREATE2 address calculation

mod common;

use alloy::primitives::{Address as AlloyAddress, FixedBytes, U256};
use async_graphql::Schema;
use blokli_api::{mutation::MutationRoot, query::QueryRoot, subscription::SubscriptionRoot};
use blokli_chain_types::AlloyAddressExt;
use hopr_crypto_types::keypairs::Keypair;
use hopr_primitive_types::traits::ToHex;

/// Executes a GraphQL query against the provided schema.
async fn execute_graphql_query(
    schema: &Schema<QueryRoot, MutationRoot, SubscriptionRoot>,
    query: &str,
) -> async_graphql::Response {
    schema.execute(query).await
}

/// Queries the calculated module address via GraphQL.
async fn query_calculate_module_address(
    schema: &Schema<QueryRoot, MutationRoot, SubscriptionRoot>,
    owner: &str,
    nonce: u64,
    safe_address: &str,
) -> anyhow::Result<serde_json::Value> {
    let query = format!(
        r#"query {{
            calculateModuleAddress(owner: "{}", nonce: "{}", safeAddress: "{}") {{
                ... on ModuleAddress {{
                    moduleAddress
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
        owner, nonce, safe_address
    );

    let response = execute_graphql_query(schema, &query).await;

    if !response.errors.is_empty() {
        anyhow::bail!("GraphQL errors: {:?}", response.errors);
    }

    Ok(response.data.into_json()?)
}

#[tokio::test]
async fn test_calculate_module_address_success() -> anyhow::Result<()> {
    let ctx = common::setup_simple_test_environment().await?;

    let owner_address = ctx.test_accounts[0].public().to_address().to_hex();
    let safe_address = ctx.test_accounts[1].public().to_address().to_hex();

    // Query module address
    let data = query_calculate_module_address(&ctx.schema, &owner_address, 0, &safe_address).await?;
    let result = &data["calculateModuleAddress"];

    // Verify successful response
    assert!(
        result["moduleAddress"].is_string(),
        "Expected moduleAddress field in successful response"
    );

    let module_address = result["moduleAddress"].as_str().unwrap();

    // Verify address format (42 characters: 0x + 40 hex chars)
    assert_eq!(
        module_address.len(),
        42,
        "Module address should be 42 characters (0x + 40 hex chars)"
    );

    // Verify it starts with 0x
    assert!(
        module_address.starts_with("0x"),
        "Module address should start with 0x"
    );

    // Verify it's not zero address
    assert_ne!(
        module_address,
        "0x0000000000000000000000000000000000000000",
        "Module address should not be zero address"
    );

    // Verify all characters after 0x are hex
    let hex_part = &module_address[2..];
    assert!(
        hex_part.chars().all(|c| c.is_ascii_hexdigit()),
        "Module address should contain only hex characters after 0x prefix"
    );

    Ok(())
}

#[tokio::test]
async fn test_calculate_module_address_invalid_owner_format() -> anyhow::Result<()> {
    let ctx = common::setup_simple_test_environment().await?;

    let invalid_owner = "not-a-valid-address";
    let safe_address = ctx.test_accounts[1].public().to_address().to_hex();

    // Query with invalid owner
    let data = query_calculate_module_address(&ctx.schema, invalid_owner, 0, &safe_address).await?;
    let result = &data["calculateModuleAddress"];

    // Verify error response
    assert!(result["code"].is_string(), "Expected code field in error response");
    assert_eq!(
        result["code"].as_str().unwrap(),
        "INVALID_ADDRESS",
        "Should return INVALID_ADDRESS error code"
    );
    assert!(
        result["message"].is_string(),
        "Expected message field in error response"
    );
    assert_eq!(
        result["address"].as_str().unwrap(),
        invalid_owner,
        "Error should include the invalid address"
    );

    Ok(())
}

#[tokio::test]
async fn test_calculate_module_address_invalid_safe_format() -> anyhow::Result<()> {
    let ctx = common::setup_simple_test_environment().await?;

    let owner_address = ctx.test_accounts[0].public().to_address().to_hex();
    let invalid_safe = "0xZZZ";

    // Query with invalid safe address
    let data = query_calculate_module_address(&ctx.schema, &owner_address, 0, invalid_safe).await?;
    let result = &data["calculateModuleAddress"];

    // Verify error response
    assert!(result["code"].is_string(), "Expected code field in error response");
    assert_eq!(
        result["code"].as_str().unwrap(),
        "INVALID_ADDRESS",
        "Should return INVALID_ADDRESS error code"
    );
    assert!(
        result["message"].is_string(),
        "Expected message field in error response"
    );
    assert_eq!(
        result["address"].as_str().unwrap(),
        invalid_safe,
        "Error should include the invalid address"
    );

    Ok(())
}

#[tokio::test]
async fn test_calculate_module_address_empty_owner() -> anyhow::Result<()> {
    let ctx = common::setup_simple_test_environment().await?;

    let empty_owner = "";
    let safe_address = ctx.test_accounts[1].public().to_address().to_hex();

    // Query with empty owner
    let data = query_calculate_module_address(&ctx.schema, empty_owner, 0, &safe_address).await?;
    let result = &data["calculateModuleAddress"];

    // Verify error response
    assert!(result["code"].is_string(), "Expected code field in error response");
    assert_eq!(
        result["code"].as_str().unwrap(),
        "INVALID_ADDRESS",
        "Should return INVALID_ADDRESS error code for empty address"
    );

    Ok(())
}

#[tokio::test]
async fn test_calculate_module_address_empty_safe() -> anyhow::Result<()> {
    let ctx = common::setup_simple_test_environment().await?;

    let owner_address = ctx.test_accounts[0].public().to_address().to_hex();
    let empty_safe = "";

    // Query with empty safe address
    let data = query_calculate_module_address(&ctx.schema, &owner_address, 0, empty_safe).await?;
    let result = &data["calculateModuleAddress"];

    // Verify error response
    assert!(result["code"].is_string(), "Expected code field in error response");
    assert_eq!(
        result["code"].as_str().unwrap(),
        "INVALID_ADDRESS",
        "Should return INVALID_ADDRESS error code for empty address"
    );

    Ok(())
}

#[tokio::test]
async fn test_calculate_module_address_accepts_0x_prefix() -> anyhow::Result<()> {
    let ctx = common::setup_simple_test_environment().await?;

    // to_hex() already includes 0x prefix
    let owner_address = ctx.test_accounts[0].public().to_address().to_hex();
    let safe_address = ctx.test_accounts[1].public().to_address().to_hex();

    // Verify addresses have 0x prefix
    assert!(owner_address.starts_with("0x"));
    assert!(safe_address.starts_with("0x"));

    // Query with 0x prefix
    let data = query_calculate_module_address(&ctx.schema, &owner_address, 0, &safe_address).await?;
    let result = &data["calculateModuleAddress"];

    // Should succeed
    assert!(
        result["moduleAddress"].is_string(),
        "Query should accept 0x prefix and return success"
    );

    Ok(())
}

#[tokio::test]
async fn test_calculate_module_address_accepts_no_prefix() -> anyhow::Result<()> {
    let ctx = common::setup_simple_test_environment().await?;

    // Strip 0x prefix from addresses
    let owner_address = ctx.test_accounts[0].public().to_address().to_hex().trim_start_matches("0x").to_string();
    let safe_address = ctx.test_accounts[1].public().to_address().to_hex().trim_start_matches("0x").to_string();

    // Query without 0x prefix
    let data = query_calculate_module_address(&ctx.schema, &owner_address, 0, &safe_address).await?;
    let result = &data["calculateModuleAddress"];

    // Should succeed
    assert!(
        result["moduleAddress"].is_string(),
        "Query should accept address without 0x prefix and return success"
    );

    Ok(())
}

#[tokio::test]
async fn test_calculate_module_address_with_large_nonce() -> anyhow::Result<()> {
    let ctx = common::setup_simple_test_environment().await?;

    let owner_address = ctx.test_accounts[0].public().to_address().to_hex();
    let safe_address = ctx.test_accounts[1].public().to_address().to_hex();

    // Use large nonce
    let large_nonce = u64::MAX;

    let data = query_calculate_module_address(&ctx.schema, &owner_address, large_nonce, &safe_address).await?;
    let result = &data["calculateModuleAddress"];

    // Should succeed with large nonce
    assert!(
        result["moduleAddress"].is_string(),
        "Query should handle large nonce values"
    );

    Ok(())
}

#[tokio::test]
async fn test_calculate_module_address_with_zero_nonce() -> anyhow::Result<()> {
    let ctx = common::setup_simple_test_environment().await?;

    let owner_address = ctx.test_accounts[0].public().to_address().to_hex();
    let safe_address = ctx.test_accounts[1].public().to_address().to_hex();

    // Use nonce = 0
    let data = query_calculate_module_address(&ctx.schema, &owner_address, 0, &safe_address).await?;
    let result = &data["calculateModuleAddress"];

    // Should succeed
    assert!(
        result["moduleAddress"].is_string(),
        "Query should handle nonce = 0"
    );

    Ok(())
}

#[tokio::test]
async fn test_calculate_module_address_deterministic_via_api() -> anyhow::Result<()> {
    let ctx = common::setup_simple_test_environment().await?;

    let owner_address = ctx.test_accounts[0].public().to_address().to_hex();
    let safe_address = ctx.test_accounts[1].public().to_address().to_hex();
    let nonce = 42u64;

    // Query three times with identical parameters
    let data1 = query_calculate_module_address(&ctx.schema, &owner_address, nonce, &safe_address).await?;
    let data2 = query_calculate_module_address(&ctx.schema, &owner_address, nonce, &safe_address).await?;
    let data3 = query_calculate_module_address(&ctx.schema, &owner_address, nonce, &safe_address).await?;

    let address1 = data1["calculateModuleAddress"]["moduleAddress"].as_str().unwrap();
    let address2 = data2["calculateModuleAddress"]["moduleAddress"].as_str().unwrap();
    let address3 = data3["calculateModuleAddress"]["moduleAddress"].as_str().unwrap();

    // Verify all three responses return identical addresses
    assert_eq!(
        address1, address2,
        "Same inputs should produce same output (query 1 vs query 2)"
    );
    assert_eq!(
        address1, address3,
        "Same inputs should produce same output (query 1 vs query 3)"
    );

    Ok(())
}

#[tokio::test]
async fn test_calculate_module_address_matches_direct_contract_call() -> anyhow::Result<()> {
    let ctx = common::setup_simple_test_environment().await?;

    let owner_hopr = ctx.test_accounts[0].public().to_address();
    let safe_hopr = ctx.test_accounts[1].public().to_address();
    let nonce = 5u64;

    // Query via GraphQL API
    let owner_hex = owner_hopr.to_hex();
    let safe_hex = safe_hopr.to_hex();

    let data = query_calculate_module_address(&ctx.schema, &owner_hex, nonce, &safe_hex).await?;
    let api_address = data["calculateModuleAddress"]["moduleAddress"].as_str().unwrap();

    // Call directly via contract
    let channels_addr_bytes = ctx.contract_addrs.channels.as_ref();
    let capability_permissions: [u8; 12] = [0x01, 0x01, 0x03, 0x03, 0x03, 0x03, 0x03, 0x03, 0x03, 0x03, 0x03, 0x03];
    let mut default_target = [0u8; 32];
    default_target[0..20].copy_from_slice(channels_addr_bytes);
    default_target[20..32].copy_from_slice(&capability_permissions);

    let contract_address = ctx
        .contract_instances
        .stake_factory
        .predictModuleAddress_1(
            AlloyAddress::from_hopr_address(owner_hopr),
            U256::from(nonce),
            AlloyAddress::from_hopr_address(safe_hopr),
            FixedBytes::from(default_target),
        )
        .call()
        .await?
        .0;

    let contract_address_hex = AlloyAddress::from(contract_address).to_hopr_address().to_hex();

    // Verify API result matches direct contract call
    assert_eq!(
        api_address, contract_address_hex,
        "API result should match direct contract call"
    );

    Ok(())
}

#[tokio::test]
async fn test_calculate_module_address_with_different_parameters() -> anyhow::Result<()> {
    // This test needs 4 accounts
    let mut config = common::TestEnvironmentConfig::default();
    config.num_test_accounts = 4;
    let ctx = common::setup_test_environment(config).await?;

    // Query with different parameter combinations
    let owner_a = ctx.test_accounts[0].public().to_address().to_hex();
    let owner_b = ctx.test_accounts[2].public().to_address().to_hex();
    let safe_a = ctx.test_accounts[1].public().to_address().to_hex();
    let safe_b = ctx.test_accounts[3].public().to_address().to_hex();

    let data1 = query_calculate_module_address(&ctx.schema, &owner_a, 0, &safe_a).await?;
    let data2 = query_calculate_module_address(&ctx.schema, &owner_b, 5, &safe_b).await?;
    let data3 = query_calculate_module_address(&ctx.schema, &owner_a, 1, &safe_a).await?;

    let address1 = data1["calculateModuleAddress"]["moduleAddress"].as_str().unwrap();
    let address2 = data2["calculateModuleAddress"]["moduleAddress"].as_str().unwrap();
    let address3 = data3["calculateModuleAddress"]["moduleAddress"].as_str().unwrap();

    // Verify each query returns a different, valid module address
    assert_ne!(address1, address2, "Different parameters should produce different addresses");
    assert_ne!(address1, address3, "Different parameters should produce different addresses");
    assert_ne!(address2, address3, "Different parameters should produce different addresses");

    // All addresses should be valid (42 chars with 0x prefix)
    assert_eq!(address1.len(), 42, "Address 1 should be valid");
    assert_eq!(address2.len(), 42, "Address 2 should be valid");
    assert_eq!(address3.len(), 42, "Address 3 should be valid");

    Ok(())
}
