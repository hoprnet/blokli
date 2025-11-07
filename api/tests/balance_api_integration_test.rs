//! Integration tests for balance API queries (hoprBalance and nativeBalance)
//!
//! These tests verify end-to-end functionality by:
//! - Running a real Anvil instance with deployed HOPR contracts
//! - Executing GraphQL queries against the schema
//! - Verifying balance values are correctly fetched from the blockchain via RPC

use std::{str::FromStr, sync::Arc, time::Duration};

use tokio::sync::OnceCell;

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
use hopr_primitive_types::{
    prelude::{HoprBalance, XDaiBalance},
    traits::ToHex,
};
use sea_orm::{Database, DatabaseConnection};

/// Static test context shared across all tests (initialized once)
static TEST_CONTEXT: OnceCell<BalanceTestContext> = OnceCell::const_new();

/// Test context containing all components needed for balance API testing
struct BalanceTestContext {
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

/// Initialize test environment with Anvil, contracts, and GraphQL schema
async fn setup_balance_test_environment() -> anyhow::Result<BalanceTestContext> {
    let _ = env_logger::builder().is_test(true).try_init();

    // Start Anvil with 1-second block time for fast testing
    let expected_block_time = Duration::from_secs(1);
    let anvil = create_anvil(Some(expected_block_time));

    // Create test accounts from Anvil's deterministic keys
    let test_accounts = vec![
        ChainKeypair::from_secret(anvil.keys()[0].to_bytes().as_ref())?,
        ChainKeypair::from_secret(anvil.keys()[1].to_bytes().as_ref())?,
        ChainKeypair::from_secret(anvil.keys()[2].to_bytes().as_ref())?,
    ];

    // Deploy HOPR contracts
    let contract_instances = {
        let client = create_rpc_client_to_anvil(&anvil, &test_accounts[0]);
        ContractInstances::deploy_for_testing(client, &test_accounts[0]).await?
    };

    let contract_addrs = ContractAddresses::from(&contract_instances);

    // Wait for contract deployments to be final
    tokio::time::sleep((1 + 2) * expected_block_time).await;

    // Setup in-memory SQLite database for testing
    // We don't need actual database tables for balance API tests since
    // balances are queried directly from RPC, not from the database
    let db = Database::connect("sqlite::memory:").await?;

    // Create RPC operations for balance queries
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

    let rpc_operations = Arc::new(RpcOperations::new(
        rpc_client.clone(),
        ReqwestClient::new(),
        RpcOperationsConfig {
            chain_id,
            contract_addrs,
            expected_block_time,
            ..Default::default()
        },
        None,
    )?);

    // Create stub transaction components (not used for balance queries)
    let transaction_store = Arc::new(TransactionStore::new());
    let transaction_validator = Arc::new(TransactionValidator::new());
    let rpc_adapter = Arc::new(RpcAdapter::new(RpcOperations::new(
        rpc_client,
        ReqwestClient::new(),
        RpcOperationsConfig {
            chain_id,
            contract_addrs,
            expected_block_time,
            ..Default::default()
        },
        None,
    )?));

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
        indexer_state,
        transaction_executor,
        transaction_store,
        rpc_operations,
    );

    Ok(BalanceTestContext {
        _anvil: anvil,
        schema,
        contract_instances,
        test_accounts,
        _db: db,
    })
}

/// Get the shared test context, initializing it once on first call
async fn get_test_context() -> &'static BalanceTestContext {
    TEST_CONTEXT
        .get_or_init(|| async {
            setup_balance_test_environment()
                .await
                .expect("Failed to initialize test context")
        })
        .await
}

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
        r#"query {{ hoprBalance(address: "{}") {{ address balance }} }}"#,
        address
    );

    let response = execute_graphql_query(schema, &query).await;

    if !response.errors.is_empty() {
        anyhow::bail!("GraphQL errors: {:?}", response.errors);
    }

    let data = response.data.into_json()?;
    Ok(data["hoprBalance"]["balance"].as_str().map(|s| s.to_string()))
}

/// Helper to query native balance via GraphQL
async fn query_native_balance(
    schema: &Schema<QueryRoot, MutationRoot, SubscriptionRoot>,
    address: &str,
) -> anyhow::Result<Option<String>> {
    let query = format!(
        r#"query {{ nativeBalance(address: "{}") {{ address balance }} }}"#,
        address
    );

    let response = execute_graphql_query(schema, &query).await;

    if !response.errors.is_empty() {
        anyhow::bail!("GraphQL errors: {:?}", response.errors);
    }

    let data = response.data.into_json()?;
    Ok(data["nativeBalance"]["balance"].as_str().map(|s| s.to_string()))
}

#[test_log::test(tokio::test)]
async fn test_hopr_balance_query_after_mint() -> anyhow::Result<()> {
    let ctx = get_test_context().await;

    blokli_chain_types::utils::mint_tokens(ctx.contract_instances.token.clone(), U256::from(1000_u128))
        .await
        .expect("Minting should return ok");

    // Fund 1000 HOPR tokens to test account
    let test_addr = ctx.test_accounts[0].public().to_address();
    blokli_chain_types::utils::fund_node(
        test_addr,
        U256::ZERO,
        U256::from(1000_u128),
        ctx.contract_instances.token.clone(),
    )
    .await
    .expect("Funding should return ok");

    // Query balance via GraphQL
    let balance = query_hopr_balance(&ctx.schema, &test_addr.to_hex()).await?;

    // Verify balance by parsing into HoprBalance type (idiomatic validation)
    assert!(balance.is_some(), "Balance should be returned");
    let balance_str = balance.unwrap();
    tracing::info!("balance: {balance_str}");

    let parsed_balance =
        HoprBalance::from_str(&balance_str).expect("Balance string should be valid HoprBalance format");
    let expected_balance = HoprBalance::from_str("1000 wxHOPR").unwrap();
    assert_eq!(parsed_balance, expected_balance, "Balance mismatch");

    Ok(())
}

#[tokio::test]
async fn test_hopr_balance_query_zero_balance() -> anyhow::Result<()> {
    let ctx = get_test_context().await;

    // Query balance of account that has never received tokens
    let test_addr = ctx.test_accounts[2].public().to_address();
    let balance = query_hopr_balance(&ctx.schema, &test_addr.to_hex()).await?;

    // Verify zero balance by parsing into HoprBalance type
    assert!(balance.is_some(), "Balance should be returned");
    let balance_str = balance.unwrap();
    let parsed_balance =
        HoprBalance::from_str(&balance_str).expect("Balance string should be valid HoprBalance format");
    let expected_balance = HoprBalance::from_str("0 wxHOPR").unwrap();
    assert_eq!(parsed_balance, expected_balance, "Balance should be zero");

    Ok(())
}

#[test_log::test(tokio::test)]
async fn test_native_balance_query_funded_account() -> anyhow::Result<()> {
    let ctx = get_test_context().await;

    // Fund 1000 xDai to test account
    let test_addr = ctx.test_accounts[0].public().to_address();
    blokli_chain_types::utils::fund_node(
        test_addr,
        U256::from(1000_u128),
        U256::ZERO,
        ctx.contract_instances.token.clone(),
    )
    .await
    .expect("Funding should return ok");

    let balance = query_native_balance(&ctx.schema, &test_addr.to_hex()).await?;

    // Verify balance exists and is non-zero by parsing into XDaiBalance type
    assert!(balance.is_some(), "Balance should be returned");
    let balance_str = balance.unwrap();
    tracing::info!("BALANCE: {balance_str}");
    let parsed_balance =
        XDaiBalance::from_str(&balance_str).expect("Balance string should be valid XDaiBalance format");
    let expected_balance = XDaiBalance::from_str("1000 xDai").unwrap();

    assert_eq!(
        parsed_balance, expected_balance,
        "Balance should be {}, got: {}",
        "1000 xDai", balance_str
    );

    Ok(())
}

#[tokio::test]
async fn test_native_balance_query_zero_balance() -> anyhow::Result<()> {
    let ctx = get_test_context().await;

    // Query balance of an address that doesn't exist in Anvil
    let zero_address = "0x0000000000000000000000000000000000000001";
    let balance = query_native_balance(&ctx.schema, zero_address).await?;

    // Verify zero balance by parsing into XDaiBalance type
    assert!(balance.is_some(), "Balance should be returned");
    let balance_str = balance.unwrap();
    let parsed_balance =
        XDaiBalance::from_str(&balance_str).expect("Balance string should be valid XDaiBalance format");
    let expected_balance = XDaiBalance::from_str("0 xDai").unwrap();
    assert_eq!(parsed_balance, expected_balance, "Balance should be zero");

    Ok(())
}

#[tokio::test]
async fn test_invalid_address_format_returns_error() -> anyhow::Result<()> {
    let ctx = get_test_context().await;

    // Try to query with invalid address format
    let invalid_addresses = vec![
        "not_an_address",
        "0xinvalid",
        "0x123",                                      // Too short
        "0xZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZ", // Invalid hex
    ];

    for invalid_addr in invalid_addresses {
        let query = format!(
            r#"query {{ hoprBalance(address: "{}") {{ address balance }} }}"#,
            invalid_addr
        );

        let response = execute_graphql_query(&ctx.schema, &query).await;

        // Should return error
        assert!(
            !response.errors.is_empty(),
            "Invalid address '{}' should return error",
            invalid_addr
        );
    }

    Ok(())
}

#[tokio::test]
async fn test_address_without_0x_prefix_is_valid() -> anyhow::Result<()> {
    let ctx = get_test_context().await;

    let test_addr = ctx.test_accounts[0].public().to_address();
    // Remove 0x prefix - this should still be valid according to validate_eth_address
    let addr_without_prefix = test_addr.to_hex().trim_start_matches("0x").to_string();

    let balance = query_hopr_balance(&ctx.schema, &addr_without_prefix).await?;

    // Should succeed and return a valid balance (parsing confirms format is correct)
    assert!(
        balance.is_some(),
        "Balance should be returned for address without 0x prefix"
    );
    let balance_str = balance.unwrap();
    let _parsed_balance =
        HoprBalance::from_str(&balance_str).expect("Balance string should be valid HoprBalance format");

    Ok(())
}

#[tokio::test]
async fn test_concurrent_balance_queries() -> anyhow::Result<()> {
    let ctx = get_test_context().await;

    // Execute concurrent balance queries (no minting needed - just testing concurrency)
    let mut handles = vec![];
    for account in &ctx.test_accounts {
        let schema = ctx.schema.clone();
        let address = account.public().to_address().to_hex();

        handles.push(tokio::spawn(async move { query_hopr_balance(&schema, &address).await }));
    }

    // Wait for all queries to complete
    let results: Vec<_> = futures::future::join_all(handles).await;

    // All queries should succeed and return valid balance formats
    for result in results {
        assert!(result.is_ok(), "Concurrent query task should not panic");
        let balance = result.unwrap().expect("Concurrent query should succeed");
        if let Some(balance_str) = balance {
            let _parsed =
                HoprBalance::from_str(&balance_str).expect("Balance string should be valid HoprBalance format");
        }
    }

    Ok(())
}

#[tokio::test]
async fn test_both_balance_types_in_single_query() -> anyhow::Result<()> {
    let ctx = get_test_context().await;

    let test_addr = ctx.test_accounts[0].public().to_address();
    let addr_str = test_addr.to_hex();

    // Query both balance types in a single GraphQL request
    let query = format!(
        r#"query {{
            hoprBalance(address: "{}") {{ address balance }}
            nativeBalance(address: "{}") {{ address balance }}
        }}"#,
        addr_str, addr_str
    );

    let response = execute_graphql_query(&ctx.schema, &query).await;

    // Verify no errors
    assert!(response.errors.is_empty(), "Query should succeed without errors");

    let data = response.data.into_json()?;

    // Verify both balances are returned with valid formats
    assert!(
        data["hoprBalance"]["balance"].is_string(),
        "HOPR balance should be returned"
    );
    let hopr_balance_str = data["hoprBalance"]["balance"].as_str().unwrap();
    let _hopr_parsed =
        HoprBalance::from_str(hopr_balance_str).expect("HOPR balance string should be valid HoprBalance format");

    assert!(
        data["nativeBalance"]["balance"].is_string(),
        "Native balance should be returned"
    );
    let native_balance_str = data["nativeBalance"]["balance"].as_str().unwrap();
    let _native_parsed =
        XDaiBalance::from_str(native_balance_str).expect("Native balance string should be valid XDaiBalance format");

    Ok(())
}

#[tokio::test]
async fn test_balance_query_performance() -> anyhow::Result<()> {
    let ctx = get_test_context().await;

    let test_addr = ctx.test_accounts[0].public().to_address().to_hex();

    // Measure query execution time
    let start = std::time::Instant::now();
    let balance = query_hopr_balance(&ctx.schema, &test_addr).await?;
    let duration = start.elapsed();

    // Validate balance format
    if let Some(balance_str) = balance {
        let _parsed = HoprBalance::from_str(&balance_str).expect("Balance string should be valid HoprBalance format");
    }

    // Balance query should complete reasonably quickly (< 10ms for Anvil RPC)
    assert!(
        duration < Duration::from_millis(10),
        "Balance query took too long: {:?}",
        duration
    );

    println!("Balance query completed in: {:?}", duration);

    Ok(())
}
