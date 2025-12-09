//! Comprehensive integration tests for GraphQL transaction mutations
//!
//! These tests verify:
//! - sendTransaction (fire-and-forget mode)
//! - sendTransactionAsync (async mode with tracking)
//! - sendTransactionSync (sync mode with confirmations)
//! - Union type handling (Transaction vs error types)
//! - Input validation and error scenarios
//! - Transaction tracking and status updates

use std::{sync::Arc, time::Duration};

use alloy::{
    consensus::{SignableTransaction, TxLegacy},
    eips::eip2718::Encodable2718,
    node_bindings::AnvilInstance,
    primitives::{Address as AlloyAddress, TxKind, U256},
    signers::{SignerSync, local::PrivateKeySigner},
};
use anyhow::Result;
use async_graphql::{EmptySubscription, Schema};
use blokli_api::{mutation::MutationRoot, query::QueryRoot};
use blokli_chain_api::{
    DefaultHttpRequestor,
    rpc_adapter::RpcAdapter,
    transaction_executor::{RawTransactionExecutor, RawTransactionExecutorConfig},
    transaction_monitor::{TransactionMonitor, TransactionMonitorConfig},
    transaction_store::{TransactionStatus, TransactionStore},
    transaction_validator::TransactionValidator,
};
use blokli_chain_rpc::{
    client::DefaultRetryPolicy,
    rpc::{RpcOperations, RpcOperationsConfig},
    transport::ReqwestClient,
};
use blokli_chain_types::{ContractAddresses, utils::create_anvil};
use blokli_db::{BlokliDbGeneralModelOperations, TargetDb, db::BlokliDb};
use hopr_crypto_types::keypairs::{ChainKeypair, Keypair};
use tokio::task::AbortHandle;

/// Test context containing all components needed for GraphQL mutation tests
struct TestContext {
    anvil: AnvilInstance,
    chain_key: ChainKeypair,
    executor: Arc<RawTransactionExecutor<RpcAdapter<DefaultHttpRequestor>>>,
    store: Arc<TransactionStore>,
    monitor: Arc<TransactionMonitor<RpcAdapter<DefaultHttpRequestor>>>,
    _rpc_adapter: Arc<RpcAdapter<DefaultHttpRequestor>>,
    monitor_handle: Option<AbortHandle>,
    schema: Schema<QueryRoot, MutationRoot, EmptySubscription>,
}

impl Drop for TestContext {
    fn drop(&mut self) {
        // Stop the monitor if it's running
        if let Some(handle) = self.monitor_handle.take() {
            handle.abort();
        }
    }
}

/// Set up a complete test environment with Anvil and GraphQL schema
async fn setup_test_environment(
    block_time: Duration,
    confirmations: u32,
    executor_config: RawTransactionExecutorConfig,
) -> Result<TestContext> {
    // Initialize logging for tests
    let _ = env_logger::builder().is_test(true).try_init();

    // Start Anvil with specified block time
    let anvil = create_anvil(Some(block_time));
    let chain_key = ChainKeypair::from_secret(anvil.keys()[0].to_bytes().as_ref())?;

    // Create RPC configuration
    let cfg = RpcOperationsConfig {
        chain_id: anvil.chain_id(),
        tx_polling_interval: Duration::from_secs(1),
        expected_block_time: block_time,
        finality: confirmations,
        gas_oracle_url: None,
        contract_addrs: ContractAddresses::default(),
        ..Default::default()
    };

    // Set up RPC client with retry policy
    let transport_client = alloy::transports::http::ReqwestTransport::new(anvil.endpoint_url());
    let rpc_client = alloy::rpc::client::ClientBuilder::default()
        .layer(alloy::transports::layers::RetryBackoffLayer::new_with_policy(
            2,
            100,
            100,
            DefaultRetryPolicy::default(),
        ))
        .transport(transport_client.clone(), transport_client.guess_local());

    let rpc_operations = RpcOperations::new(rpc_client, ReqwestClient::new(), cfg, None)?;
    let rpc_adapter = Arc::new(RpcAdapter::new(rpc_operations));

    // Create transaction components
    let transaction_store = Arc::new(TransactionStore::new());
    let transaction_validator = Arc::new(TransactionValidator::new());

    let transaction_executor = Arc::new(RawTransactionExecutor::with_shared_dependencies(
        rpc_adapter.clone(),
        transaction_store.clone(),
        transaction_validator.clone(),
        executor_config,
    ));

    // Create and start transaction monitor
    let monitor_config = TransactionMonitorConfig {
        poll_interval: Duration::from_secs(1),
        timeout: Duration::from_secs(30),
        per_transaction_delay: Duration::from_millis(10),
    };

    let transaction_monitor = Arc::new(TransactionMonitor::new(
        transaction_store.clone(),
        (*rpc_adapter).clone(),
        monitor_config,
    ));

    let monitor_clone = transaction_monitor.clone();
    let monitor_handle = tokio::spawn(async move {
        monitor_clone.start().await;
    })
    .abort_handle();

    // Create in-memory database
    let db = BlokliDb::new_in_memory().await?;

    // Build GraphQL schema
    let schema = Schema::build(QueryRoot, MutationRoot, EmptySubscription)
        .data(db.conn(TargetDb::Index).clone())
        .data(anvil.chain_id())
        .data("test".to_string())
        .data(ContractAddresses::default())
        .data(transaction_executor.clone())
        .data(transaction_store.clone())
        .finish();

    Ok(TestContext {
        anvil,
        chain_key,
        executor: transaction_executor,
        store: transaction_store,
        monitor: transaction_monitor,
        _rpc_adapter: rpc_adapter,
        monitor_handle: Some(monitor_handle),
        schema,
    })
}

/// Helper to create a simple transaction for testing
fn create_test_transaction(from: &ChainKeypair, to: AlloyAddress, value: u64, nonce: u64, chain_id: u64) -> Vec<u8> {
    let signer = PrivateKeySigner::from_slice(from.secret().as_ref()).expect("valid signer");

    let tx = TxLegacy {
        chain_id: Some(chain_id),
        nonce,
        gas_price: 1_000_000_000, // 1 gwei
        gas_limit: 21_000,
        to: TxKind::Call(to),
        value: U256::from(value),
        input: Default::default(),
    };

    // Sign the transaction hash (blocking operation for tests)
    let tx_hash = tx.signature_hash();
    let sig = signer.sign_hash_sync(&tx_hash).expect("signing failed");
    let signed_tx = tx.into_signed(sig);

    let mut encoded = Vec::new();
    signed_tx.encode_2718(&mut encoded);
    encoded
}

/// Helper to execute GraphQL mutation and return result
async fn execute_mutation(
    schema: &Schema<QueryRoot, MutationRoot, EmptySubscription>,
    query: &str,
) -> serde_json::Value {
    let response = schema.execute(query).await;
    serde_json::to_value(response).expect("Failed to serialize response")
}

#[tokio::test]
async fn test_send_transaction_returns_hash_immediately() -> Result<()> {
    let ctx = setup_test_environment(Duration::from_secs(1), 2, RawTransactionExecutorConfig::default()).await?;

    // Create a test transaction
    let raw_tx = create_test_transaction(&ctx.chain_key, AlloyAddress::ZERO, 1_000_000, 0, ctx.anvil.chain_id());
    let raw_tx_hex = format!("0x{}", hex::encode(&raw_tx));

    let query = format!(
        r#"mutation {{
            sendTransaction(input: {{ rawTransaction: "{}" }}) {{
                __typename
                ... on SendTransactionSuccess {{
                    transactionHash
                }}
                ... on RpcError {{
                    code
                    message
                }}
            }}
        }}"#,
        raw_tx_hex
    );

    let result = execute_mutation(&ctx.schema, &query).await;
    let data = &result["data"]["sendTransaction"];

    // Verify it returns success with transaction hash
    assert_eq!(data["__typename"], "SendTransactionSuccess");
    assert!(data["transactionHash"].is_string());
    let tx_hash = data["transactionHash"].as_str().unwrap();
    assert_eq!(tx_hash.len(), 66); // 0x + 64 hex chars
    assert!(tx_hash.starts_with("0x"));

    // Verify transaction was NOT stored (fire-and-forget mode)
    assert_eq!(ctx.store.count(), 0);

    Ok(())
}

#[tokio::test]
async fn test_send_transaction_with_invalid_hex() -> Result<()> {
    let ctx = setup_test_environment(Duration::from_secs(1), 2, RawTransactionExecutorConfig::default()).await?;

    let query = r#"mutation {
        sendTransaction(input: { rawTransaction: "0xZZZZ" }) {
            __typename
            ... on SendTransactionSuccess {
                transactionHash
            }
            ... on RpcError {
                code
                message
            }
        }
    }"#;

    let result = execute_mutation(&ctx.schema, query).await;

    // Should return error in GraphQL errors array (not union type)
    assert!(result["errors"].is_array());
    let error_message = result["errors"][0]["message"].as_str().unwrap();
    assert!(error_message.to_lowercase().contains("hex") || error_message.to_lowercase().contains("invalid"));

    Ok(())
}

#[tokio::test]
async fn test_send_transaction_async_returns_uuid_and_tracks() -> Result<()> {
    let ctx = setup_test_environment(Duration::from_secs(1), 2, RawTransactionExecutorConfig::default()).await?;

    // Create a test transaction
    let raw_tx = create_test_transaction(&ctx.chain_key, AlloyAddress::ZERO, 1_000_000, 0, ctx.anvil.chain_id());
    let raw_tx_hex = format!("0x{}", hex::encode(&raw_tx));

    let query = format!(
        r#"mutation {{
            sendTransactionAsync(input: {{ rawTransaction: "{}" }}) {{
                __typename
                ... on Transaction {{
                    id
                    status
                    submittedAt
                    transactionHash
                }}
                ... on RpcError {{
                    code
                    message
                }}
            }}
        }}"#,
        raw_tx_hex
    );

    let result = execute_mutation(&ctx.schema, &query).await;
    let data = &result["data"]["sendTransactionAsync"];

    // Verify it returns Transaction with UUID
    assert_eq!(data["__typename"], "Transaction");
    let tx_id = data["id"].as_str().unwrap();

    // Verify UUID format (contains hyphens)
    assert!(tx_id.contains('-'));
    assert!(uuid::Uuid::parse_str(tx_id).is_ok());

    // Verify status is SUBMITTED
    assert_eq!(data["status"], "SUBMITTED");

    // Verify timestamp is present
    assert!(data["submittedAt"].is_string());

    // Verify transaction hash is present
    assert!(data["transactionHash"].is_string());

    // Verify transaction was stored
    assert_eq!(ctx.store.count(), 1);

    // Wait for confirmation
    tokio::time::sleep(Duration::from_secs(1)).await;

    // Verify transaction status updated to CONFIRMED
    let uuid_id = uuid::Uuid::parse_str(tx_id)?;
    let record = ctx.store.get(uuid_id)?;
    assert_eq!(record.status, TransactionStatus::Confirmed);

    Ok(())
}

#[tokio::test]
async fn test_send_transaction_sync_waits_for_confirmations() -> Result<()> {
    let ctx = setup_test_environment(Duration::from_secs(1), 2, RawTransactionExecutorConfig::default()).await?;

    // Create a test transaction
    let raw_tx = create_test_transaction(&ctx.chain_key, AlloyAddress::ZERO, 1_000_000, 0, ctx.anvil.chain_id());
    let raw_tx_hex = format!("0x{}", hex::encode(&raw_tx));

    let query = format!(
        r#"mutation {{
            sendTransactionSync(input: {{ rawTransaction: "{}" }}) {{
                __typename
                ... on Transaction {{
                    id
                    status
                    submittedAt
                    transactionHash
                }}
                ... on RpcError {{
                    code
                    message
                }}
                ... on TimeoutError {{
                    code
                    message
                }}
            }}
        }}"#,
        raw_tx_hex
    );

    let start = std::time::Instant::now();
    let result = execute_mutation(&ctx.schema, &query).await;
    let elapsed = start.elapsed();

    let data = &result["data"]["sendTransactionSync"];

    // Verify it returns Transaction
    assert_eq!(data["__typename"], "Transaction");

    // Verify status is CONFIRMED (waited for confirmations)
    assert_eq!(data["status"], "CONFIRMED");

    // Verify it waited at least long enough for confirmations (2 confirmations * 100ms block time)
    assert!(elapsed >= Duration::from_millis(150));

    // Verify transaction was stored
    assert_eq!(ctx.store.count(), 1);

    Ok(())
}

#[tokio::test]
async fn test_send_transaction_sync_with_custom_confirmations() -> Result<()> {
    let ctx = setup_test_environment(Duration::from_secs(1), 2, RawTransactionExecutorConfig::default()).await?;

    // Create a test transaction
    let raw_tx = create_test_transaction(&ctx.chain_key, AlloyAddress::ZERO, 1_000_000, 0, ctx.anvil.chain_id());
    let raw_tx_hex = format!("0x{}", hex::encode(&raw_tx));

    let query = format!(
        r#"mutation {{
            sendTransactionSync(input: {{ rawTransaction: "{}" }}, confirmations: 1) {{
                __typename
                ... on Transaction {{
                    id
                    status
                }}
            }}
        }}"#,
        raw_tx_hex
    );

    let start = std::time::Instant::now();
    let result = execute_mutation(&ctx.schema, &query).await;
    let elapsed = start.elapsed();

    let data = &result["data"]["sendTransactionSync"];

    // Verify status is CONFIRMED
    assert_eq!(data["status"], "CONFIRMED");

    // Verify it waited for 1 confirmation (with 1-second block time, should be ~1-2 seconds)
    assert!(
        elapsed >= Duration::from_secs(1),
        "Should wait at least 1 second for confirmation"
    );
    assert!(
        elapsed < Duration::from_secs(3),
        "Should complete within 3 seconds with 1 confirmation"
    );

    Ok(())
}

#[tokio::test]
async fn test_send_transaction_async_with_empty_input() -> Result<()> {
    let ctx = setup_test_environment(Duration::from_secs(1), 2, RawTransactionExecutorConfig::default()).await?;

    let query = r#"mutation {
        sendTransactionAsync(input: { rawTransaction: "" }) {
            __typename
            ... on Transaction {
                id
            }
            ... on RpcError {
                code
                message
            }
        }
    }"#;

    let result = execute_mutation(&ctx.schema, query).await;

    // Should return error (empty transaction is invalid)
    assert!(result["errors"].is_array() || result["data"]["sendTransactionAsync"]["__typename"] == "RpcError");

    Ok(())
}

#[tokio::test]
async fn test_mutations_accept_hex_with_and_without_prefix() -> Result<()> {
    let ctx = setup_test_environment(Duration::from_secs(1), 2, RawTransactionExecutorConfig::default()).await?;

    // Create a test transaction
    let raw_tx = create_test_transaction(&ctx.chain_key, AlloyAddress::ZERO, 1_000_000, 0, ctx.anvil.chain_id());

    // Test with 0x prefix
    let with_prefix = format!("0x{}", hex::encode(&raw_tx));
    let query1 = format!(
        r#"mutation {{
            sendTransaction(input: {{ rawTransaction: "{}" }}) {{
                __typename
                ... on SendTransactionSuccess {{
                    transactionHash
                }}
            }}
        }}"#,
        with_prefix
    );

    let result1 = execute_mutation(&ctx.schema, &query1).await;
    assert_eq!(
        result1["data"]["sendTransaction"]["__typename"],
        "SendTransactionSuccess"
    );

    // Test without 0x prefix
    let without_prefix = hex::encode(&raw_tx);

    // Create new transaction with different nonce to avoid duplicate
    let raw_tx2 = create_test_transaction(&ctx.chain_key, AlloyAddress::ZERO, 1_000_000, 1, ctx.anvil.chain_id());
    let without_prefix2 = hex::encode(&raw_tx2);

    let query2 = format!(
        r#"mutation {{
            sendTransaction(input: {{ rawTransaction: "{}" }}) {{
                __typename
                ... on SendTransactionSuccess {{
                    transactionHash
                }}
            }}
        }}"#,
        without_prefix2
    );

    let result2 = execute_mutation(&ctx.schema, &query2).await;
    assert_eq!(
        result2["data"]["sendTransaction"]["__typename"],
        "SendTransactionSuccess"
    );

    Ok(())
}

#[tokio::test]
async fn test_send_transaction_sync_timeout() -> Result<()> {
    // Set very short timeout to trigger timeout error
    let config = RawTransactionExecutorConfig {
        confirmation_timeout: Duration::from_millis(50),
        ..Default::default()
    };

    let ctx = setup_test_environment(
        Duration::from_secs(2), // Slow block time to trigger timeout
        5,                      // High confirmations
        config,
    )
    .await?;

    // Create a test transaction
    let raw_tx = create_test_transaction(&ctx.chain_key, AlloyAddress::ZERO, 1_000_000, 0, ctx.anvil.chain_id());
    let raw_tx_hex = format!("0x{}", hex::encode(&raw_tx));

    let query = format!(
        r#"mutation {{
            sendTransactionSync(input: {{ rawTransaction: "{}" }}) {{
                __typename
                ... on Transaction {{
                    status
                }}
                ... on TimeoutError {{
                    code
                    message
                }}
            }}
        }}"#,
        raw_tx_hex
    );

    let result = execute_mutation(&ctx.schema, &query).await;
    let data = &result["data"]["sendTransactionSync"];

    // Should return error (either TimeoutError or RpcError with timeout)
    let typename = data["__typename"].as_str().unwrap();
    assert!(
        typename == "TimeoutError" || typename == "RpcError",
        "Expected TimeoutError or RpcError, got {}",
        typename
    );

    // Check that error message indicates timeout
    if let Some(message_val) = data.get("message") {
        if let Some(message) = message_val.as_str() {
            let msg_lower = message.to_lowercase();
            assert!(
                msg_lower.contains("timeout") || msg_lower.contains("timed out"),
                "Error message should mention timeout: {}",
                message
            );
        }
    }

    Ok(())
}
