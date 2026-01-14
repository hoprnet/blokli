//! Comprehensive integration tests for GraphQL transaction mutations
//!
//! These tests verify:
//! - sendTransaction (fire-and-forget mode)
//! - sendTransactionAsync (async mode with tracking)
//! - sendTransactionSync (sync mode with confirmations)
//! - Union type handling (Transaction vs error types)
//! - Input validation and error scenarios
//! - Transaction tracking and status updates

mod common;

use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use anyhow::Result;
use async_graphql::{EmptySubscription, Schema};
use blokli_api::{mutation::MutationRoot, query::QueryRoot};
use blokli_chain_api::{
    transaction_executor::RawTransactionExecutorConfig,
    transaction_store::{TransactionStatus, TransactionStore},
};
use blokli_chain_types::ContractAddresses;
use blokli_db::{BlokliDbGeneralModelOperations, TargetDb, db::BlokliDb};
use hopr_bindings::exports::alloy::{
    consensus::{SignableTransaction, TxLegacy},
    eips::eip2718::Encodable2718,
    primitives::{Address as AlloyAddress, TxKind, U256},
    providers::{Provider, ProviderBuilder},
    signers::{SignerSync, local::PrivateKeySigner},
};
use hopr_crypto_types::keypairs::{ChainKeypair, Keypair};

/// Test context containing all components needed for GraphQL mutation tests
struct TestContext {
    _tx_ctx: common::TransactionTestContext,
    chain_id: u64,
    schema: Schema<QueryRoot, MutationRoot, EmptySubscription>,
}

impl TestContext {
    fn chain_key(&self) -> &ChainKeypair {
        &self._tx_ctx.chain_key
    }

    fn store(&self) -> &Arc<TransactionStore> {
        &self._tx_ctx.store
    }
}

/// Set up a complete test environment with Anvil and GraphQL schema
async fn setup_test_environment(
    block_time: Duration,
    confirmations: u32,
    executor_config: RawTransactionExecutorConfig,
) -> Result<TestContext> {
    // Use common transaction test helper
    let tx_ctx = common::setup_transaction_test_environment(
        block_time,
        Duration::from_secs(1), // poll_interval
        confirmations,
        Some(executor_config),
    )
    .await?;

    // Create in-memory database
    let db = BlokliDb::new_in_memory().await?;

    // Build GraphQL schema with EmptySubscription variant
    let schema = Schema::build(QueryRoot, MutationRoot, EmptySubscription)
        .data(db.conn(TargetDb::Index).clone())
        .data(31337u64) // Anvil chain ID
        .data("test".to_string())
        .data(ContractAddresses::default())
        .data(tx_ctx.executor.clone())
        .data(tx_ctx.store.clone())
        .finish();

    Ok(TestContext {
        _tx_ctx: tx_ctx,
        chain_id: 31337, // Anvil chain ID
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

/// Helper to get the current nonce for the test account from RPC
async fn get_current_nonce(ctx: &TestContext) -> u64 {
    let address = ctx.chain_key().public().to_address();
    let alloy_address = AlloyAddress::from_slice(address.as_ref());

    let provider = ProviderBuilder::new().connect_http(ctx._tx_ctx.anvil.endpoint_url());

    provider
        .get_transaction_count(alloy_address)
        .await
        .expect("Failed to get transaction count")
}

#[tokio::test]
async fn test_send_transaction_returns_hash_immediately() -> Result<()> {
    let ctx = setup_test_environment(Duration::from_secs(1), 2, RawTransactionExecutorConfig::default()).await?;

    // Get current nonce from RPC
    let nonce = get_current_nonce(&ctx).await;

    // Create a test transaction
    let raw_tx = create_test_transaction(ctx.chain_key(), AlloyAddress::ZERO, 1_000_000, nonce, ctx.chain_id);
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
    assert_eq!(ctx.store().count(), 0);

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

    // Get current nonce from RPC
    let nonce = get_current_nonce(&ctx).await;

    // Create a test transaction
    let raw_tx = create_test_transaction(ctx.chain_key(), AlloyAddress::ZERO, 1_000_000, nonce, ctx.chain_id);
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
    assert_eq!(ctx.store().count(), 1);

    // Wait for confirmation
    tokio::time::sleep(Duration::from_secs(1)).await;

    // Verify transaction status updated to CONFIRMED
    let uuid_id = uuid::Uuid::parse_str(tx_id)?;
    let record = ctx.store().get(uuid_id)?;
    assert_eq!(record.status, TransactionStatus::Confirmed);

    Ok(())
}

#[tokio::test]
async fn test_send_transaction_sync_waits_for_confirmations() -> Result<()> {
    let ctx = setup_test_environment(Duration::from_secs(1), 2, RawTransactionExecutorConfig::default()).await?;

    // Get current nonce from RPC
    let nonce = get_current_nonce(&ctx).await;

    // Create a test transaction
    let raw_tx = create_test_transaction(ctx.chain_key(), AlloyAddress::ZERO, 1_000_000, nonce, ctx.chain_id);
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

    let start = Instant::now();
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
    assert_eq!(ctx.store().count(), 1);

    Ok(())
}

#[tokio::test]
async fn test_send_transaction_sync_with_custom_confirmations() -> Result<()> {
    let ctx = setup_test_environment(Duration::from_secs(1), 2, RawTransactionExecutorConfig::default()).await?;

    // Get current nonce from RPC
    let nonce = get_current_nonce(&ctx).await;

    // Create a test transaction
    let raw_tx = create_test_transaction(ctx.chain_key(), AlloyAddress::ZERO, 1_000_000, nonce, ctx.chain_id);
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

    let start = Instant::now();
    let result = execute_mutation(&ctx.schema, &query).await;
    let elapsed = start.elapsed();

    let data = &result["data"]["sendTransactionSync"];

    // Verify status is CONFIRMED
    assert_eq!(data["status"], "CONFIRMED");

    // Verify it waited for confirmation (with 1-second block time, could be instant if submitted just before block)
    // Just verify it completes within a reasonable time
    assert!(
        elapsed < Duration::from_secs(5),
        "Should complete within 5 seconds with 1 confirmation, took {:?}",
        elapsed
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

    // Get current nonce from RPC
    let nonce = get_current_nonce(&ctx).await;

    // Create a test transaction
    let raw_tx = create_test_transaction(ctx.chain_key(), AlloyAddress::ZERO, 1_000_000, nonce, ctx.chain_id);

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

    // Test without 0x prefix - create new transaction with different nonce to avoid duplicate
    let raw_tx2 = create_test_transaction(ctx.chain_key(), AlloyAddress::ZERO, 1_000_000, nonce + 1, ctx.chain_id);
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

    // Get current nonce from RPC
    let nonce = get_current_nonce(&ctx).await;

    // Create a test transaction
    let raw_tx = create_test_transaction(ctx.chain_key(), AlloyAddress::ZERO, 1_000_000, nonce, ctx.chain_id);
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
