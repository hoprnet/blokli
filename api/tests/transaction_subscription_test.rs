//! Comprehensive integration tests for transactionUpdated subscription
//!
//! These tests verify:
//! - Subscription to transaction status updates
//! - Initial state emission
//! - Status transition updates (SUBMITTED → CONFIRMED)
//! - Invalid UUID handling
//! - Multiple concurrent subscriptions
//! - Subscription lifecycle

mod common;

use std::{sync::Arc, time::Duration};

use anyhow::Result;
use async_graphql::{EmptyMutation, Schema};
use blokli_api::{query::QueryRoot, subscription::SubscriptionRoot};
use blokli_chain_api::transaction_store::{TransactionRecord, TransactionStatus, TransactionStore};
use blokli_chain_types::ContractAddresses;
use blokli_db::{BlokliDbGeneralModelOperations, TargetDb, db::BlokliDb};
use futures::StreamExt;
use hopr_bindings::exports::alloy::{
    consensus::{SignableTransaction, TxLegacy},
    eips::eip2718::Encodable2718,
    primitives::{Address as AlloyAddress, TxKind, U256},
    signers::{SignerSync, local::PrivateKeySigner},
};
use hopr_crypto_types::{
    keypairs::{ChainKeypair, Keypair},
    types::Hash,
};
use tokio::task::AbortHandle;

/// Test context for subscription tests
struct TestContext {
    chain_key: ChainKeypair,
    store: Arc<TransactionStore>,
    schema: Schema<QueryRoot, EmptyMutation, SubscriptionRoot>,
    _monitor_handle: Option<AbortHandle>,
}

impl Drop for TestContext {
    fn drop(&mut self) {
        // Stop the monitor if it's running
        if let Some(handle) = self._monitor_handle.take() {
            handle.abort();
        }
    }
}

/// Set up test environment with subscription support
async fn setup_test_environment() -> Result<TestContext> {
    // Use common transaction test helper with faster polling for subscriptions
    let tx_ctx = common::setup_transaction_test_environment(
        Duration::from_secs(1),    // block_time
        Duration::from_millis(50), // poll_interval (faster for subscription tests)
        2,                         // finality
        None,                      // executor_config (use default)
    )
    .await?;

    // Create in-memory database
    let db = BlokliDb::new_in_memory().await?;

    // Build GraphQL schema with SubscriptionRoot (EmptyMutation variant)
    let schema = Schema::build(QueryRoot, EmptyMutation, SubscriptionRoot)
        .data(db.conn(TargetDb::Index).clone())
        .data(31337u64) // Anvil chain ID
        .data("test".to_string())
        .data(ContractAddresses::default())
        .data(tx_ctx.executor.clone())
        .data(tx_ctx.store.clone())
        .finish();

    Ok(TestContext {
        chain_key: tx_ctx.chain_key.clone(),
        store: tx_ctx.store.clone(),
        schema,
        _monitor_handle: tx_ctx.monitor_handle.clone(),
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

#[tokio::test]
async fn test_transaction_updated_emits_initial_state() -> Result<()> {
    let ctx = setup_test_environment().await?;

    // Insert a test transaction in SUBMITTED state
    let tx_id = uuid::Uuid::new_v4();
    let record = TransactionRecord {
        id: tx_id,
        raw_transaction: vec![0x01, 0x02, 0x03],
        transaction_hash: Hash::default(),
        status: TransactionStatus::Submitted,
        submitted_at: chrono::Utc::now(),
        confirmed_at: None,
        error_message: None,
        safe_execution: None,
    };
    ctx.store.insert(record)?;

    let query = format!(
        r#"subscription {{
            transactionUpdated(id: "{}") {{
                id
                status
                submittedAt
                transactionHash
            }}
        }}"#,
        tx_id
    );

    let mut stream = ctx.schema.execute_stream(&query).boxed();

    // Wait for initial emission
    let response = tokio::time::timeout(Duration::from_secs(1), stream.next())
        .await
        .expect("Timeout waiting for initial state")
        .expect("Stream should produce a value");

    let data = serde_json::to_value(response).expect("Failed to serialize response");

    // Verify initial state
    assert_eq!(data["data"]["transactionUpdated"]["id"], tx_id.to_string());
    assert_eq!(data["data"]["transactionUpdated"]["status"], "SUBMITTED");
    assert!(data["data"]["transactionUpdated"]["submittedAt"].is_string());
    assert!(data["data"]["transactionUpdated"]["transactionHash"].is_string());

    Ok(())
}

#[tokio::test]
async fn test_transaction_updated_receives_status_changes() -> Result<()> {
    let ctx = setup_test_environment().await?;

    // Insert a test transaction in SUBMITTED state
    let tx_id = uuid::Uuid::new_v4();
    let record = TransactionRecord {
        id: tx_id,
        raw_transaction: create_test_transaction(&ctx.chain_key, AlloyAddress::ZERO, 1_000_000, 0, 31337),
        transaction_hash: Hash::default(),
        status: TransactionStatus::Submitted,
        submitted_at: chrono::Utc::now(),
        confirmed_at: None,
        error_message: None,
        safe_execution: None,
    };
    ctx.store.insert(record)?;

    let query = format!(
        r#"subscription {{
            transactionUpdated(id: "{}") {{
                id
                status
            }}
        }}"#,
        tx_id
    );

    let mut stream = ctx.schema.execute_stream(&query).boxed();

    // Receive initial SUBMITTED state
    let response1 = tokio::time::timeout(Duration::from_secs(1), stream.next())
        .await
        .expect("Timeout waiting for initial state")
        .expect("Stream should produce a value");
    let data1 = serde_json::to_value(response1).expect("Failed to serialize response");
    assert_eq!(data1["data"]["transactionUpdated"]["status"], "SUBMITTED");

    // Update status to CONFIRMED
    ctx.store.update_status(tx_id, TransactionStatus::Confirmed, None)?;

    // Receive status update
    let response2 = tokio::time::timeout(Duration::from_secs(1), stream.next())
        .await
        .expect("Timeout waiting for status update")
        .expect("Stream should produce a value");
    let data2 = serde_json::to_value(response2).expect("Failed to serialize response");
    assert_eq!(data2["data"]["transactionUpdated"]["status"], "CONFIRMED");

    Ok(())
}

#[tokio::test]
async fn test_transaction_updated_with_invalid_uuid() -> Result<()> {
    let ctx = setup_test_environment().await?;

    let query = r#"subscription {
        transactionUpdated(id: "not-a-valid-uuid") {
            id
            status
        }
    }"#;

    let mut stream = ctx.schema.execute_stream(query).boxed();

    // Should receive an error immediately
    let response = stream.next().await.expect("Should return error");

    // Check for error in response
    assert!(response.errors.len() > 0, "Expected errors in response");
    let error_message = &response.errors[0].message;
    assert!(error_message.to_lowercase().contains("invalid") || error_message.to_lowercase().contains("uuid"));

    Ok(())
}

#[tokio::test]
async fn test_transaction_updated_with_nonexistent_transaction() -> Result<()> {
    let ctx = setup_test_environment().await?;

    // Query with valid UUID that doesn't exist in store
    let non_existent_id = uuid::Uuid::new_v4();

    let query = format!(
        r#"subscription {{
            transactionUpdated(id: "{}") {{
                id
                status
            }}
        }}"#,
        non_existent_id
    );

    let mut stream = ctx.schema.execute_stream(&query).boxed();

    // Subscription should start, but not emit anything yet
    // (transaction might be added later)
    let timeout_result = tokio::time::timeout(Duration::from_millis(300), stream.next()).await;

    // Should timeout (no transaction to emit)
    assert!(
        timeout_result.is_err(),
        "Should timeout waiting for nonexistent transaction"
    );

    Ok(())
}

#[tokio::test]
async fn test_transaction_updated_multiple_status_transitions() -> Result<()> {
    let ctx = setup_test_environment().await?;

    // Insert a test transaction in PENDING state
    let tx_id = uuid::Uuid::new_v4();
    let record = TransactionRecord {
        id: tx_id,
        raw_transaction: create_test_transaction(&ctx.chain_key, AlloyAddress::ZERO, 1_000_000, 0, 31337),
        transaction_hash: Hash::default(),
        status: TransactionStatus::Pending,
        submitted_at: chrono::Utc::now(),
        confirmed_at: None,
        error_message: None,
        safe_execution: None,
    };
    ctx.store.insert(record)?;

    let query = format!(
        r#"subscription {{
            transactionUpdated(id: "{}") {{
                status
            }}
        }}"#,
        tx_id
    );

    let mut stream = ctx.schema.execute_stream(&query).boxed();

    // 1. Receive initial PENDING state
    let response1 = tokio::time::timeout(Duration::from_secs(1), stream.next())
        .await
        .expect("Timeout waiting for response")
        .expect("Stream should produce a value");
    let data1 = serde_json::to_value(response1).expect("Failed to serialize response");
    assert_eq!(data1["data"]["transactionUpdated"]["status"], "PENDING");

    // 2. Update to SUBMITTED
    ctx.store.update_status(tx_id, TransactionStatus::Submitted, None)?;
    let response2 = tokio::time::timeout(Duration::from_secs(1), stream.next())
        .await
        .expect("Timeout waiting for response")
        .expect("Stream should produce a value");
    let data2 = serde_json::to_value(response2).expect("Failed to serialize response");
    assert_eq!(data2["data"]["transactionUpdated"]["status"], "SUBMITTED");

    // 3. Update to CONFIRMED
    ctx.store.update_status(tx_id, TransactionStatus::Confirmed, None)?;
    let response3 = tokio::time::timeout(Duration::from_secs(1), stream.next())
        .await
        .expect("Timeout waiting for response")
        .expect("Stream should produce a value");
    let data3 = serde_json::to_value(response3).expect("Failed to serialize response");
    assert_eq!(data3["data"]["transactionUpdated"]["status"], "CONFIRMED");

    Ok(())
}

#[tokio::test]
async fn test_transaction_updated_concurrent_subscriptions() -> Result<()> {
    let ctx = setup_test_environment().await?;

    // Insert a test transaction
    let tx_id = uuid::Uuid::new_v4();
    let record = TransactionRecord {
        id: tx_id,
        raw_transaction: vec![0x01, 0x02, 0x03],
        transaction_hash: Hash::default(),
        status: TransactionStatus::Submitted,
        submitted_at: chrono::Utc::now(),
        confirmed_at: None,
        error_message: None,
        safe_execution: None,
    };
    ctx.store.insert(record)?;

    let query = format!(
        r#"subscription {{
            transactionUpdated(id: "{}") {{
                status
            }}
        }}"#,
        tx_id
    );

    // Create two concurrent subscriptions
    let mut stream1 = ctx.schema.execute_stream(&query).boxed();
    let mut stream2 = ctx.schema.execute_stream(&query).boxed();

    // Both should receive initial state
    let response1_1 = tokio::time::timeout(Duration::from_secs(1), stream1.next())
        .await
        .expect("Timeout waiting for response")
        .expect("Stream should produce a value");
    let data1_1 = serde_json::to_value(response1_1).expect("Failed to serialize response");
    assert_eq!(data1_1["data"]["transactionUpdated"]["status"], "SUBMITTED");

    let response2_1 = tokio::time::timeout(Duration::from_secs(1), stream2.next())
        .await
        .expect("Timeout waiting for response")
        .expect("Stream should produce a value");
    let data2_1 = serde_json::to_value(response2_1).expect("Failed to serialize response");
    assert_eq!(data2_1["data"]["transactionUpdated"]["status"], "SUBMITTED");

    // Update status
    ctx.store.update_status(tx_id, TransactionStatus::Confirmed, None)?;

    // Both should receive the update
    let response1_2 = tokio::time::timeout(Duration::from_secs(1), stream1.next())
        .await
        .expect("Timeout waiting for response")
        .expect("Stream should produce a value");
    let data1_2 = serde_json::to_value(response1_2).expect("Failed to serialize response");
    assert_eq!(data1_2["data"]["transactionUpdated"]["status"], "CONFIRMED");

    let response2_2 = tokio::time::timeout(Duration::from_secs(1), stream2.next())
        .await
        .expect("Timeout waiting for response")
        .expect("Stream should produce a value");
    let data2_2 = serde_json::to_value(response2_2).expect("Failed to serialize response");
    assert_eq!(data2_2["data"]["transactionUpdated"]["status"], "CONFIRMED");

    Ok(())
}

#[tokio::test]
async fn test_transaction_updated_handles_all_status_types() -> Result<()> {
    let ctx = setup_test_environment().await?;

    let test_statuses = vec![
        (TransactionStatus::Pending, "PENDING"),
        (TransactionStatus::Submitted, "SUBMITTED"),
        (TransactionStatus::Confirmed, "CONFIRMED"),
        (TransactionStatus::Reverted, "REVERTED"),
        (TransactionStatus::Timeout, "TIMEOUT"),
        (TransactionStatus::ValidationFailed, "VALIDATION_FAILED"),
        (TransactionStatus::SubmissionFailed, "SUBMISSION_FAILED"),
    ];

    for (store_status, expected_gql_status) in test_statuses {
        // Create a new transaction for each status test
        let tx_id = uuid::Uuid::new_v4();
        let record = TransactionRecord {
            id: tx_id,
            raw_transaction: vec![0x01, 0x02, 0x03],
            transaction_hash: Hash::default(),
            status: store_status,
            submitted_at: chrono::Utc::now(),
            confirmed_at: if store_status == TransactionStatus::Confirmed {
                Some(chrono::Utc::now())
            } else {
                None
            },
            error_message: None,
            safe_execution: None,
        };
        ctx.store.insert(record)?;

        let query = format!(
            r#"subscription {{
                transactionUpdated(id: "{}") {{
                    status
                }}
            }}"#,
            tx_id
        );

        let mut stream = ctx.schema.execute_stream(&query).boxed();

        // Receive initial state
        let response = tokio::time::timeout(Duration::from_secs(1), stream.next())
            .await
            .expect("Timeout waiting for response")
            .expect("Stream should produce a value");
        let data = serde_json::to_value(response).expect("Failed to serialize response");

        assert_eq!(
            data["data"]["transactionUpdated"]["status"], expected_gql_status,
            "Status mismatch for {:?}",
            store_status
        );
    }

    Ok(())
}
