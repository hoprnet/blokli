//! Integration tests for ticketParametersUpdated subscription

use std::{sync::Arc, time::Duration};

use alloy::{rpc::client::ClientBuilder, transports::http::ReqwestTransport};
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
    rpc::{RpcOperations, RpcOperationsConfig},
    transport::ReqwestClient,
};
use blokli_chain_types::ContractAddresses;
use blokli_db::{BlokliDbGeneralModelOperations, TargetDb, db::BlokliDb};
use futures::StreamExt;
use hopr_primitive_types::{prelude::HoprBalance, traits::IntoEndian};
use sea_orm::{ActiveModelTrait, EntityTrait, Set};

/// Initialize chain_info with ticket parameters
async fn init_chain_info_with_params(
    db: &sea_orm::DatabaseConnection,
    block: i64,
    ticket_price: HoprBalance,
    min_win_prob: f32,
) -> Result<(), sea_orm::DbErr> {
    // Delete any existing entry first
    blokli_db_entity::chain_info::Entity::delete_many().exec(db).await?;

    // Convert HoprBalance to 12-byte big-endian format (taking last 12 bytes of 32-byte representation)
    let price_bytes_32 = ticket_price.to_be_bytes();
    let ticket_price_bytes = price_bytes_32[20..].to_vec(); // Last 12 bytes

    let chain_info = blokli_db_entity::chain_info::ActiveModel {
        id: Set(1),
        last_indexed_block: Set(block),
        ticket_price: Set(Some(ticket_price_bytes)),
        min_incoming_ticket_win_prob: Set(min_win_prob),
        channels_dst: Set(None),
        ledger_dst: Set(None),
        safe_registry_dst: Set(None),
        channel_closure_grace_period: Set(None),
        last_indexed_tx_index: Set(0),
        last_indexed_log_index: Set(0),
    };
    chain_info.insert(db).await?;
    Ok(())
}

/// Create a minimal GraphQL schema for testing subscriptions
fn create_test_schema(db: &BlokliDb) -> Schema<QueryRoot, MutationRoot, SubscriptionRoot> {
    let indexer_state = IndexerState::new(10, 100);
    let transaction_store = Arc::new(TransactionStore::new());
    let transaction_validator = Arc::new(TransactionValidator::new());

    // Create mock RPC client for testing (won't actually connect)
    let transport = ReqwestTransport::new("http://localhost:8545".parse().unwrap());
    let rpc_client = ClientBuilder::default().transport(transport.clone(), transport.guess_local());
    let transport_client = ReqwestClient::new();

    // Create stub RPC operations (not used for subscription tests)
    let rpc_ops = Arc::new(
        RpcOperations::new(
            rpc_client.clone(),
            transport_client.clone(),
            RpcOperationsConfig::default(),
            None,
        )
        .expect("Failed to create RPC operations"),
    );

    let rpc_adapter = Arc::new(RpcAdapter::new(
        RpcOperations::new(rpc_client, transport_client, RpcOperationsConfig::default(), None)
            .expect("Failed to create RPC adapter operations"),
    ));

    let transaction_executor = Arc::new(RawTransactionExecutor::with_shared_dependencies(
        rpc_adapter,
        transaction_store.clone(),
        transaction_validator,
        RawTransactionExecutorConfig::default(),
    ));

    build_schema(
        db.conn(TargetDb::Index).clone(),
        1,
        "test-network".to_string(),
        ContractAddresses::default(),
        indexer_state,
        transaction_executor,
        transaction_store,
        rpc_ops,
        db.sqlite_notification_manager().cloned(),
    )
}

#[tokio::test]
async fn test_ticket_parameters_subscription_emits_initial_values() {
    let db = BlokliDb::new_in_memory().await.unwrap();

    // Initialize chain_info with ticket parameters
    let ticket_price = HoprBalance::from(1000_u64); // 1000 wei
    let min_win_prob = 0.75_f32;

    init_chain_info_with_params(db.conn(TargetDb::Index), 100, ticket_price, min_win_prob)
        .await
        .unwrap();

    // Create GraphQL schema
    let schema = create_test_schema(&db);

    // Execute subscription query
    let query = r#"
        subscription {
            ticketParametersUpdated {
                minTicketWinningProbability
                ticketPrice
            }
        }
    "#;

    let mut stream = schema.execute_stream(query).boxed();

    // Should receive initial parameters within timeout
    let result = tokio::time::timeout(Duration::from_secs(2), stream.next()).await;

    assert!(result.is_ok(), "Subscription should emit within timeout");
    let response = result.unwrap().unwrap();

    // Verify no errors
    assert!(
        response.errors.is_empty(),
        "Response should not have errors: {:?}",
        response.errors
    );

    // Extract data
    let data = response.data.into_json().unwrap();
    let params = &data["ticketParametersUpdated"];

    // Verify ticket price (1000 wei)
    assert_eq!(params["ticketPrice"].as_str().unwrap(), "1000");

    // Verify winning probability
    let prob = params["minTicketWinningProbability"].as_f64().unwrap();
    assert!((prob - 0.75).abs() < 0.001);
}

#[tokio::test]
async fn test_ticket_parameters_subscription_handles_missing_ticket_price() {
    let db = BlokliDb::new_in_memory().await.unwrap();

    // Delete any existing entry first
    blokli_db_entity::chain_info::Entity::delete_many()
        .exec(db.conn(TargetDb::Index))
        .await
        .unwrap();

    // Initialize chain_info without ticket_price (None)
    let chain_info = blokli_db_entity::chain_info::ActiveModel {
        id: Set(1),
        last_indexed_block: Set(100),
        ticket_price: Set(None), // Missing ticket price
        min_incoming_ticket_win_prob: Set(0.5),
        channels_dst: Set(None),
        ledger_dst: Set(None),
        safe_registry_dst: Set(None),
        channel_closure_grace_period: Set(None),
        last_indexed_tx_index: Set(0),
        last_indexed_log_index: Set(0),
    };
    chain_info.insert(db.conn(TargetDb::Index)).await.unwrap();

    // Create GraphQL schema
    let schema = create_test_schema(&db);

    // Execute subscription query
    let query = r#"
        subscription {
            ticketParametersUpdated {
                minTicketWinningProbability
                ticketPrice
            }
        }
    "#;

    let mut stream = schema.execute_stream(query).boxed();

    // Should receive default ticket price (0) when None
    let result = tokio::time::timeout(Duration::from_secs(2), stream.next()).await;

    assert!(result.is_ok(), "Subscription should emit within timeout");
    let response = result.unwrap().unwrap();
    assert!(response.errors.is_empty());

    let data = response.data.into_json().unwrap();
    let params = &data["ticketParametersUpdated"];

    // Default to "0" when ticket_price is None
    assert_eq!(params["ticketPrice"].as_str().unwrap(), "0");
    let prob = params["minTicketWinningProbability"].as_f64().unwrap();
    assert!((prob - 0.5).abs() < 0.001);
}

#[tokio::test]
async fn test_ticket_parameters_subscription_handles_zero_values() {
    let db = BlokliDb::new_in_memory().await.unwrap();

    // Initialize with zero values
    let zero_price = HoprBalance::zero();
    init_chain_info_with_params(db.conn(TargetDb::Index), 100, zero_price, 0.0)
        .await
        .unwrap();

    // Create GraphQL schema
    let schema = create_test_schema(&db);

    // Execute subscription query
    let query = r#"
        subscription {
            ticketParametersUpdated {
                minTicketWinningProbability
                ticketPrice
            }
        }
    "#;

    let mut stream = schema.execute_stream(query).boxed();

    // Should handle zero values gracefully
    let result = tokio::time::timeout(Duration::from_secs(2), stream.next()).await;

    assert!(result.is_ok(), "Subscription should emit within timeout");
    let response = result.unwrap().unwrap();
    assert!(response.errors.is_empty());

    let data = response.data.into_json().unwrap();
    let params = &data["ticketParametersUpdated"];

    assert_eq!(params["ticketPrice"].as_str().unwrap(), "0");
    let prob = params["minTicketWinningProbability"].as_f64().unwrap();
    assert!((prob - 0.0).abs() < 0.001);
}

#[tokio::test]
async fn test_ticket_parameters_subscription_handles_max_values() {
    let db = BlokliDb::new_in_memory().await.unwrap();

    // Initialize with maximum u64 value (HoprBalance only supports u64)
    let max_price = HoprBalance::from(u64::MAX);
    init_chain_info_with_params(db.conn(TargetDb::Index), 100, max_price, 1.0)
        .await
        .unwrap();

    // Create GraphQL schema
    let schema = create_test_schema(&db);

    // Execute subscription query
    let query = r#"
        subscription {
            ticketParametersUpdated {
                minTicketWinningProbability
                ticketPrice
            }
        }
    "#;

    let mut stream = schema.execute_stream(query).boxed();

    // Should handle maximum values
    let result = tokio::time::timeout(Duration::from_secs(2), stream.next()).await;

    assert!(result.is_ok(), "Subscription should emit within timeout");
    let response = result.unwrap().unwrap();
    assert!(response.errors.is_empty());

    let data = response.data.into_json().unwrap();
    let params = &data["ticketParametersUpdated"];

    // Max u64 value = 18446744073709551615
    assert_eq!(params["ticketPrice"].as_str().unwrap(), "18446744073709551615");
    let prob = params["minTicketWinningProbability"].as_f64().unwrap();
    assert!((prob - 1.0).abs() < 0.001);
}

#[tokio::test]
async fn test_subscription_receives_ticket_price_update() {
    let db = BlokliDb::new_in_memory().await.unwrap();

    // Initialize with initial value
    init_chain_info_with_params(db.conn(TargetDb::Index), 100, HoprBalance::from(1000_u64), 0.5)
        .await
        .unwrap();

    let schema = create_test_schema(&db);

    let query = r#"
        subscription {
            ticketParametersUpdated {
                minTicketWinningProbability
                ticketPrice
            }
        }
    "#;

    let mut stream = schema.execute_stream(query).boxed();

    // Receive initial value
    let initial = tokio::time::timeout(Duration::from_secs(2), stream.next())
        .await
        .unwrap()
        .unwrap();

    assert!(initial.errors.is_empty());
    let initial_data = initial.data.into_json().unwrap();
    assert_eq!(
        initial_data["ticketParametersUpdated"]["ticketPrice"].as_str().unwrap(),
        "1000"
    );
    assert_eq!(
        initial_data["ticketParametersUpdated"]["minTicketWinningProbability"]
            .as_f64()
            .unwrap(),
        0.5
    );

    // Update ticket price in database
    init_chain_info_with_params(db.conn(TargetDb::Index), 110, HoprBalance::from(2000_u64), 0.5)
        .await
        .unwrap();

    // Should receive update
    let updated = tokio::time::timeout(Duration::from_secs(3), stream.next())
        .await
        .unwrap()
        .unwrap();

    assert!(updated.errors.is_empty());
    let updated_data = updated.data.into_json().unwrap();
    assert_eq!(
        updated_data["ticketParametersUpdated"]["ticketPrice"].as_str().unwrap(),
        "2000"
    );
    assert_eq!(
        updated_data["ticketParametersUpdated"]["minTicketWinningProbability"]
            .as_f64()
            .unwrap(),
        0.5
    );
}

#[tokio::test]
async fn test_subscription_receives_winning_probability_update() {
    let db = BlokliDb::new_in_memory().await.unwrap();

    // Initialize with initial value
    init_chain_info_with_params(db.conn(TargetDb::Index), 100, HoprBalance::from(1000_u64), 0.5)
        .await
        .unwrap();

    let schema = create_test_schema(&db);

    let query = r#"
        subscription {
            ticketParametersUpdated {
                minTicketWinningProbability
                ticketPrice
            }
        }
    "#;

    let mut stream = schema.execute_stream(query).boxed();

    // Receive initial value
    let initial = tokio::time::timeout(Duration::from_secs(2), stream.next())
        .await
        .unwrap()
        .unwrap();

    assert!(initial.errors.is_empty());
    let initial_data = initial.data.into_json().unwrap();
    assert_eq!(
        initial_data["ticketParametersUpdated"]["minTicketWinningProbability"]
            .as_f64()
            .unwrap(),
        0.5
    );

    // Update winning probability in database
    init_chain_info_with_params(db.conn(TargetDb::Index), 110, HoprBalance::from(1000_u64), 0.9)
        .await
        .unwrap();

    // Should receive update (polling will pick it up within 2 seconds)
    let updated = tokio::time::timeout(Duration::from_secs(3), stream.next())
        .await
        .unwrap()
        .unwrap();

    assert!(updated.errors.is_empty());
    let updated_data = updated.data.into_json().unwrap();
    let prob = updated_data["ticketParametersUpdated"]["minTicketWinningProbability"]
        .as_f64()
        .unwrap();
    assert!((prob - 0.9).abs() < 0.01, "Expected ~0.9, got {}", prob);
    assert_eq!(
        updated_data["ticketParametersUpdated"]["ticketPrice"].as_str().unwrap(),
        "1000"
    );
}

#[tokio::test]
async fn test_subscription_receives_both_parameters_update() {
    let db = BlokliDb::new_in_memory().await.unwrap();

    // Initialize with initial values
    init_chain_info_with_params(db.conn(TargetDb::Index), 100, HoprBalance::from(500_u64), 0.3)
        .await
        .unwrap();

    let schema = create_test_schema(&db);

    let query = r#"
        subscription {
            ticketParametersUpdated {
                minTicketWinningProbability
                ticketPrice
            }
        }
    "#;

    let mut stream = schema.execute_stream(query).boxed();

    // Receive initial value
    let initial = tokio::time::timeout(Duration::from_secs(2), stream.next())
        .await
        .unwrap()
        .unwrap();

    assert!(initial.errors.is_empty());

    // Update both parameters simultaneously
    init_chain_info_with_params(db.conn(TargetDb::Index), 110, HoprBalance::from(3000_u64), 0.95)
        .await
        .unwrap();

    // Should receive update with both new values
    let updated = tokio::time::timeout(Duration::from_secs(3), stream.next())
        .await
        .unwrap()
        .unwrap();

    assert!(updated.errors.is_empty());
    let updated_data = updated.data.into_json().unwrap();
    assert_eq!(
        updated_data["ticketParametersUpdated"]["ticketPrice"].as_str().unwrap(),
        "3000"
    );
    let prob = updated_data["ticketParametersUpdated"]["minTicketWinningProbability"]
        .as_f64()
        .unwrap();
    assert!((prob - 0.95).abs() < 0.01, "Expected ~0.95, got {}", prob);
}

#[tokio::test]
async fn test_subscription_receives_multiple_updates() {
    let db = BlokliDb::new_in_memory().await.unwrap();

    // Initialize
    init_chain_info_with_params(db.conn(TargetDb::Index), 100, HoprBalance::from(100_u64), 0.1)
        .await
        .unwrap();

    let schema = create_test_schema(&db);

    let query = r#"
        subscription {
            ticketParametersUpdated {
                ticketPrice
            }
        }
    "#;

    let mut stream = schema.execute_stream(query).boxed();

    // Receive initial
    let initial = tokio::time::timeout(Duration::from_secs(2), stream.next())
        .await
        .unwrap()
        .unwrap();
    assert!(initial.errors.is_empty());

    // Update 1
    init_chain_info_with_params(db.conn(TargetDb::Index), 110, HoprBalance::from(200_u64), 0.1)
        .await
        .unwrap();

    let update1 = tokio::time::timeout(Duration::from_secs(3), stream.next())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        update1.data.into_json().unwrap()["ticketParametersUpdated"]["ticketPrice"]
            .as_str()
            .unwrap(),
        "200"
    );

    // Update 2
    init_chain_info_with_params(db.conn(TargetDb::Index), 120, HoprBalance::from(300_u64), 0.1)
        .await
        .unwrap();

    let update2 = tokio::time::timeout(Duration::from_secs(3), stream.next())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        update2.data.into_json().unwrap()["ticketParametersUpdated"]["ticketPrice"]
            .as_str()
            .unwrap(),
        "300"
    );
}
