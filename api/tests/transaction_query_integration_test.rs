//! Comprehensive integration tests for transaction query
//!
//! These tests verify:
//! - Querying transactions by UUID
//! - All transaction status types
//! - Union type handling (Transaction vs InvalidTransactionIdError)
//! - UUID format validation
//! - Non-existent transaction handling

mod common;

use std::{sync::Arc, time::Duration};

use anyhow::Result;
use async_graphql::{EmptySubscription, Schema};
use blokli_api::{mutation::MutationRoot, query::QueryRoot};
use blokli_chain_api::transaction_store::{TransactionRecord, TransactionStatus, TransactionStore};
use blokli_chain_types::ContractAddresses;
use blokli_db::{BlokliDbGeneralModelOperations, TargetDb, db::BlokliDb};
use hopr_crypto_types::types::Hash;
use tokio::task::AbortHandle;

/// Test context for transaction query tests
struct TestContext {
    store: Arc<TransactionStore>,
    schema: Schema<QueryRoot, MutationRoot, EmptySubscription>,
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

/// Set up test environment
async fn setup_test_environment() -> Result<TestContext> {
    // Use common transaction test helper
    let tx_ctx = common::setup_transaction_test_environment(
        Duration::from_secs(1),     // block_time
        Duration::from_millis(100), // poll_interval
        2,                          // finality
        None,                       // executor_config (use default)
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
        store: tx_ctx.store.clone(),
        schema,
        _monitor_handle: tx_ctx.monitor_handle.clone(),
    })
}

/// Helper to execute GraphQL query and return result
async fn execute_query(schema: &Schema<QueryRoot, MutationRoot, EmptySubscription>, query: &str) -> serde_json::Value {
    let response = schema.execute(query).await;
    serde_json::to_value(response).expect("Failed to serialize response")
}

#[tokio::test]
async fn test_transaction_query_finds_existing_transaction() -> Result<()> {
    let ctx = setup_test_environment().await?;

    // Insert a test transaction into the store
    let tx_id = uuid::Uuid::new_v4();
    let record = TransactionRecord {
        id: tx_id,
        raw_transaction: vec![0x01, 0x02, 0x03],
        transaction_hash: Hash::default(),
        status: TransactionStatus::Submitted,
        submitted_at: chrono::Utc::now(),
        confirmed_at: None,
        error_message: None,
    };
    ctx.store.insert(record)?;

    let query = format!(
        r#"query {{
            transaction(id: "{}") {{
                __typename
                ... on Transaction {{
                    id
                    status
                    submittedAt
                    transactionHash
                }}
                ... on InvalidTransactionIdError {{
                    code
                    message
                }}
            }}
        }}"#,
        tx_id
    );

    let result = execute_query(&ctx.schema, &query).await;
    let data = &result["data"]["transaction"];

    // Verify it returns Transaction
    assert_eq!(data["__typename"], "Transaction");
    assert_eq!(data["id"], tx_id.to_string());
    assert_eq!(data["status"], "SUBMITTED");
    assert!(data["submittedAt"].is_string());
    assert!(data["transactionHash"].is_string());

    Ok(())
}

#[tokio::test]
async fn test_transaction_query_returns_none_for_nonexistent_uuid() -> Result<()> {
    let ctx = setup_test_environment().await?;

    // Query with valid UUID that doesn't exist in store
    let non_existent_id = uuid::Uuid::new_v4();

    let query = format!(
        r#"query {{
            transaction(id: "{}") {{
                __typename
                ... on Transaction {{
                    id
                }}
                ... on InvalidTransactionIdError {{
                    code
                    message
                }}
            }}
        }}"#,
        non_existent_id
    );

    let result = execute_query(&ctx.schema, &query).await;
    let data = &result["data"]["transaction"];

    // Should return null for non-existent transaction
    assert!(data.is_null());

    Ok(())
}

#[tokio::test]
async fn test_transaction_query_with_invalid_uuid() -> Result<()> {
    let ctx = setup_test_environment().await?;

    let query = r#"query {
        transaction(id: "not-a-valid-uuid") {
            __typename
            ... on Transaction {
                id
            }
            ... on InvalidTransactionIdError {
                code
                message
            }
        }
    }"#;

    let result = execute_query(&ctx.schema, query).await;
    let data = &result["data"]["transaction"];

    // Should return InvalidTransactionIdError
    assert_eq!(data["__typename"], "InvalidTransactionIdError");
    assert_eq!(data["code"], "INVALID_TRANSACTION_ID");
    assert!(data["message"].is_string());

    Ok(())
}

#[tokio::test]
async fn test_transaction_query_with_empty_id() -> Result<()> {
    let ctx = setup_test_environment().await?;

    let query = r#"query {
        transaction(id: "") {
            __typename
            ... on Transaction {
                id
            }
            ... on InvalidTransactionIdError {
                code
                message
            }
        }
    }"#;

    let result = execute_query(&ctx.schema, query).await;
    let data = &result["data"]["transaction"];

    // Should return InvalidTransactionIdError
    assert_eq!(data["__typename"], "InvalidTransactionIdError");
    assert_eq!(data["code"], "INVALID_TRANSACTION_ID");

    Ok(())
}

#[tokio::test]
async fn test_transaction_query_all_status_types() -> Result<()> {
    let ctx = setup_test_environment().await?;

    let status_tests = vec![
        (TransactionStatus::Pending, "PENDING"),
        (TransactionStatus::Submitted, "SUBMITTED"),
        (TransactionStatus::Confirmed, "CONFIRMED"),
        (TransactionStatus::Reverted, "REVERTED"),
        (TransactionStatus::Timeout, "TIMEOUT"),
        (TransactionStatus::ValidationFailed, "VALIDATION_FAILED"),
        (TransactionStatus::SubmissionFailed, "SUBMISSION_FAILED"),
    ];

    for (status, expected_gql_status) in status_tests {
        let tx_id = uuid::Uuid::new_v4();
        let record = TransactionRecord {
            id: tx_id,
            raw_transaction: vec![0x01, 0x02, 0x03],
            transaction_hash: Hash::default(),
            status,
            submitted_at: chrono::Utc::now(),
            confirmed_at: if status == TransactionStatus::Confirmed {
                Some(chrono::Utc::now())
            } else {
                None
            },
            error_message: None,
        };
        ctx.store.insert(record)?;

        let query = format!(
            r#"query {{
                transaction(id: "{}") {{
                    __typename
                    ... on Transaction {{
                        status
                    }}
                }}
            }}"#,
            tx_id
        );

        let result = execute_query(&ctx.schema, &query).await;
        let data = &result["data"]["transaction"];

        assert_eq!(data["status"], expected_gql_status, "Status mismatch for {:?}", status);
    }

    Ok(())
}

#[tokio::test]
async fn test_transaction_query_uuid_format_variations() -> Result<()> {
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
    };
    ctx.store.insert(record)?;

    // Test with standard UUID format (with hyphens)
    let query1 = format!(
        r#"query {{
            transaction(id: "{}") {{
                __typename
                ... on Transaction {{
                    id
                }}
            }}
        }}"#,
        tx_id
    );

    let result1 = execute_query(&ctx.schema, &query1).await;
    assert_eq!(result1["data"]["transaction"]["__typename"], "Transaction");

    // Test with uppercase UUID
    let query2 = format!(
        r#"query {{
            transaction(id: "{}") {{
                __typename
                ... on Transaction {{
                    id
                }}
            }}
        }}"#,
        tx_id.to_string().to_uppercase()
    );

    let result2 = execute_query(&ctx.schema, &query2).await;
    assert_eq!(result2["data"]["transaction"]["__typename"], "Transaction");

    Ok(())
}

#[tokio::test]
async fn test_transaction_query_with_special_characters_in_id() -> Result<()> {
    let ctx = setup_test_environment().await?;

    let query = r#"query {
        transaction(id: "abc-123-xyz-@#$") {
            __typename
            ... on Transaction {
                id
            }
            ... on InvalidTransactionIdError {
                code
                message
            }
        }
    }"#;

    let result = execute_query(&ctx.schema, query).await;
    let data = &result["data"]["transaction"];

    // Should return InvalidTransactionIdError
    assert_eq!(data["__typename"], "InvalidTransactionIdError");

    Ok(())
}
