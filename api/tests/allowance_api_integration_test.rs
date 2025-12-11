//! Integration tests for Safe HOPR allowance API queries (safeHoprAllowance)
//!
//! These tests verify end-to-end functionality by:
//! - Running a real Anvil instance with deployed HOPR contracts
//! - Executing GraphQL queries against the schema
//! - Verifying allowance values are correctly fetched from the blockchain via RPC

mod common;

use std::{str::FromStr, time::Duration};

use alloy::primitives::{Address as AlloyAddress, U256};
use async_graphql::Schema;
use blokli_api::{mutation::MutationRoot, query::QueryRoot, subscription::SubscriptionRoot};
use blokli_chain_types::AlloyAddressExt;
use hopr_crypto_types::keypairs::Keypair;
use hopr_primitive_types::{prelude::HoprBalance, traits::ToHex};

/// Execute a GraphQL query against the schema and return the response
async fn execute_graphql_query(
    schema: &Schema<QueryRoot, MutationRoot, SubscriptionRoot>,
    query: &str,
) -> async_graphql::Response {
    schema.execute(query).await
}

/// Helper to query Safe HOPR allowance via GraphQL
async fn query_safe_hopr_allowance(
    schema: &Schema<QueryRoot, MutationRoot, SubscriptionRoot>,
    address: &str,
) -> anyhow::Result<Option<String>> {
    let query = format!(
        r#"query {{
            safeHoprAllowance(address: "{}") {{
                __typename
                ... on SafeHoprAllowance {{ address allowance }}
                ... on InvalidAddressError {{ code message address }}
                ... on QueryFailedError {{ code message }}
            }}
        }}"#,
        address
    );

    let response = execute_graphql_query(schema, &query).await;

    if !response.errors.is_empty() {
        anyhow::bail!("GraphQL errors: {:?}", response.errors);
    }

    let data = response.data.into_json()?;

    match data["safeHoprAllowance"]["__typename"].as_str() {
        Some("SafeHoprAllowance") => Ok(data["safeHoprAllowance"]["allowance"].as_str().map(|s| s.to_string())),
        Some("InvalidAddressError") => anyhow::bail!(
            "Invalid address error: {}",
            data["safeHoprAllowance"]["message"].as_str().unwrap_or("unknown error")
        ),
        Some("QueryFailedError") => anyhow::bail!(
            "Query failed error: {}",
            data["safeHoprAllowance"]["message"].as_str().unwrap_or("unknown error")
        ),
        _ => anyhow::bail!("Unknown result type"),
    }
}

/// Comprehensive allowance API integration test covering all scenarios.
/// This consolidated test runs setup once for optimal performance.
#[test_log::test(tokio::test)]
async fn test_allowance_api_integration() -> anyhow::Result<()> {
    let ctx = common::setup_simple_test_environment().await?;

    // ========================================
    // Phase 1: Input Validation Tests
    // ========================================

    // Test Case 1: Invalid address formats should return InvalidAddressError variant
    {
        let invalid_addresses = vec![
            "not_an_address",
            "0xinvalid",
            "0x123",                                      // Too short
            "0xZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZ", // Invalid hex
        ];

        for invalid_addr in invalid_addresses {
            let query = format!(
                r#"query {{
                    safeHoprAllowance(address: "{}") {{
                        __typename
                        ... on SafeHoprAllowance {{ address allowance }}
                        ... on InvalidAddressError {{ code message address }}
                        ... on QueryFailedError {{ code message }}
                    }}
                }}"#,
                invalid_addr
            );

            let response = execute_graphql_query(&ctx.schema, &query).await;

            // Should not have GraphQL errors
            assert!(
                response.errors.is_empty(),
                "Query should not have GraphQL errors for invalid address '{}'",
                invalid_addr
            );

            let data = response.data.into_json().expect("Should have data");

            // Should return InvalidAddressError variant
            assert_eq!(
                data["safeHoprAllowance"]["__typename"].as_str(),
                Some("InvalidAddressError"),
                "Invalid address '{}' should return InvalidAddressError variant",
                invalid_addr
            );

            // Should have error code
            assert_eq!(
                data["safeHoprAllowance"]["code"].as_str(),
                Some("INVALID_ADDRESS"),
                "Should have INVALID_ADDRESS error code"
            );
        }
    }

    // Test Case 2: Address without 0x prefix should be valid
    {
        let test_addr = ctx.test_accounts[0].public().to_address();
        let addr_without_prefix = test_addr.to_hex().trim_start_matches("0x").to_string();

        let allowance = query_safe_hopr_allowance(&ctx.schema, &addr_without_prefix).await?;

        assert!(
            allowance.is_some(),
            "Allowance should be returned for address without 0x prefix"
        );
        let allowance_str = allowance.unwrap();
        let _parsed_allowance =
            HoprBalance::from_str(&allowance_str).expect("Allowance string should be valid HoprBalance format");
    }

    // ========================================
    // Phase 2: Zero Allowance Query Tests
    // ========================================

    // Test Case 3: Query allowance for Safe that hasn't approved channels contract
    {
        let test_addr = ctx.test_accounts[2].public().to_address();
        let allowance = query_safe_hopr_allowance(&ctx.schema, &test_addr.to_hex()).await?;

        assert!(allowance.is_some(), "Allowance should be returned");
        let allowance_str = allowance.unwrap();
        let parsed_allowance =
            HoprBalance::from_str(&allowance_str).expect("Allowance string should be valid HoprBalance format");
        let expected_allowance = HoprBalance::from_str("0 wxHOPR").unwrap();
        assert_eq!(parsed_allowance, expected_allowance, "Allowance should be zero");
    }

    // ========================================
    // Phase 3: Non-Zero Allowance Tests
    // ========================================

    // Test Case 4: Approve channels contract and verify allowance
    {
        let safe_addr = ctx.test_accounts[0].public().to_address();
        let channels_addr = ctx.contract_addrs.channels;

        // Mint tokens to the Safe first
        let token_amount = U256::from(1000) * U256::from(10).pow(U256::from(18));
        blokli_chain_types::utils::mint_tokens(ctx.contract_instances.token.clone(), token_amount)
            .await
            .expect("Minting should succeed");

        blokli_chain_types::utils::fund_node(
            safe_addr,
            U256::ZERO,
            token_amount,
            ctx.contract_instances.token.clone(),
        )
        .await
        .expect("Funding should succeed");

        // Approve the channels contract to spend tokens
        let approval_amount = U256::from(500) * U256::from(10).pow(U256::from(18));
        let channels_alloy_addr = AlloyAddress::from_hopr_address(channels_addr);
        ctx.contract_instances
            .token
            .approve(channels_alloy_addr, approval_amount)
            .send()
            .await
            .expect("Approval transaction should succeed")
            .watch()
            .await
            .expect("Approval should be confirmed");

        // Query allowance via GraphQL
        let allowance = query_safe_hopr_allowance(&ctx.schema, &safe_addr.to_hex()).await?;

        // Verify allowance
        assert!(allowance.is_some(), "Allowance should be returned");
        let allowance_str = allowance.unwrap();
        tracing::info!("HOPR allowance: {allowance_str}");

        let parsed_allowance =
            HoprBalance::from_str(&allowance_str).expect("Allowance string should be valid HoprBalance format");
        let expected_allowance = HoprBalance::from_str("500 wxHOPR").unwrap();
        assert_eq!(parsed_allowance, expected_allowance, "Allowance mismatch");
    }

    // ========================================
    // Phase 4: Multi-Query Scenario Tests
    // ========================================

    // Test Case 5: Query allowance and balance in single GraphQL request
    {
        let safe_addr = ctx.test_accounts[0].public().to_address();
        let addr_str = safe_addr.to_hex();

        let query = format!(
            r#"query {{
                safeHoprAllowance(address: "{}") {{
                    __typename
                    ... on SafeHoprAllowance {{ address allowance }}
                    ... on InvalidAddressError {{ code message address }}
                    ... on QueryFailedError {{ code message }}
                }}
                hoprBalance(address: "{}") {{
                    __typename
                    ... on HoprBalance {{ address balance }}
                    ... on InvalidAddressError {{ code message address }}
                    ... on QueryFailedError {{ code message }}
                }}
            }}"#,
            addr_str, addr_str
        );

        let response = execute_graphql_query(&ctx.schema, &query).await;

        assert!(response.errors.is_empty(), "Query should succeed without errors");

        let data = response.data.into_json()?;

        // Check safeHoprAllowance is SafeHoprAllowance variant
        assert_eq!(
            data["safeHoprAllowance"]["__typename"].as_str(),
            Some("SafeHoprAllowance"),
            "Should return SafeHoprAllowance variant"
        );
        assert!(
            data["safeHoprAllowance"]["allowance"].is_string(),
            "Allowance should be returned"
        );
        let allowance_str = data["safeHoprAllowance"]["allowance"].as_str().unwrap();
        let _allowance_parsed =
            HoprBalance::from_str(allowance_str).expect("Allowance string should be valid HoprBalance format");

        // Check hoprBalance is HoprBalance variant
        assert_eq!(
            data["hoprBalance"]["__typename"].as_str(),
            Some("HoprBalance"),
            "Should return HoprBalance variant"
        );
        assert!(data["hoprBalance"]["balance"].is_string(), "Balance should be returned");
        let balance_str = data["hoprBalance"]["balance"].as_str().unwrap();
        let _balance_parsed =
            HoprBalance::from_str(balance_str).expect("Balance string should be valid HoprBalance format");
    }

    // ========================================
    // Phase 5: Concurrency & Performance Tests
    // ========================================

    // Test Case 6: Concurrent allowance queries
    {
        let mut handles = vec![];
        for account in &ctx.test_accounts {
            let schema = ctx.schema.clone();
            let address = account.public().to_address().to_hex();

            handles.push(tokio::spawn(async move {
                query_safe_hopr_allowance(&schema, &address).await
            }));
        }

        let results: Vec<_> = futures::future::join_all(handles).await;

        for result in results {
            assert!(result.is_ok(), "Concurrent query task should not panic");
            let allowance = result.unwrap().expect("Concurrent query should succeed");
            if let Some(allowance_str) = allowance {
                let _parsed =
                    HoprBalance::from_str(&allowance_str).expect("Allowance string should be valid HoprBalance format");
            }
        }
    }

    // Test Case 7: Query performance measurement
    {
        let safe_addr = ctx.test_accounts[0].public().to_address().to_hex();

        let start = std::time::Instant::now();
        let allowance = query_safe_hopr_allowance(&ctx.schema, &safe_addr).await?;
        let duration = start.elapsed();

        if let Some(allowance_str) = allowance {
            let _parsed =
                HoprBalance::from_str(&allowance_str).expect("Allowance string should be valid HoprBalance format");
        }

        assert!(
            duration < Duration::from_millis(10),
            "Allowance query took too long: {:?}",
            duration
        );

        println!("Allowance query completed in: {:?}", duration);
    }

    Ok(())
}
