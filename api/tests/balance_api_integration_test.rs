//! Integration tests for balance API queries (hoprBalance and nativeBalance)
//!
//! These tests verify end-to-end functionality by:
//! - Running a real Anvil instance with deployed HOPR contracts
//! - Executing GraphQL queries against the schema
//! - Verifying balance values are correctly fetched from the blockchain via RPC

mod common;

use std::{
    str::FromStr,
    time::{Duration, Instant},
};

use async_graphql::Schema;
use blokli_api::{mutation::MutationRoot, query::QueryRoot, subscription::SubscriptionRoot};
use hopr_bindings::exports::alloy::primitives::U256;
use hopr_crypto_types::keypairs::Keypair;
use hopr_primitive_types::{
    prelude::{HoprBalance, XDaiBalance},
    traits::ToHex,
};

/// Execute a GraphQL query against the schema and return the response
async fn execute_graphql_query(
    schema: &Schema<QueryRoot, MutationRoot, SubscriptionRoot>,
    query: &str,
) -> async_graphql::Response {
    schema.execute(query).await
}

/// Helper to query HOPR balance via GraphQL
async fn query_hopr_balance(
    schema: &Schema<QueryRoot, MutationRoot, SubscriptionRoot>,
    address: &str,
) -> anyhow::Result<Option<String>> {
    let query = format!(
        r#"query {{
            hoprBalance(address: "{}") {{
                __typename
                ... on HoprBalance {{ address balance }}
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

    match data["hoprBalance"]["__typename"].as_str() {
        Some("HoprBalance") => Ok(data["hoprBalance"]["balance"].as_str().map(|s| s.to_string())),
        Some("InvalidAddressError") => anyhow::bail!(
            "Invalid address error: {}",
            data["hoprBalance"]["message"].as_str().unwrap_or("unknown error")
        ),
        Some("QueryFailedError") => anyhow::bail!(
            "Query failed error: {}",
            data["hoprBalance"]["message"].as_str().unwrap_or("unknown error")
        ),
        _ => anyhow::bail!("Unknown result type"),
    }
}

/// Helper to query native balance via GraphQL
async fn query_native_balance(
    schema: &Schema<QueryRoot, MutationRoot, SubscriptionRoot>,
    address: &str,
) -> anyhow::Result<Option<String>> {
    let query = format!(
        r#"query {{
            nativeBalance(address: "{}") {{
                __typename
                ... on NativeBalance {{ address balance }}
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

    match data["nativeBalance"]["__typename"].as_str() {
        Some("NativeBalance") => Ok(data["nativeBalance"]["balance"].as_str().map(|s| s.to_string())),
        Some("InvalidAddressError") => anyhow::bail!(
            "Invalid address error: {}",
            data["nativeBalance"]["message"].as_str().unwrap_or("unknown error")
        ),
        Some("QueryFailedError") => anyhow::bail!(
            "Query failed error: {}",
            data["nativeBalance"]["message"].as_str().unwrap_or("unknown error")
        ),
        _ => anyhow::bail!("Unknown result type"),
    }
}

/// Helper to query Safe HOPR allowance via GraphQL
#[allow(unused)] // TODO: not yet used in the tests
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

/// Comprehensive balance API integration test covering all scenarios.
/// This consolidated test runs setup once for optimal performance.
#[test_log::test(tokio::test)]
async fn test_balance_api_integration() -> anyhow::Result<()> {
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
                    hoprBalance(address: "{}") {{
                        __typename
                        ... on HoprBalance {{ address balance }}
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
                data["hoprBalance"]["__typename"].as_str(),
                Some("InvalidAddressError"),
                "Invalid address '{}' should return InvalidAddressError variant",
                invalid_addr
            );

            // Should have error code
            assert_eq!(
                data["hoprBalance"]["code"].as_str(),
                Some("INVALID_ADDRESS"),
                "Should have INVALID_ADDRESS error code"
            );
        }
    }

    // Test Case 2: Address without 0x prefix should be valid
    {
        let test_addr = ctx.test_accounts[0].public().to_address();
        let addr_without_prefix = test_addr.to_hex().trim_start_matches("0x").to_string();

        let balance = query_hopr_balance(&ctx.schema, &addr_without_prefix).await?;

        assert!(
            balance.is_some(),
            "Balance should be returned for address without 0x prefix"
        );
        let balance_str = balance.unwrap();
        let _parsed_balance =
            HoprBalance::from_str(&balance_str).expect("Balance string should be valid HoprBalance format");
    }

    // ========================================
    // Phase 2: Zero Balance Query Tests
    // ========================================

    // Test Case 3: Query HOPR balance for unfunded account
    {
        let test_addr = ctx.test_accounts[2].public().to_address();
        let balance = query_hopr_balance(&ctx.schema, &test_addr.to_hex()).await?;

        assert!(balance.is_some(), "Balance should be returned");
        let balance_str = balance.unwrap();
        let parsed_balance =
            HoprBalance::from_str(&balance_str).expect("Balance string should be valid HoprBalance format");
        let expected_balance = HoprBalance::from_str("0 wxHOPR").unwrap();
        assert_eq!(parsed_balance, expected_balance, "Balance should be zero");
    }

    // Test Case 4: Query native balance for zero address
    {
        let zero_address = "0x0000000000000000000000000000000000000001";
        let balance = query_native_balance(&ctx.schema, zero_address).await?;

        assert!(balance.is_some(), "Balance should be returned");
        let balance_str = balance.unwrap();
        let parsed_balance =
            XDaiBalance::from_str(&balance_str).expect("Balance string should be valid XDaiBalance format");
        let expected_balance = XDaiBalance::from_str("0 xDai").unwrap();
        assert_eq!(parsed_balance, expected_balance, "Balance should be zero");
    }

    // ========================================
    // Phase 3: Funded Balance Operation Tests
    // ========================================

    // Test Case 5: Mint and fund HOPR tokens, then query balance
    {
        // Mint 1000 HOPR tokens in wei (1000 * 10^18)
        let token_amount = U256::from(1000) * U256::from(10).pow(U256::from(18));
        blokli_chain_types::utils::mint_tokens(ctx.contract_instances.token.clone(), token_amount)
            .await
            .expect("Minting should return ok");

        // Fund 1000 HOPR tokens to test account (in wei)
        let test_addr = ctx.test_accounts[0].public().to_address();
        blokli_chain_types::utils::fund_node(
            test_addr,
            U256::ZERO,
            token_amount,
            ctx.contract_instances.token.clone(),
        )
        .await
        .expect("Funding should return ok");

        // Query balance via GraphQL
        let balance = query_hopr_balance(&ctx.schema, &test_addr.to_hex()).await?;

        // Verify balance by parsing into HoprBalance type
        assert!(balance.is_some(), "Balance should be returned");
        let balance_str = balance.unwrap();
        tracing::info!("HOPR balance: {balance_str}");

        let parsed_balance =
            HoprBalance::from_str(&balance_str).expect("Balance string should be valid HoprBalance format");
        let expected_balance = HoprBalance::from_str("1000 wxHOPR").unwrap();
        assert_eq!(parsed_balance, expected_balance, "Balance mismatch");
    }

    // Test Case 6: Fund native tokens and query balance
    {
        // Fund 1000 xDai to test account (in wei: 1000 * 10^18)
        let test_addr = ctx.test_accounts[0].public().to_address();
        let native_amount = U256::from(1000) * U256::from(10).pow(U256::from(18));
        blokli_chain_types::utils::fund_node(
            test_addr,
            native_amount,
            U256::ZERO,
            ctx.contract_instances.token.clone(),
        )
        .await
        .expect("Funding should return ok");

        let balance = query_native_balance(&ctx.schema, &test_addr.to_hex()).await?;

        // Verify balance exists and is non-zero
        assert!(balance.is_some(), "Balance should be returned");
        let balance_str = balance.unwrap();
        tracing::info!("Native balance: {balance_str}");
        let parsed_balance =
            XDaiBalance::from_str(&balance_str).expect("Balance string should be valid XDaiBalance format");

        // Account should have at least 1000 xDai (Anvil pre-funds accounts with ~10000 ETH)
        let minimum_expected = XDaiBalance::from_str("1000 xDai").unwrap();
        assert!(
            parsed_balance >= minimum_expected,
            "Balance should be at least 1000 xDai, got: {}",
            balance_str
        );
    }

    // ========================================
    // Phase 4: Multi-Query Scenario Tests
    // ========================================

    // Test Case 7: Query both balance types in single GraphQL request
    {
        let test_addr = ctx.test_accounts[0].public().to_address();
        let addr_str = test_addr.to_hex();

        let query = format!(
            r#"query {{
                hoprBalance(address: "{}") {{
                    __typename
                    ... on HoprBalance {{ address balance }}
                    ... on InvalidAddressError {{ code message address }}
                    ... on QueryFailedError {{ code message }}
                }}
                nativeBalance(address: "{}") {{
                    __typename
                    ... on NativeBalance {{ address balance }}
                    ... on InvalidAddressError {{ code message address }}
                    ... on QueryFailedError {{ code message }}
                }}
            }}"#,
            addr_str, addr_str
        );

        let response = execute_graphql_query(&ctx.schema, &query).await;

        assert!(response.errors.is_empty(), "Query should succeed without errors");

        let data = response.data.into_json()?;

        // Check hoprBalance is HoprBalance variant
        assert_eq!(
            data["hoprBalance"]["__typename"].as_str(),
            Some("HoprBalance"),
            "Should return HoprBalance variant"
        );
        assert!(
            data["hoprBalance"]["balance"].is_string(),
            "HOPR balance should be returned"
        );
        let hopr_balance_str = data["hoprBalance"]["balance"].as_str().unwrap();
        let _hopr_parsed =
            HoprBalance::from_str(hopr_balance_str).expect("HOPR balance string should be valid HoprBalance format");

        // Check nativeBalance is NativeBalance variant
        assert_eq!(
            data["nativeBalance"]["__typename"].as_str(),
            Some("NativeBalance"),
            "Should return NativeBalance variant"
        );
        assert!(
            data["nativeBalance"]["balance"].is_string(),
            "Native balance should be returned"
        );
        let native_balance_str = data["nativeBalance"]["balance"].as_str().unwrap();
        let _native_parsed = XDaiBalance::from_str(native_balance_str)
            .expect("Native balance string should be valid XDaiBalance format");
    }

    // ========================================
    // Phase 5: Concurrency & Performance Tests
    // ========================================

    // Test Case 8: Concurrent balance queries
    {
        let mut handles = vec![];
        for account in &ctx.test_accounts {
            let schema = ctx.schema.clone();
            let address = account.public().to_address().to_hex();

            handles.push(tokio::spawn(async move { query_hopr_balance(&schema, &address).await }));
        }

        let results: Vec<_> = futures::future::join_all(handles).await;

        for result in results {
            assert!(result.is_ok(), "Concurrent query task should not panic");
            let balance = result.unwrap().expect("Concurrent query should succeed");
            if let Some(balance_str) = balance {
                let _parsed =
                    HoprBalance::from_str(&balance_str).expect("Balance string should be valid HoprBalance format");
            }
        }
    }

    // Test Case 9: Query performance measurement
    {
        let test_addr = ctx.test_accounts[0].public().to_address().to_hex();

        let start = Instant::now();
        let balance = query_hopr_balance(&ctx.schema, &test_addr).await?;
        let duration = start.elapsed();

        if let Some(balance_str) = balance {
            let _parsed =
                HoprBalance::from_str(&balance_str).expect("Balance string should be valid HoprBalance format");
        }

        assert!(
            duration < Duration::from_millis(10),
            "Balance query took too long: {:?}",
            duration
        );

        println!("Balance query completed in: {:?}", duration);
    }

    Ok(())
}
