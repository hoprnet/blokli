//! Integration tests for keyBindingFeeUpdated subscription
//!
//! These tests verify the key binding fee subscription functionality:
//! - Initial fee emission from chain_info
//! - Fee update events from IndexerState
//! - No duplicate emissions for same value
//! - Edge cases (zero fee, very large fee)

use std::{str::FromStr, sync::Arc, time::Duration};

use alloy::{rpc::client::ClientBuilder, transports::http::ReqwestTransport};
use async_graphql::Schema;
use blokli_api::{mutation::MutationRoot, query::QueryRoot, schema::build_schema, subscription::SubscriptionRoot};
use blokli_api_types::{Account, TokenValueString};
use blokli_chain_api::{
    rpc_adapter::RpcAdapter,
    transaction_executor::{RawTransactionExecutor, RawTransactionExecutorConfig},
    transaction_store::TransactionStore,
    transaction_validator::TransactionValidator,
};
use blokli_chain_indexer::{IndexerState, state::IndexerEvent};
use blokli_chain_rpc::{
    rpc::{RpcOperations, RpcOperationsConfig},
    transport::ReqwestClient,
};
use blokli_chain_types::ContractAddresses;
use blokli_db::{BlokliDbGeneralModelOperations, TargetDb, db::BlokliDb};
use futures::StreamExt;
use hopr_primitive_types::{prelude::HoprBalance, primitives::Address, traits::IntoEndian};
use sea_orm::{ActiveModelTrait, Set};

/// Create a minimal GraphQL schema for testing subscriptions
fn create_test_schema(db: &BlokliDb, indexer_state: IndexerState) -> Schema<QueryRoot, MutationRoot, SubscriptionRoot> {
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
async fn test_key_binding_fee_subscription_emits_initial_fee() {
    let db = BlokliDb::new_in_memory().await.unwrap();
    let indexer_state = IndexerState::new(10, 100);

    // Set initial key binding fee in chain_info
    let fee = HoprBalance::from_str("100 wxHOPR").unwrap();
    let chain_info = blokli_db_entity::chain_info::ActiveModel {
        id: Set(1),
        key_binding_fee: Set(Some(fee.to_be_bytes().to_vec())),
        ..Default::default()
    };
    chain_info.update(db.conn(TargetDb::Index)).await.unwrap();

    // Create GraphQL schema
    let schema = create_test_schema(&db, indexer_state);

    // Execute subscription query
    let query = r#"
        subscription {
            keyBindingFeeUpdated
        }
    "#;

    let mut stream = schema.execute_stream(query).boxed();

    // Should receive initial fee within timeout
    let result = tokio::time::timeout(Duration::from_secs(2), stream.next()).await;

    assert!(result.is_ok(), "Subscription should emit initial fee within timeout");
    let response = result.unwrap().unwrap();

    // Verify no errors
    assert!(
        response.errors.is_empty(),
        "Response should not have errors: {:?}",
        response.errors
    );

    // Extract data
    let data = response.data.into_json().unwrap();
    let fee_str = data["keyBindingFeeUpdated"].as_str().unwrap();

    // Fee includes token identifier
    assert_eq!(fee_str, "100 wxHOPR");
}

#[tokio::test]
async fn test_key_binding_fee_subscription_receives_update_event() {
    let db = BlokliDb::new_in_memory().await.unwrap();
    let indexer_state = IndexerState::new(10, 100);

    // Set initial key binding fee
    let initial_fee = HoprBalance::from_str("100 wxHOPR").unwrap();
    let chain_info = blokli_db_entity::chain_info::ActiveModel {
        id: Set(1),
        key_binding_fee: Set(Some(initial_fee.to_be_bytes().to_vec())),
        ..Default::default()
    };
    chain_info.update(db.conn(TargetDb::Index)).await.unwrap();

    let schema = create_test_schema(&db, indexer_state.clone());

    let query = r#"
        subscription {
            keyBindingFeeUpdated
        }
    "#;

    let mut stream = schema.execute_stream(query).boxed();

    // Receive initial fee
    let initial = tokio::time::timeout(Duration::from_secs(2), stream.next())
        .await
        .unwrap()
        .unwrap();

    assert!(initial.errors.is_empty());
    let initial_data = initial.data.into_json().unwrap();
    assert_eq!(initial_data["keyBindingFeeUpdated"].as_str().unwrap(), "100 wxHOPR");

    // Publish update event with new fee
    let new_fee = TokenValueString("200 wxHOPR".to_string());
    indexer_state.publish_event(IndexerEvent::KeyBindingFeeUpdated(new_fee));

    // Should receive updated fee
    let updated = tokio::time::timeout(Duration::from_secs(2), stream.next())
        .await
        .unwrap()
        .unwrap();

    assert!(updated.errors.is_empty());
    let updated_data = updated.data.into_json().unwrap();
    assert_eq!(updated_data["keyBindingFeeUpdated"].as_str().unwrap(), "200 wxHOPR");
}

#[tokio::test]
async fn test_key_binding_fee_subscription_no_duplicate_emissions() {
    let db = BlokliDb::new_in_memory().await.unwrap();
    let indexer_state = IndexerState::new(10, 100);

    // Set initial key binding fee
    let initial_fee = HoprBalance::from_str("100 wxHOPR").unwrap();
    let chain_info = blokli_db_entity::chain_info::ActiveModel {
        id: Set(1),
        key_binding_fee: Set(Some(initial_fee.to_be_bytes().to_vec())),
        ..Default::default()
    };
    chain_info.update(db.conn(TargetDb::Index)).await.unwrap();

    let schema = create_test_schema(&db, indexer_state.clone());

    let query = r#"
        subscription {
            keyBindingFeeUpdated
        }
    "#;

    let mut stream = schema.execute_stream(query).boxed();

    // Receive initial fee
    let initial = tokio::time::timeout(Duration::from_secs(2), stream.next())
        .await
        .unwrap()
        .unwrap();

    assert!(initial.errors.is_empty());

    // Publish same fee value again (should not emit duplicate)
    let same_fee = TokenValueString("100 wxHOPR".to_string());
    indexer_state.publish_event(IndexerEvent::KeyBindingFeeUpdated(same_fee));

    // Should NOT receive duplicate emission within timeout
    let no_duplicate = tokio::time::timeout(Duration::from_millis(1500), stream.next()).await;
    assert!(no_duplicate.is_err(), "Should not emit duplicate values");
}

#[tokio::test]
async fn test_key_binding_fee_subscription_multiple_updates() {
    let db = BlokliDb::new_in_memory().await.unwrap();
    let indexer_state = IndexerState::new(10, 100);

    // Set initial key binding fee
    let initial_fee = HoprBalance::from_str("100 wxHOPR").unwrap();
    let chain_info = blokli_db_entity::chain_info::ActiveModel {
        id: Set(1),
        key_binding_fee: Set(Some(initial_fee.to_be_bytes().to_vec())),
        ..Default::default()
    };
    chain_info.update(db.conn(TargetDb::Index)).await.unwrap();

    let schema = create_test_schema(&db, indexer_state.clone());

    let query = r#"
        subscription {
            keyBindingFeeUpdated
        }
    "#;

    let mut stream = schema.execute_stream(query).boxed();

    // Receive initial fee
    let initial = tokio::time::timeout(Duration::from_secs(2), stream.next())
        .await
        .unwrap()
        .unwrap();
    assert!(initial.errors.is_empty());

    // Update 1: 200 HOPR
    let fee1 = TokenValueString("200000000000000000000".to_string());
    indexer_state.publish_event(IndexerEvent::KeyBindingFeeUpdated(fee1));

    let update1 = tokio::time::timeout(Duration::from_secs(2), stream.next())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        update1.data.into_json().unwrap()["keyBindingFeeUpdated"]
            .as_str()
            .unwrap(),
        "200000000000000000000"
    );

    // Update 2: 300 HOPR
    let fee2 = TokenValueString("300000000000000000000".to_string());
    indexer_state.publish_event(IndexerEvent::KeyBindingFeeUpdated(fee2));

    let update2 = tokio::time::timeout(Duration::from_secs(2), stream.next())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        update2.data.into_json().unwrap()["keyBindingFeeUpdated"]
            .as_str()
            .unwrap(),
        "300000000000000000000"
    );

    // Update 3: 150 HOPR (can decrease)
    let fee3 = TokenValueString("150000000000000000000".to_string());
    indexer_state.publish_event(IndexerEvent::KeyBindingFeeUpdated(fee3));

    let update3 = tokio::time::timeout(Duration::from_secs(2), stream.next())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        update3.data.into_json().unwrap()["keyBindingFeeUpdated"]
            .as_str()
            .unwrap(),
        "150000000000000000000"
    );
}

#[tokio::test]
async fn test_key_binding_fee_subscription_zero_fee() {
    let db = BlokliDb::new_in_memory().await.unwrap();
    let indexer_state = IndexerState::new(10, 100);

    // Set zero fee
    let zero_fee = HoprBalance::from_str("0 wxHOPR").unwrap();
    let chain_info = blokli_db_entity::chain_info::ActiveModel {
        id: Set(1),
        key_binding_fee: Set(Some(zero_fee.to_be_bytes().to_vec())),
        ..Default::default()
    };
    chain_info.update(db.conn(TargetDb::Index)).await.unwrap();

    let schema = create_test_schema(&db, indexer_state);

    let query = r#"
        subscription {
            keyBindingFeeUpdated
        }
    "#;

    let mut stream = schema.execute_stream(query).boxed();

    let result = tokio::time::timeout(Duration::from_secs(2), stream.next())
        .await
        .unwrap()
        .unwrap();

    assert!(result.errors.is_empty());
    let data = result.data.into_json().unwrap();
    assert_eq!(data["keyBindingFeeUpdated"].as_str().unwrap(), "0 wxHOPR");
}

#[tokio::test]
async fn test_key_binding_fee_subscription_large_fee() {
    let db = BlokliDb::new_in_memory().await.unwrap();
    let indexer_state = IndexerState::new(10, 100);

    // Set very large fee (1 million HOPR)
    let large_fee = HoprBalance::from_str("1000000 wxHOPR").unwrap();
    let chain_info = blokli_db_entity::chain_info::ActiveModel {
        id: Set(1),
        key_binding_fee: Set(Some(large_fee.to_be_bytes().to_vec())),
        ..Default::default()
    };
    chain_info.update(db.conn(TargetDb::Index)).await.unwrap();

    let schema = create_test_schema(&db, indexer_state);

    let query = r#"
        subscription {
            keyBindingFeeUpdated
        }
    "#;

    let mut stream = schema.execute_stream(query).boxed();

    let result = tokio::time::timeout(Duration::from_secs(2), stream.next())
        .await
        .unwrap()
        .unwrap();

    assert!(result.errors.is_empty());
    let data = result.data.into_json().unwrap();
    // 1,000,000 HOPR with token identifier
    assert_eq!(data["keyBindingFeeUpdated"].as_str().unwrap(), "1000000 wxHOPR");
}

#[tokio::test]
async fn test_key_binding_fee_subscription_no_initial_fee() {
    let db = BlokliDb::new_in_memory().await.unwrap();
    let indexer_state = IndexerState::new(10, 100);

    // Don't set any initial fee (chain_info has None)
    // chain_info row exists by default from BlokliDb::new_in_memory() but key_binding_fee is None

    let schema = create_test_schema(&db, indexer_state.clone());

    let query = r#"
        subscription {
            keyBindingFeeUpdated
        }
    "#;

    let mut stream = schema.execute_stream(query).boxed();

    // Should not emit anything initially if fee is None
    let no_initial = tokio::time::timeout(Duration::from_millis(1000), stream.next()).await;
    assert!(
        no_initial.is_err(),
        "Should not emit if chain_info.key_binding_fee is None"
    );

    // Now publish an event
    let fee = TokenValueString("100000000000000000000".to_string());
    indexer_state.publish_event(IndexerEvent::KeyBindingFeeUpdated(fee));

    // Should receive the event
    let result = tokio::time::timeout(Duration::from_secs(2), stream.next())
        .await
        .unwrap()
        .unwrap();

    assert!(result.errors.is_empty());
    let data = result.data.into_json().unwrap();
    assert_eq!(data["keyBindingFeeUpdated"].as_str().unwrap(), "100000000000000000000");
}

#[tokio::test]
async fn test_key_binding_fee_subscription_ignores_other_events() {
    let db = BlokliDb::new_in_memory().await.unwrap();
    let indexer_state = IndexerState::new(10, 100);

    // Set initial fee
    let initial_fee = HoprBalance::from_str("100 wxHOPR").unwrap();
    let chain_info = blokli_db_entity::chain_info::ActiveModel {
        id: Set(1),
        key_binding_fee: Set(Some(initial_fee.to_be_bytes().to_vec())),
        ..Default::default()
    };
    chain_info.update(db.conn(TargetDb::Index)).await.unwrap();

    let schema = create_test_schema(&db, indexer_state.clone());

    let query = r#"
        subscription {
            keyBindingFeeUpdated
        }
    "#;

    let mut stream = schema.execute_stream(query).boxed();

    // Receive initial fee
    let initial = tokio::time::timeout(Duration::from_secs(2), stream.next())
        .await
        .unwrap()
        .unwrap();
    assert!(initial.errors.is_empty());

    // Publish irrelevant events (should be ignored)
    indexer_state.publish_event(IndexerEvent::AccountUpdated(Account {
        keyid: 1,
        chain_key: "0x1234".to_string(),
        packet_key: "0x5678".to_string(),
        safe_address: None,
        multi_addresses: vec![],
    }));

    let safe_address = Address::from([0xab; 20]);
    indexer_state.publish_event(IndexerEvent::SafeDeployed(safe_address));

    // Should not receive anything from irrelevant events
    let no_irrelevant = tokio::time::timeout(Duration::from_millis(1000), stream.next()).await;
    assert!(no_irrelevant.is_err(), "Should ignore non-KeyBindingFeeUpdated events");

    // Now publish relevant event
    let new_fee = TokenValueString("200000000000000000000".to_string());
    indexer_state.publish_event(IndexerEvent::KeyBindingFeeUpdated(new_fee));

    // Should receive the KeyBindingFeeUpdated event
    let result = tokio::time::timeout(Duration::from_secs(2), stream.next())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        result.data.into_json().unwrap()["keyBindingFeeUpdated"]
            .as_str()
            .unwrap(),
        "200000000000000000000"
    );
}
