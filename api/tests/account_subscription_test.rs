//! Integration tests for accountUpdated subscription
//!
//! These tests verify the real-time account update subscription functionality:
//! - Initial account emission on subscription
//! - Filter combinations (keyid, packetKey, chainKey)
//! - Account updates (Safe linking, announcements)
//! - Multiple concurrent subscribers
//! - Unsubscribe behavior

use std::{sync::Arc, time::Duration};

use alloy::{rpc::client::ClientBuilder, transports::http::ReqwestTransport};
use async_graphql::Schema;
use blokli_api::{mutation::MutationRoot, query::QueryRoot, schema::build_schema, subscription::SubscriptionRoot};
use blokli_api_types::Account;
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
use blokli_db::{BlokliDbGeneralModelOperations, TargetDb, accounts::BlokliDbAccountOperations, db::BlokliDb};
use futures::StreamExt;
use hopr_crypto_types::prelude::{ChainKeypair, Keypair, OffchainKeypair};
use hopr_primitive_types::{prelude::Address, traits::ToHex};
use multiaddr::Multiaddr;

/// Helper to generate random keypair for testing
fn random_keypair() -> ChainKeypair {
    ChainKeypair::random()
}

/// Helper to generate random offchain keypair
fn random_offchain_keypair() -> OffchainKeypair {
    OffchainKeypair::random()
}

/// Create a minimal GraphQL schema for testing subscriptions
/// Returns both the schema and the IndexerState for publishing events
fn create_test_schema(db: &BlokliDb) -> (Schema<QueryRoot, MutationRoot, SubscriptionRoot>, IndexerState) {
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

    let schema = build_schema(
        db.conn(TargetDb::Index).clone(),
        1,
        "test-network".to_string(),
        ContractAddresses::default(),
        indexer_state.clone(),
        transaction_executor,
        transaction_store,
        rpc_ops,
        db.sqlite_notification_manager().cloned(),
    );

    (schema, indexer_state)
}

#[tokio::test]
async fn test_account_subscription_emits_initial_account_with_keyid_filter() {
    let db = BlokliDb::new_in_memory().await.unwrap();

    // Create test account
    let keypair = random_keypair();
    let offchain = random_offchain_keypair();
    let chain_key = keypair.public().to_address();
    let packet_key = offchain.public();
    let safe_address = Address::from([0x01; 20]);

    db.upsert_account(None, 1, chain_key, *packet_key, Some(safe_address), 100, 0, 0)
        .await
        .unwrap();

    // Create GraphQL schema
    let (schema, _indexer_state) = create_test_schema(&db);

    // Execute subscription query with keyid filter
    let query = r#"
        subscription {
            accountUpdated(keyid: 1) {
                keyid
                chainKey
                packetKey
                safeAddress
                multiAddresses
            }
        }
    "#;

    let mut stream = schema.execute_stream(query).boxed();

    // Should receive initial account within timeout
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
    let account = &data["accountUpdated"];

    // Verify account fields
    assert_eq!(account["keyid"].as_i64().unwrap(), 1);
    assert_eq!(account["chainKey"].as_str().unwrap(), chain_key.to_hex());
    assert!(packet_key.to_hex().contains(account["packetKey"].as_str().unwrap()));
    assert_eq!(account["safeAddress"].as_str().unwrap(), safe_address.to_hex());
    assert_eq!(account["multiAddresses"].as_array().unwrap().len(), 0);
}

#[tokio::test]
async fn test_account_subscription_emits_initial_account_with_packet_key_filter() {
    let db = BlokliDb::new_in_memory().await.unwrap();

    // Create test account
    let keypair = random_keypair();
    let offchain = random_offchain_keypair();
    let chain_key = keypair.public().to_address();
    let packet_key = offchain.public();

    db.upsert_account(None, 1, chain_key, *packet_key, None, 100, 0, 0)
        .await
        .unwrap();

    // Create GraphQL schema
    let (schema, _indexer_state) = create_test_schema(&db);

    // Execute subscription query with packetKey filter
    let query = format!(
        r#"
        subscription {{
            accountUpdated(packetKey: "{}") {{
                keyid
                chainKey
                packetKey
                safeAddress
            }}
        }}
    "#,
        packet_key.to_hex()
    );

    let mut stream = schema.execute_stream(&query).boxed();

    // Should receive initial account within timeout
    let result = tokio::time::timeout(Duration::from_secs(2), stream.next()).await;

    assert!(result.is_ok(), "Subscription should emit within timeout");
    let response = result.unwrap().unwrap();

    assert!(response.errors.is_empty());

    let data = response.data.into_json().unwrap();
    let account = &data["accountUpdated"];

    assert_eq!(account["keyid"].as_i64().unwrap(), 1);
    assert_eq!(account["chainKey"].as_str().unwrap(), chain_key.to_hex());
    assert!(packet_key.to_hex().contains(account["packetKey"].as_str().unwrap()));
    assert!(account["safeAddress"].is_null());
}

#[tokio::test]
async fn test_account_subscription_emits_initial_account_with_chain_key_filter() {
    let db = BlokliDb::new_in_memory().await.unwrap();

    // Create test account
    let keypair = random_keypair();
    let offchain = random_offchain_keypair();
    let chain_key = keypair.public().to_address();
    let packet_key = offchain.public();

    db.upsert_account(None, 1, chain_key, *packet_key, None, 100, 0, 0)
        .await
        .unwrap();

    // Create GraphQL schema
    let (schema, _indexer_state) = create_test_schema(&db);

    // Execute subscription query with chainKey filter
    let query = format!(
        r#"
        subscription {{
            accountUpdated(chainKey: "{}") {{
                keyid
                chainKey
                packetKey
            }}
        }}
    "#,
        chain_key.to_hex()
    );

    let mut stream = schema.execute_stream(&query).boxed();

    // Should receive initial account within timeout
    let result = tokio::time::timeout(Duration::from_secs(2), stream.next()).await;

    assert!(result.is_ok(), "Subscription should emit within timeout");
    let response = result.unwrap().unwrap();

    assert!(response.errors.is_empty());

    let data = response.data.into_json().unwrap();
    let account = &data["accountUpdated"];

    assert_eq!(account["keyid"].as_i64().unwrap(), 1);
    assert_eq!(account["chainKey"].as_str().unwrap(), chain_key.to_hex());
    assert!(packet_key.to_hex().contains(account["packetKey"].as_str().unwrap()));
}

#[tokio::test]
async fn test_account_subscription_without_filters_emits_all_accounts() {
    let db = BlokliDb::new_in_memory().await.unwrap();

    // Create multiple test accounts
    let keypair1 = random_keypair();
    let offchain1 = random_offchain_keypair();
    db.upsert_account(
        None,
        1,
        keypair1.public().to_address(),
        *offchain1.public(),
        None,
        100,
        0,
        0,
    )
    .await
    .unwrap();

    let keypair2 = random_keypair();
    let offchain2 = random_offchain_keypair();
    db.upsert_account(
        None,
        2,
        keypair2.public().to_address(),
        *offchain2.public(),
        None,
        101,
        0,
        0,
    )
    .await
    .unwrap();

    // Create GraphQL schema
    let (schema, _indexer_state) = create_test_schema(&db);

    // Execute subscription query without filters
    let query = r#"
        subscription {
            accountUpdated {
                keyid
            }
        }
    "#;

    let mut stream = schema.execute_stream(query).boxed();

    // Should receive accounts within timeout
    let result1 = tokio::time::timeout(Duration::from_secs(2), stream.next()).await;
    assert!(result1.is_ok(), "Should receive first account");

    let result2 = tokio::time::timeout(Duration::from_secs(2), stream.next()).await;
    assert!(result2.is_ok(), "Should receive second account");

    // Collect keyids
    let response1 = result1.unwrap().unwrap();
    let data1 = response1.data.into_json().unwrap();
    let keyid1 = data1["accountUpdated"]["keyid"].as_i64().unwrap();

    let response2 = result2.unwrap().unwrap();
    let data2 = response2.data.into_json().unwrap();
    let keyid2 = data2["accountUpdated"]["keyid"].as_i64().unwrap();

    // Should receive both accounts (order may vary)
    let mut keyids = vec![keyid1, keyid2];
    keyids.sort();
    assert_eq!(keyids, vec![1, 2]);
}

#[tokio::test]
async fn test_account_subscription_receives_safe_address_update() {
    let db = BlokliDb::new_in_memory().await.unwrap();

    // Create account without safe address
    let keypair = random_keypair();
    let offchain = random_offchain_keypair();
    let chain_key = keypair.public().to_address();
    let packet_key = offchain.public();

    db.upsert_account(None, 1, chain_key, *packet_key, None, 100, 0, 0)
        .await
        .unwrap();

    let (schema, indexer_state) = create_test_schema(&db);

    let query = r#"
        subscription {
            accountUpdated(keyid: 1) {
                keyid
                safeAddress
            }
        }
    "#;

    let mut stream = schema.execute_stream(query).boxed();

    // Receive initial value (no safe address)
    let initial = tokio::time::timeout(Duration::from_secs(2), stream.next())
        .await
        .unwrap()
        .unwrap();

    assert!(initial.errors.is_empty());
    let initial_data = initial.data.into_json().unwrap();
    assert!(initial_data["accountUpdated"]["safeAddress"].is_null());

    // Update account with safe address
    let safe_address = Address::from([0x02; 20]);
    db.upsert_account(None, 1, chain_key, *packet_key, Some(safe_address), 110, 0, 0)
        .await
        .unwrap();

    // Publish AccountUpdated event (this is what the indexer would do)
    indexer_state.publish_event(IndexerEvent::AccountUpdated(Account {
        keyid: 1,
        chain_key: chain_key.to_hex(),
        packet_key: packet_key.to_hex(),
        safe_address: Some(safe_address.to_hex()),
        multi_addresses: vec![],
    }));

    // Should receive update with safe address
    let updated = tokio::time::timeout(Duration::from_millis(500), stream.next())
        .await
        .unwrap()
        .unwrap();

    assert!(updated.errors.is_empty());
    let updated_data = updated.data.into_json().unwrap();
    assert_eq!(
        updated_data["accountUpdated"]["safeAddress"].as_str().unwrap(),
        safe_address.to_hex()
    );
}

#[tokio::test]
async fn test_account_subscription_receives_announcement_update() {
    let db = BlokliDb::new_in_memory().await.unwrap();

    // Create account
    let keypair = random_keypair();
    let offchain = random_offchain_keypair();
    let chain_key = keypair.public().to_address();
    let packet_key = offchain.public();

    db.upsert_account(None, 1, chain_key, *packet_key, None, 100, 0, 0)
        .await
        .unwrap();

    let (schema, indexer_state) = create_test_schema(&db);

    let query = r#"
        subscription {
            accountUpdated(keyid: 1) {
                keyid
                multiAddresses
            }
        }
    "#;

    let mut stream = schema.execute_stream(query).boxed();

    // Receive initial value (no announcements)
    let initial = tokio::time::timeout(Duration::from_secs(2), stream.next())
        .await
        .unwrap()
        .unwrap();

    assert!(initial.errors.is_empty());
    let initial_data = initial.data.into_json().unwrap();
    assert_eq!(
        initial_data["accountUpdated"]["multiAddresses"]
            .as_array()
            .unwrap()
            .len(),
        0
    );

    // Add announcement
    let multiaddr: Multiaddr = "/ip4/127.0.0.1/tcp/9091".parse().unwrap();
    db.insert_announcement(None, chain_key, multiaddr.clone(), 110)
        .await
        .unwrap();

    // Publish AccountUpdated event (this is what the indexer would do)
    indexer_state.publish_event(IndexerEvent::AccountUpdated(Account {
        keyid: 1,
        chain_key: chain_key.to_hex(),
        packet_key: packet_key.to_hex(),
        safe_address: None,
        multi_addresses: vec![multiaddr.to_string()],
    }));

    // Should receive update with announcement
    let updated = tokio::time::timeout(Duration::from_millis(500), stream.next())
        .await
        .unwrap()
        .unwrap();

    assert!(updated.errors.is_empty());
    let updated_data = updated.data.into_json().unwrap();
    let multi_addresses = updated_data["accountUpdated"]["multiAddresses"].as_array().unwrap();
    assert_eq!(multi_addresses.len(), 1);
    assert_eq!(multi_addresses[0].as_str().unwrap(), "/ip4/127.0.0.1/tcp/9091");
}

#[tokio::test]
async fn test_account_subscription_receives_multiple_updates() {
    let db = BlokliDb::new_in_memory().await.unwrap();

    // Create account
    let keypair = random_keypair();
    let offchain = random_offchain_keypair();
    let chain_key = keypair.public().to_address();
    let packet_key = offchain.public();

    db.upsert_account(None, 1, chain_key, *packet_key, None, 100, 0, 0)
        .await
        .unwrap();

    let (schema, indexer_state) = create_test_schema(&db);

    let query = r#"
        subscription {
            accountUpdated(keyid: 1) {
                keyid
                safeAddress
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

    // Update 1: Add safe address
    let safe_address1 = Address::from([0x01; 20]);
    db.upsert_account(None, 1, chain_key, *packet_key, Some(safe_address1), 110, 0, 0)
        .await
        .unwrap();

    // Publish event
    indexer_state.publish_event(IndexerEvent::AccountUpdated(Account {
        keyid: 1,
        chain_key: chain_key.to_hex(),
        packet_key: packet_key.to_hex(),
        safe_address: Some(safe_address1.to_hex()),
        multi_addresses: vec![],
    }));

    let update1 = tokio::time::timeout(Duration::from_millis(500), stream.next())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        update1.data.into_json().unwrap()["accountUpdated"]["safeAddress"]
            .as_str()
            .unwrap(),
        safe_address1.to_hex()
    );

    // Update 2: Change safe address
    let safe_address2 = Address::from([0x02; 20]);
    db.upsert_account(None, 1, chain_key, *packet_key, Some(safe_address2), 120, 0, 0)
        .await
        .unwrap();

    // Publish event
    indexer_state.publish_event(IndexerEvent::AccountUpdated(Account {
        keyid: 1,
        chain_key: chain_key.to_hex(),
        packet_key: packet_key.to_hex(),
        safe_address: Some(safe_address2.to_hex()),
        multi_addresses: vec![],
    }));

    let update2 = tokio::time::timeout(Duration::from_millis(500), stream.next())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        update2.data.into_json().unwrap()["accountUpdated"]["safeAddress"]
            .as_str()
            .unwrap(),
        safe_address2.to_hex()
    );
}

#[tokio::test]
async fn test_account_subscription_with_combined_filters() {
    let db = BlokliDb::new_in_memory().await.unwrap();

    // Create two accounts
    let keypair1 = random_keypair();
    let offchain1 = random_offchain_keypair();
    db.upsert_account(
        None,
        1,
        keypair1.public().to_address(),
        *offchain1.public(),
        None,
        100,
        0,
        0,
    )
    .await
    .unwrap();

    let keypair2 = random_keypair();
    let offchain2 = random_offchain_keypair();
    db.upsert_account(
        None,
        2,
        keypair2.public().to_address(),
        *offchain2.public(),
        None,
        101,
        0,
        0,
    )
    .await
    .unwrap();

    let (schema, _indexer_state) = create_test_schema(&db);

    // Subscribe with keyid and chainKey filters (both should match account 1)
    let query = format!(
        r#"
        subscription {{
            accountUpdated(keyid: 1, chainKey: "{}") {{
                keyid
                chainKey
            }}
        }}
    "#,
        keypair1.public().to_address().to_hex()
    );

    let mut stream = schema.execute_stream(&query).boxed();

    // Should only receive account 1
    let result = tokio::time::timeout(Duration::from_secs(2), stream.next())
        .await
        .unwrap()
        .unwrap();

    assert!(result.errors.is_empty());
    let data = result.data.into_json().unwrap();
    assert_eq!(data["accountUpdated"]["keyid"].as_i64().unwrap(), 1);
    assert_eq!(
        data["accountUpdated"]["chainKey"].as_str().unwrap(),
        keypair1.public().to_address().to_hex()
    );
}

#[tokio::test]
async fn test_account_subscription_filter_excludes_non_matching_accounts() {
    let db = BlokliDb::new_in_memory().await.unwrap();

    // Create two accounts
    let keypair1 = random_keypair();
    let offchain1 = random_offchain_keypair();
    db.upsert_account(
        None,
        1,
        keypair1.public().to_address(),
        *offchain1.public(),
        None,
        100,
        0,
        0,
    )
    .await
    .unwrap();

    let keypair2 = random_keypair();
    let offchain2 = random_offchain_keypair();
    db.upsert_account(
        None,
        2,
        keypair2.public().to_address(),
        *offchain2.public(),
        None,
        101,
        0,
        0,
    )
    .await
    .unwrap();

    let (schema, indexer_state) = create_test_schema(&db);

    // Subscribe with keyid filter for account 1 only
    let query = r#"
        subscription {
            accountUpdated(keyid: 1) {
                keyid
            }
        }
    "#;

    let mut stream = schema.execute_stream(query).boxed();

    // Should only receive account 1
    let result1 = tokio::time::timeout(Duration::from_secs(2), stream.next())
        .await
        .unwrap()
        .unwrap();

    let data1 = result1.data.into_json().unwrap();
    assert_eq!(data1["accountUpdated"]["keyid"].as_i64().unwrap(), 1);

    // Publish update for account 2 - should NOT appear in subscription since filter is keyid: 1
    let safe_address2 = Address::from([0x02; 20]);
    indexer_state.publish_event(IndexerEvent::AccountUpdated(Account {
        keyid: 2,
        chain_key: keypair2.public().to_address().to_hex(),
        packet_key: offchain2.public().to_hex(),
        safe_address: Some(safe_address2.to_hex()),
        multi_addresses: vec![],
    }));

    // Then publish update for account 1 to verify it still comes through
    let safe_address1 = Address::from([0x01; 20]);
    indexer_state.publish_event(IndexerEvent::AccountUpdated(Account {
        keyid: 1,
        chain_key: keypair1.public().to_address().to_hex(),
        packet_key: offchain1.public().to_hex(),
        safe_address: Some(safe_address1.to_hex()),
        multi_addresses: vec![],
    }));

    // Should receive account 1 update, not account 2
    let result2 = tokio::time::timeout(Duration::from_millis(500), stream.next())
        .await
        .unwrap()
        .unwrap();

    let data2 = result2.data.into_json().unwrap();
    assert_eq!(
        data2["accountUpdated"]["keyid"].as_i64().unwrap(),
        1,
        "Should only emit account 1, never account 2"
    );
}

#[tokio::test]
async fn test_account_subscription_handles_empty_database() {
    let db = BlokliDb::new_in_memory().await.unwrap();

    let (schema, _indexer_state) = create_test_schema(&db);

    let query = r#"
        subscription {
            accountUpdated {
                keyid
            }
        }
    "#;

    let mut stream = schema.execute_stream(query).boxed();

    // Should not receive any accounts within timeout (empty database)
    let no_result = tokio::time::timeout(Duration::from_millis(1500), stream.next()).await;
    assert!(no_result.is_err(), "Should not emit accounts from empty database");
}

#[tokio::test]
async fn test_account_subscription_handles_account_with_multiple_announcements() {
    let db = BlokliDb::new_in_memory().await.unwrap();

    // Create account
    let keypair = random_keypair();
    let offchain = random_offchain_keypair();
    let chain_key = keypair.public().to_address();
    let packet_key = offchain.public();

    db.upsert_account(None, 1, chain_key, *packet_key, None, 100, 0, 0)
        .await
        .unwrap();

    // Add multiple announcements
    let multiaddr1: Multiaddr = "/ip4/127.0.0.1/tcp/9091".parse().unwrap();
    let multiaddr2: Multiaddr = "/ip4/127.0.0.1/tcp/9092".parse().unwrap();
    db.insert_announcement(None, chain_key, multiaddr1, 101).await.unwrap();
    db.insert_announcement(None, chain_key, multiaddr2, 102).await.unwrap();

    let (schema, _indexer_state) = create_test_schema(&db);

    let query = r#"
        subscription {
            accountUpdated(keyid: 1) {
                keyid
                multiAddresses
            }
        }
    "#;

    let mut stream = schema.execute_stream(query).boxed();

    // Should receive account with multiple announcements
    let result = tokio::time::timeout(Duration::from_secs(2), stream.next())
        .await
        .unwrap()
        .unwrap();

    assert!(result.errors.is_empty());
    let data = result.data.into_json().unwrap();
    let multi_addresses = data["accountUpdated"]["multiAddresses"].as_array().unwrap();
    assert_eq!(multi_addresses.len(), 2);
}

#[tokio::test]
async fn test_account_subscription_multiple_concurrent_subscribers() {
    let db = BlokliDb::new_in_memory().await.unwrap();

    // Create account
    let keypair = random_keypair();
    let offchain = random_offchain_keypair();
    let chain_key = keypair.public().to_address();
    let packet_key = offchain.public();

    db.upsert_account(None, 1, chain_key, *packet_key, None, 100, 0, 0)
        .await
        .unwrap();

    let (schema, indexer_state) = create_test_schema(&db);

    let query = r#"
        subscription {
            accountUpdated(keyid: 1) {
                keyid
                safeAddress
            }
        }
    "#;

    // Create two concurrent subscribers
    let mut stream1 = schema.execute_stream(query).boxed();
    let mut stream2 = schema.execute_stream(query).boxed();

    // Both should receive initial account
    let result1 = tokio::time::timeout(Duration::from_secs(2), stream1.next())
        .await
        .unwrap()
        .unwrap();
    assert!(result1.errors.is_empty());

    let result2 = tokio::time::timeout(Duration::from_secs(2), stream2.next())
        .await
        .unwrap()
        .unwrap();
    assert!(result2.errors.is_empty());

    // Update account in database
    let safe_address = Address::from([0x01; 20]);
    db.upsert_account(None, 1, chain_key, *packet_key, Some(safe_address), 110, 0, 0)
        .await
        .unwrap();

    // Publish AccountUpdated event (this is what the indexer would do)
    indexer_state.publish_event(IndexerEvent::AccountUpdated(Account {
        keyid: 1,
        chain_key: chain_key.to_hex(),
        packet_key: packet_key.to_hex(),
        safe_address: Some(safe_address.to_hex()),
        multi_addresses: vec![],
    }));

    // Both should receive the update
    let update1 = tokio::time::timeout(Duration::from_millis(500), stream1.next())
        .await
        .unwrap()
        .unwrap();
    let data1 = update1.data.into_json().unwrap();
    assert_eq!(
        data1["accountUpdated"]["safeAddress"].as_str().unwrap(),
        safe_address.to_hex()
    );

    let update2 = tokio::time::timeout(Duration::from_millis(500), stream2.next())
        .await
        .unwrap()
        .unwrap();
    let data2 = update2.data.into_json().unwrap();
    assert_eq!(
        data2["accountUpdated"]["safeAddress"].as_str().unwrap(),
        safe_address.to_hex()
    );
}
