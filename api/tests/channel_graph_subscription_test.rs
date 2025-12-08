//! Integration tests for openedChannelGraphUpdated subscription
//!
//! These tests verify the channel graph subscription functionality:
//! - Initial graph emission with all open channels
//! - Graph updates when channels are opened
//! - Graph updates when channels are closed
//! - Account information is included for all channel participants
//! - Only OPEN channels are included in the graph

use std::{str::FromStr, sync::Arc, time::Duration};

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
use blokli_db::{
    BlokliDbGeneralModelOperations, TargetDb, accounts::BlokliDbAccountOperations, channels::BlokliDbChannelOperations,
    db::BlokliDb,
};
use futures::StreamExt;
use hopr_crypto_types::prelude::{ChainKeypair, Keypair, OffchainKeypair};
use hopr_internal_types::channels::{ChannelEntry, ChannelStatus};
use hopr_primitive_types::prelude::HoprBalance;

/// Helper to generate random keypair for testing
fn random_keypair() -> ChainKeypair {
    ChainKeypair::random()
}

/// Helper to generate random offchain keypair
fn random_offchain_keypair() -> OffchainKeypair {
    OffchainKeypair::random()
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
async fn test_opened_channel_graph_subscription_emits_initial_graph() {
    let db = BlokliDb::new_in_memory().await.unwrap();

    // Create accounts
    let keypair1 = random_keypair();
    let keypair2 = random_keypair();
    let offchain1 = random_offchain_keypair();
    let offchain2 = random_offchain_keypair();
    let addr1 = keypair1.public().to_address();
    let addr2 = keypair2.public().to_address();

    db.upsert_account(None, 1, addr1, *offchain1.public(), None, 100, 0, 0)
        .await
        .unwrap();
    db.upsert_account(None, 2, addr2, *offchain2.public(), None, 101, 0, 0)
        .await
        .unwrap();

    // Create an open channel
    let balance = HoprBalance::from_str("1000 wxHOPR").unwrap();
    let channel = ChannelEntry::new(addr1, addr2, balance, 0, ChannelStatus::Open, 1);
    db.upsert_channel(None, channel, 100, 0, 0).await.unwrap();

    // Create GraphQL schema
    let schema = create_test_schema(&db);

    // Execute subscription query
    let query = r#"
        subscription {
            openedChannelGraphUpdated {
                channels {
                    source
                    destination
                    status
                }
                accounts {
                    keyid
                    chainKey
                    packetKey
                }
            }
        }
    "#;

    let mut stream = schema.execute_stream(query).boxed();

    // Should receive initial graph within timeout
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
    let graph = &data["openedChannelGraphUpdated"];

    // Verify channels
    let channels = graph["channels"].as_array().unwrap();
    assert_eq!(channels.len(), 1);
    assert_eq!(channels[0]["source"].as_i64().unwrap(), 1);
    assert_eq!(channels[0]["destination"].as_i64().unwrap(), 2);
    assert_eq!(channels[0]["status"].as_str().unwrap(), "OPEN");

    // Verify accounts
    let accounts = graph["accounts"].as_array().unwrap();
    assert_eq!(accounts.len(), 2);

    // Collect account keyids
    let mut keyids: Vec<i64> = accounts.iter().map(|a| a["keyid"].as_i64().unwrap()).collect();
    keyids.sort();
    assert_eq!(keyids, vec![1, 2]);
}

#[tokio::test]
async fn test_opened_channel_graph_subscription_includes_multiple_channels() {
    let db = BlokliDb::new_in_memory().await.unwrap();

    // Create 3 accounts
    let keypair1 = random_keypair();
    let keypair2 = random_keypair();
    let keypair3 = random_keypair();
    let offchain1 = random_offchain_keypair();
    let offchain2 = random_offchain_keypair();
    let offchain3 = random_offchain_keypair();
    let addr1 = keypair1.public().to_address();
    let addr2 = keypair2.public().to_address();
    let addr3 = keypair3.public().to_address();

    db.upsert_account(None, 1, addr1, *offchain1.public(), None, 100, 0, 0)
        .await
        .unwrap();
    db.upsert_account(None, 2, addr2, *offchain2.public(), None, 101, 0, 0)
        .await
        .unwrap();
    db.upsert_account(None, 3, addr3, *offchain3.public(), None, 102, 0, 0)
        .await
        .unwrap();

    // Create two open channels
    let balance1 = HoprBalance::from_str("1000 wxHOPR").unwrap();
    let channel1 = ChannelEntry::new(addr1, addr2, balance1, 0, ChannelStatus::Open, 1);
    db.upsert_channel(None, channel1, 100, 0, 0).await.unwrap();

    let balance2 = HoprBalance::from_str("2000 wxHOPR").unwrap();
    let channel2 = ChannelEntry::new(addr2, addr3, balance2, 0, ChannelStatus::Open, 1);
    db.upsert_channel(None, channel2, 101, 0, 0).await.unwrap();

    let schema = create_test_schema(&db);

    let query = r#"
        subscription {
            openedChannelGraphUpdated {
                channels {
                    source
                    destination
                }
                accounts {
                    keyid
                }
            }
        }
    "#;

    let mut stream = schema.execute_stream(query).boxed();

    let result = tokio::time::timeout(Duration::from_secs(2), stream.next())
        .await
        .unwrap()
        .unwrap();

    assert!(result.errors.is_empty());
    let data = result.data.into_json().unwrap();
    let graph = &data["openedChannelGraphUpdated"];

    // Should have 2 channels
    let channels = graph["channels"].as_array().unwrap();
    assert_eq!(channels.len(), 2);

    // Should have 3 accounts (all participants)
    let accounts = graph["accounts"].as_array().unwrap();
    assert_eq!(accounts.len(), 3);
}

#[tokio::test]
async fn test_opened_channel_graph_subscription_excludes_closed_channels() {
    let db = BlokliDb::new_in_memory().await.unwrap();

    // Create accounts
    let keypair1 = random_keypair();
    let keypair2 = random_keypair();
    let keypair3 = random_keypair();
    let offchain1 = random_offchain_keypair();
    let offchain2 = random_offchain_keypair();
    let offchain3 = random_offchain_keypair();
    let addr1 = keypair1.public().to_address();
    let addr2 = keypair2.public().to_address();
    let addr3 = keypair3.public().to_address();

    db.upsert_account(None, 1, addr1, *offchain1.public(), None, 100, 0, 0)
        .await
        .unwrap();
    db.upsert_account(None, 2, addr2, *offchain2.public(), None, 101, 0, 0)
        .await
        .unwrap();
    db.upsert_account(None, 3, addr3, *offchain3.public(), None, 102, 0, 0)
        .await
        .unwrap();

    // Create one OPEN channel and one CLOSED channel
    let balance1 = HoprBalance::from_str("1000 wxHOPR").unwrap();
    let open_channel = ChannelEntry::new(addr1, addr2, balance1, 0, ChannelStatus::Open, 1);
    db.upsert_channel(None, open_channel, 100, 0, 0).await.unwrap();

    let balance2 = HoprBalance::from_str("2000 wxHOPR").unwrap();
    let closed_channel = ChannelEntry::new(addr2, addr3, balance2, 0, ChannelStatus::Closed, 1);
    db.upsert_channel(None, closed_channel, 101, 0, 0).await.unwrap();

    let schema = create_test_schema(&db);

    let query = r#"
        subscription {
            openedChannelGraphUpdated {
                channels {
                    source
                    destination
                    status
                }
                accounts {
                    keyid
                }
            }
        }
    "#;

    let mut stream = schema.execute_stream(query).boxed();

    let result = tokio::time::timeout(Duration::from_secs(2), stream.next())
        .await
        .unwrap()
        .unwrap();

    assert!(result.errors.is_empty());
    let data = result.data.into_json().unwrap();
    let graph = &data["openedChannelGraphUpdated"];

    // Should only have 1 open channel
    let channels = graph["channels"].as_array().unwrap();
    assert_eq!(channels.len(), 1);
    assert_eq!(channels[0]["source"].as_i64().unwrap(), 1);
    assert_eq!(channels[0]["destination"].as_i64().unwrap(), 2);
    assert_eq!(channels[0]["status"].as_str().unwrap(), "OPEN");

    // Should only have 2 accounts (those participating in open channels)
    let accounts = graph["accounts"].as_array().unwrap();
    assert_eq!(accounts.len(), 2);
}

#[tokio::test]
async fn test_opened_channel_graph_subscription_receives_new_channel_update() {
    let db = BlokliDb::new_in_memory().await.unwrap();

    // Create accounts
    let keypair1 = random_keypair();
    let keypair2 = random_keypair();
    let keypair3 = random_keypair();
    let offchain1 = random_offchain_keypair();
    let offchain2 = random_offchain_keypair();
    let offchain3 = random_offchain_keypair();
    let addr1 = keypair1.public().to_address();
    let addr2 = keypair2.public().to_address();
    let addr3 = keypair3.public().to_address();

    db.upsert_account(None, 1, addr1, *offchain1.public(), None, 100, 0, 0)
        .await
        .unwrap();
    db.upsert_account(None, 2, addr2, *offchain2.public(), None, 101, 0, 0)
        .await
        .unwrap();
    db.upsert_account(None, 3, addr3, *offchain3.public(), None, 102, 0, 0)
        .await
        .unwrap();

    // Create initial channel
    let balance1 = HoprBalance::from_str("1000 wxHOPR").unwrap();
    let channel1 = ChannelEntry::new(addr1, addr2, balance1, 0, ChannelStatus::Open, 1);
    db.upsert_channel(None, channel1, 100, 0, 0).await.unwrap();

    let schema = create_test_schema(&db);

    let query = r#"
        subscription {
            openedChannelGraphUpdated {
                channels {
                    source
                    destination
                }
            }
        }
    "#;

    let mut stream = schema.execute_stream(query).boxed();

    // Receive initial graph with 1 channel
    let initial = tokio::time::timeout(Duration::from_secs(2), stream.next())
        .await
        .unwrap()
        .unwrap();

    assert!(initial.errors.is_empty());
    let initial_data = initial.data.into_json().unwrap();
    assert_eq!(
        initial_data["openedChannelGraphUpdated"]["channels"]
            .as_array()
            .unwrap()
            .len(),
        1
    );

    // Add a new open channel
    let balance2 = HoprBalance::from_str("2000 wxHOPR").unwrap();
    let channel2 = ChannelEntry::new(addr2, addr3, balance2, 0, ChannelStatus::Open, 1);
    db.upsert_channel(None, channel2, 110, 0, 0).await.unwrap();

    // Should receive updated graph with 2 channels
    let updated = tokio::time::timeout(Duration::from_secs(3), stream.next())
        .await
        .unwrap()
        .unwrap();

    assert!(updated.errors.is_empty());
    let updated_data = updated.data.into_json().unwrap();
    let channels = updated_data["openedChannelGraphUpdated"]["channels"]
        .as_array()
        .unwrap();
    assert_eq!(channels.len(), 2);
}

#[tokio::test]
async fn test_opened_channel_graph_subscription_receives_channel_closure_update() {
    let db = BlokliDb::new_in_memory().await.unwrap();

    // Create accounts
    let keypair1 = random_keypair();
    let keypair2 = random_keypair();
    let offchain1 = random_offchain_keypair();
    let offchain2 = random_offchain_keypair();
    let addr1 = keypair1.public().to_address();
    let addr2 = keypair2.public().to_address();

    db.upsert_account(None, 1, addr1, *offchain1.public(), None, 100, 0, 0)
        .await
        .unwrap();
    db.upsert_account(None, 2, addr2, *offchain2.public(), None, 101, 0, 0)
        .await
        .unwrap();

    // Create open channel
    let balance = HoprBalance::from_str("1000 wxHOPR").unwrap();
    let channel = ChannelEntry::new(addr1, addr2, balance, 0, ChannelStatus::Open, 1);
    db.upsert_channel(None, channel, 100, 0, 0).await.unwrap();

    let schema = create_test_schema(&db);

    let query = r#"
        subscription {
            openedChannelGraphUpdated {
                channels {
                    source
                    destination
                }
            }
        }
    "#;

    let mut stream = schema.execute_stream(query).boxed();

    // Receive initial graph with 1 channel
    let initial = tokio::time::timeout(Duration::from_secs(2), stream.next())
        .await
        .unwrap()
        .unwrap();

    assert!(initial.errors.is_empty());
    let initial_data = initial.data.into_json().unwrap();
    assert_eq!(
        initial_data["openedChannelGraphUpdated"]["channels"]
            .as_array()
            .unwrap()
            .len(),
        1
    );

    // Close the channel
    let closed_channel = ChannelEntry::new(addr1, addr2, balance, 0, ChannelStatus::Closed, 1);
    db.upsert_channel(None, closed_channel, 110, 0, 0).await.unwrap();

    // Should receive updated graph with 0 channels
    let updated = tokio::time::timeout(Duration::from_secs(3), stream.next())
        .await
        .unwrap()
        .unwrap();

    assert!(updated.errors.is_empty());
    let updated_data = updated.data.into_json().unwrap();
    let channels = updated_data["openedChannelGraphUpdated"]["channels"]
        .as_array()
        .unwrap();
    assert_eq!(channels.len(), 0);
}

#[tokio::test]
async fn test_opened_channel_graph_subscription_handles_empty_database() {
    let db = BlokliDb::new_in_memory().await.unwrap();

    let schema = create_test_schema(&db);

    let query = r#"
        subscription {
            openedChannelGraphUpdated {
                channels {
                    source
                }
            }
        }
    "#;

    let mut stream = schema.execute_stream(query).boxed();

    // Should receive graph with empty channels array
    let result = tokio::time::timeout(Duration::from_secs(2), stream.next())
        .await
        .unwrap()
        .unwrap();

    assert!(result.errors.is_empty());
    let data = result.data.into_json().unwrap();
    let channels = data["openedChannelGraphUpdated"]["channels"].as_array().unwrap();
    assert_eq!(channels.len(), 0);
}

#[tokio::test]
async fn test_opened_channel_graph_subscription_includes_channel_balance() {
    let db = BlokliDb::new_in_memory().await.unwrap();

    // Create accounts
    let keypair1 = random_keypair();
    let keypair2 = random_keypair();
    let offchain1 = random_offchain_keypair();
    let offchain2 = random_offchain_keypair();
    let addr1 = keypair1.public().to_address();
    let addr2 = keypair2.public().to_address();

    db.upsert_account(None, 1, addr1, *offchain1.public(), None, 100, 0, 0)
        .await
        .unwrap();
    db.upsert_account(None, 2, addr2, *offchain2.public(), None, 101, 0, 0)
        .await
        .unwrap();

    // Create channel with specific balance
    let balance = HoprBalance::from_str("1234 wxHOPR").unwrap();
    let channel = ChannelEntry::new(addr1, addr2, balance, 0, ChannelStatus::Open, 1);
    db.upsert_channel(None, channel, 100, 0, 0).await.unwrap();

    let schema = create_test_schema(&db);

    let query = r#"
        subscription {
            openedChannelGraphUpdated {
                channels {
                    balance
                }
            }
        }
    "#;

    let mut stream = schema.execute_stream(query).boxed();

    let result = tokio::time::timeout(Duration::from_secs(2), stream.next())
        .await
        .unwrap()
        .unwrap();

    assert!(result.errors.is_empty());
    let data = result.data.into_json().unwrap();
    let channels = data["openedChannelGraphUpdated"]["channels"].as_array().unwrap();
    assert_eq!(channels.len(), 1);
    // Balance is in wei (10^18 per HOPR)
    assert_eq!(channels[0]["balance"].as_str().unwrap(), "1234000000000000000000");
}

#[tokio::test]
async fn test_opened_channel_graph_subscription_balance_update() {
    let db = BlokliDb::new_in_memory().await.unwrap();

    // Create accounts
    let keypair1 = random_keypair();
    let keypair2 = random_keypair();
    let offchain1 = random_offchain_keypair();
    let offchain2 = random_offchain_keypair();
    let addr1 = keypair1.public().to_address();
    let addr2 = keypair2.public().to_address();

    db.upsert_account(None, 1, addr1, *offchain1.public(), None, 100, 0, 0)
        .await
        .unwrap();
    db.upsert_account(None, 2, addr2, *offchain2.public(), None, 101, 0, 0)
        .await
        .unwrap();

    // Create channel
    let initial_balance = HoprBalance::from_str("1000 wxHOPR").unwrap();
    let channel = ChannelEntry::new(addr1, addr2, initial_balance, 0, ChannelStatus::Open, 1);
    db.upsert_channel(None, channel, 100, 0, 0).await.unwrap();

    let schema = create_test_schema(&db);

    let query = r#"
        subscription {
            openedChannelGraphUpdated {
                channels {
                    balance
                }
            }
        }
    "#;

    let mut stream = schema.execute_stream(query).boxed();

    // Receive initial balance
    let initial = tokio::time::timeout(Duration::from_secs(2), stream.next())
        .await
        .unwrap()
        .unwrap();

    assert!(initial.errors.is_empty());
    let initial_data = initial.data.into_json().unwrap();
    let initial_balance_str = initial_data["openedChannelGraphUpdated"]["channels"][0]["balance"]
        .as_str()
        .unwrap();
    assert_eq!(initial_balance_str, "1000000000000000000000");

    // Update channel balance
    let updated_balance = HoprBalance::from_str("2000 wxHOPR").unwrap();
    let updated_channel = ChannelEntry::new(addr1, addr2, updated_balance, 0, ChannelStatus::Open, 1);
    db.upsert_channel(None, updated_channel, 110, 0, 0).await.unwrap();

    // Should receive updated balance
    let updated = tokio::time::timeout(Duration::from_secs(3), stream.next())
        .await
        .unwrap()
        .unwrap();

    assert!(updated.errors.is_empty());
    let updated_data = updated.data.into_json().unwrap();
    let updated_balance_str = updated_data["openedChannelGraphUpdated"]["channels"][0]["balance"]
        .as_str()
        .unwrap();
    assert_eq!(updated_balance_str, "2000000000000000000000");
}
