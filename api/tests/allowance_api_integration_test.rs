//! Integration tests for Safe HOPR allowance API queries (safeHoprAllowance)
//!
//! These tests verify end-to-end functionality by:
//! - Running a real Anvil instance with deployed HOPR contracts
//! - Executing GraphQL queries against the schema
//! - Verifying allowance values are correctly fetched from the blockchain via RPC

use std::{str::FromStr, sync::Arc, time::Duration};

use alloy::{
    node_bindings::AnvilInstance,
    primitives::U256,
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
use hopr_primitive_types::{prelude::HoprBalance, traits::ToHex};
use sea_orm::{Database, DatabaseConnection};

/// Test context containing all components needed for allowance API testing
struct TestContext {
    /// Anvil instance (must be kept alive)
    _anvil: AnvilInstance,
    /// GraphQL schema with all resolvers
    schema: Schema<QueryRoot, MutationRoot, SubscriptionRoot>,
    /// Deployed contract instances for token operations
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
    // We don't need actual database tables for allowance API tests since
    // allowances are queried directly from RPC, not from the database
    let db = Database::connect("sqlite::memory:")
        .await
        .expect("Failed to create test database");

    // Create RPC operations for allowance queries
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

    // Create stub transaction components (not used for allowance queries)
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
        None, // No SQLite notification manager for allowance tests
    );

    Ok(TestContext {
        _anvil: anvil,
        schema,
        contract_instances,
        test_accounts,
        _db: db,
    })
}

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
    let ctx = setup_test_environment().await?;

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
        let channels_addr = ContractAddresses::from(&ctx.contract_instances).channels;

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
        use blokli_chain_types::AlloyAddressExt;
        let channels_alloy_addr = alloy::primitives::Address::from_hopr_address(channels_addr);
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
