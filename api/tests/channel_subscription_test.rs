//! Integration tests for channelUpdated subscription
//!
//! These tests verify the real-time channel update subscription functionality:
//! - Initial channel emission on subscription
//! - Filter combinations (sourceKeyId, destinationKeyId, concreteChannelId, status)
//! - Channel state transitions (OPEN → PENDINGTOCLOSE → CLOSED)
//! - Balance updates
//! - Multiple concurrent subscribers

use std::{
    str::FromStr,
    sync::Arc,
    time::{Duration, SystemTime},
};

use async_graphql::Schema;
use blokli_api::{mutation::MutationRoot, query::QueryRoot, schema::build_schema, subscription::SubscriptionRoot};
use blokli_api_types::{Account, Channel, ChannelStatus as ApiChannelStatus, TokenValueString, UInt64};
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
use blokli_db::{
    BlokliDbGeneralModelOperations, TargetDb, accounts::BlokliDbAccountOperations, channels::BlokliDbChannelOperations,
    db::BlokliDb,
};
use blokli_db_entity::{
    chain_info, channel, channel_state, conversions::account_aggregation::fetch_accounts_by_keyids,
};
use chrono::Utc;
use futures::StreamExt;
use hopr_bindings::exports::alloy::{rpc::client::ClientBuilder, transports::http::ReqwestTransport};
use hopr_crypto_types::prelude::{ChainKeypair, Keypair, OffchainKeypair};
use hopr_internal_types::channels::{ChannelEntry, ChannelStatus};
use hopr_primitive_types::{
    prelude::HoprBalance,
    traits::{IntoEndian, ToHex},
};
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter, QueryOrder, Set, sea_query::OnConflict};

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
    create_test_schema_with_state(db, IndexerState::new(10, 100))
}

/// Create a minimal GraphQL schema for testing subscriptions with a specific IndexerState
fn create_test_schema_with_state(
    db: &BlokliDb,
    indexer_state: IndexerState,
) -> Schema<QueryRoot, MutationRoot, SubscriptionRoot> {
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
        1,
        8, // Test finality value
        indexer_state,
        transaction_executor,
        transaction_store,
        rpc_ops,
    )
}

/// Helper to update chain_info watermark for tests
async fn update_watermark(db: &BlokliDb, block: i64, tx_index: i64, log_index: i64) {
    let chain_info_model = chain_info::ActiveModel {
        id: Set(1),
        last_indexed_block: Set(block),
        last_indexed_tx_index: Set(Some(tx_index)),
        last_indexed_log_index: Set(Some(log_index)),
        ticket_price: Set(None),
        min_incoming_ticket_win_prob: Set(0.0),
        channels_dst: Set(None),
        ledger_dst: Set(None),
        safe_registry_dst: Set(None),
        channel_closure_grace_period: Set(None),
        key_binding_fee: Set(None),
    };

    chain_info::Entity::insert(chain_info_model)
        .on_conflict(
            OnConflict::column(chain_info::Column::Id)
                .update_columns([
                    chain_info::Column::LastIndexedBlock,
                    chain_info::Column::LastIndexedTxIndex,
                    chain_info::Column::LastIndexedLogIndex,
                ])
                .to_owned(),
        )
        .exec(db.conn(TargetDb::Index))
        .await
        .unwrap();
}

/// Helper to manually create a ChannelUpdate event for testing Phase 2 subscriptions
async fn create_channel_update_event(
    db: &BlokliDb,
    concrete_channel_id: &str,
) -> Result<blokli_api_types::ChannelUpdate, Box<dyn std::error::Error>> {
    // Find the channel
    let channel_model = channel::Entity::find()
        .filter(
            channel::Column::ConcreteChannelId
                .eq(concrete_channel_id.strip_prefix("0x").unwrap_or(concrete_channel_id)),
        )
        .one(db.conn(TargetDb::Index))
        .await?
        .ok_or("Channel not found")?;

    // Find the latest channel state
    let state = channel_state::Entity::find()
        .filter(channel_state::Column::ChannelId.eq(channel_model.id))
        .order_by_desc(channel_state::Column::PublishedBlock)
        .order_by_desc(channel_state::Column::PublishedTxIndex)
        .order_by_desc(channel_state::Column::PublishedLogIndex)
        .one(db.conn(TargetDb::Index))
        .await?
        .ok_or("Channel state not found")?;

    // Fetch accounts
    let accounts = fetch_accounts_by_keyids(
        db.conn(TargetDb::Index),
        vec![channel_model.source, channel_model.destination],
    )
    .await?;

    let source_account = accounts
        .iter()
        .find(|a| a.keyid == channel_model.source)
        .ok_or("Source account not found")?;
    let dest_account = accounts
        .iter()
        .find(|a| a.keyid == channel_model.destination)
        .ok_or("Destination account not found")?;

    // Convert to GraphQL types
    let channel_gql = Channel {
        concrete_channel_id: channel_model.concrete_channel_id.clone(),
        source: channel_model.source,
        destination: channel_model.destination,
        balance: TokenValueString(HoprBalance::from_be_bytes(&state.balance).to_string()),
        status: ApiChannelStatus::from(state.status),
        epoch: i32::try_from(state.epoch)?,
        ticket_index: UInt64(u64::try_from(state.ticket_index)?),
        closure_time: state.closure_time.map(|time| time.with_timezone(&Utc)),
    };

    let source_gql = Account {
        keyid: source_account.keyid,
        chain_key: source_account.chain_key.clone(),
        packet_key: source_account.packet_key.clone(),
        safe_address: source_account.safe_address.clone(),
        multi_addresses: source_account.multi_addresses.clone(),
    };

    let dest_gql = Account {
        keyid: dest_account.keyid,
        chain_key: dest_account.chain_key.clone(),
        packet_key: dest_account.packet_key.clone(),
        safe_address: dest_account.safe_address.clone(),
        multi_addresses: dest_account.multi_addresses.clone(),
    };

    Ok(blokli_api_types::ChannelUpdate {
        channel: channel_gql,
        source: source_gql,
        destination: dest_gql,
    })
}

#[tokio::test]
async fn test_channel_subscription_emits_initial_channel_with_source_filter() {
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
    let balance = HoprBalance::from_str("1000 wxHOPR").unwrap();
    let channel = ChannelEntry::new(addr1, addr2, balance, 0, ChannelStatus::Open, 1);
    db.upsert_channel(None, channel, 100, 0, 0).await.unwrap();

    // Update watermark to include all test data
    update_watermark(&db, 1000, 0, 0).await;

    let schema = create_test_schema(&db);

    // Execute subscription query with sourceKeyId filter
    let query = r#"
        subscription {
            channelUpdated(sourceKeyId: 1) {
                source
                destination
                balance
                status
                epoch
                ticketIndex
            }
        }
    "#;

    let mut stream = schema.execute_stream(query).boxed();

    // Should receive initial channel within timeout
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
    let channel_data = &data["channelUpdated"];

    // Verify channel fields
    assert_eq!(channel_data["source"].as_i64().unwrap(), 1);
    assert_eq!(channel_data["destination"].as_i64().unwrap(), 2);
    assert_eq!(channel_data["status"].as_str().unwrap(), "OPEN");
    assert_eq!(channel_data["epoch"].as_i64().unwrap(), 1);
}

#[tokio::test]
async fn test_channel_subscription_emits_initial_channel_with_destination_filter() {
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
    let balance = HoprBalance::from_str("500 wxHOPR").unwrap();
    let channel = ChannelEntry::new(addr1, addr2, balance, 0, ChannelStatus::Open, 1);
    db.upsert_channel(None, channel, 100, 0, 0).await.unwrap();

    // Update watermark to include all test data
    update_watermark(&db, 1000, 0, 0).await;

    let schema = create_test_schema(&db);

    // Subscribe with destinationKeyId filter
    let query = r#"
        subscription {
            channelUpdated(destinationKeyId: 2) {
                source
                destination
                balance
                status
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
    let channel_data = &data["channelUpdated"];

    assert_eq!(channel_data["source"].as_i64().unwrap(), 1);
    assert_eq!(channel_data["destination"].as_i64().unwrap(), 2);
    assert_eq!(channel_data["status"].as_str().unwrap(), "OPEN");
}

#[tokio::test]
async fn test_channel_subscription_emits_initial_channel_with_concrete_channel_id_filter() {
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
    let balance = HoprBalance::from_str("500 wxHOPR").unwrap();
    let channel = ChannelEntry::new(addr1, addr2, balance, 0, ChannelStatus::Open, 1);
    let channel_id = channel.get_id().to_hex();

    db.upsert_channel(None, channel, 100, 0, 0).await.unwrap();

    // Update watermark to include all test data
    update_watermark(&db, 1000, 0, 0).await;

    let schema = create_test_schema(&db);

    // Subscribe with concreteChannelId filter
    let query = format!(
        r#"
        subscription {{
            channelUpdated(concreteChannelId: "{}") {{
                concreteChannelId
                source
                destination
                status
            }}
        }}
    "#,
        channel_id
    );

    let mut stream = schema.execute_stream(&query).boxed();

    let result = tokio::time::timeout(Duration::from_secs(2), stream.next())
        .await
        .unwrap()
        .unwrap();

    assert!(result.errors.is_empty());
    let data = result.data.into_json().unwrap();
    let channel_data = &data["channelUpdated"];

    assert!(channel_id.contains(channel_data["concreteChannelId"].as_str().unwrap()));
    assert_eq!(channel_data["status"].as_str().unwrap(), "OPEN");
}

#[tokio::test]
async fn test_channel_subscription_emits_initial_channel_with_status_filter() {
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

    // Create OPEN channel
    let balance = HoprBalance::from_str("1000 wxHOPR").unwrap();
    let channel = ChannelEntry::new(addr1, addr2, balance, 0, ChannelStatus::Open, 1);
    db.upsert_channel(None, channel, 100, 0, 0).await.unwrap();

    // Update watermark to include all test data
    update_watermark(&db, 1000, 0, 0).await;

    let schema = create_test_schema(&db);

    // Subscribe with status filter for OPEN channels
    let query = r#"
        subscription {
            channelUpdated(sourceKeyId: 1, status: OPEN) {
                source
                destination
                status
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
    let channel_data = &data["channelUpdated"];

    assert_eq!(channel_data["status"].as_str().unwrap(), "OPEN");
}

#[tokio::test]
async fn test_channel_subscription_without_filters_emits_all_channels() {
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

    // Create two channels
    let balance1 = HoprBalance::from_str("1000 wxHOPR").unwrap();
    let channel1 = ChannelEntry::new(addr1, addr2, balance1, 0, ChannelStatus::Open, 1);
    db.upsert_channel(None, channel1, 100, 0, 0).await.unwrap();

    let balance2 = HoprBalance::from_str("2000 wxHOPR").unwrap();
    let channel2 = ChannelEntry::new(addr2, addr3, balance2, 0, ChannelStatus::Open, 1);
    db.upsert_channel(None, channel2, 101, 0, 0).await.unwrap();

    // Update watermark to include all test data
    update_watermark(&db, 1000, 0, 0).await;

    let schema = create_test_schema(&db);

    // Subscribe without filters
    let query = r#"
        subscription {
            channelUpdated {
                source
                destination
            }
        }
    "#;

    let mut stream = schema.execute_stream(query).boxed();

    // Should receive both channels
    let result1 = tokio::time::timeout(Duration::from_secs(2), stream.next())
        .await
        .unwrap()
        .unwrap();
    let data1 = result1.data.into_json().unwrap();

    let result2 = tokio::time::timeout(Duration::from_secs(2), stream.next())
        .await
        .unwrap()
        .unwrap();
    let data2 = result2.data.into_json().unwrap();

    // Collect source/destination pairs
    let mut pairs = vec![
        (
            data1["channelUpdated"]["source"].as_i64().unwrap(),
            data1["channelUpdated"]["destination"].as_i64().unwrap(),
        ),
        (
            data2["channelUpdated"]["source"].as_i64().unwrap(),
            data2["channelUpdated"]["destination"].as_i64().unwrap(),
        ),
    ];
    pairs.sort();

    // Should have both channel pairs
    assert_eq!(pairs, vec![(1, 2), (2, 3)]);
}

#[tokio::test]
async fn test_channel_subscription_receives_balance_update() {
    let db = BlokliDb::new_in_memory().await.unwrap();
    let indexer_state = IndexerState::new(10, 100);

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

    // Create channel with initial balance
    let initial_balance = HoprBalance::from_str("1000 wxHOPR").unwrap();
    let channel = ChannelEntry::new(addr1, addr2, initial_balance, 0, ChannelStatus::Open, 1);
    let channel_id = channel.get_id().to_hex();
    db.upsert_channel(None, channel, 100, 0, 0).await.unwrap();

    // Update watermark to include all test data
    update_watermark(&db, 1000, 0, 0).await;

    let schema = create_test_schema_with_state(&db, indexer_state.clone());

    let query = r#"
        subscription {
            channelUpdated(sourceKeyId: 1) {
                source
                destination
                balance
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
    // Balance is in wei (10^18 per HOPR), so 1000 HOPR = 1000 * 10^18 wei
    assert_eq!(
        initial_data["channelUpdated"]["balance"].as_str().unwrap(),
        initial_balance.to_string()
    );

    // Update channel balance
    let updated_balance = HoprBalance::from_str("2000 wxHOPR").unwrap();
    let updated_channel = ChannelEntry::new(addr1, addr2, updated_balance, 0, ChannelStatus::Open, 1);
    db.upsert_channel(None, updated_channel, 110, 0, 0).await.unwrap();

    // Publish event to trigger Phase 2 update
    let channel_update = create_channel_update_event(&db, &channel_id).await.unwrap();
    indexer_state.publish_event(IndexerEvent::ChannelUpdated(Box::new(channel_update)));

    // Should receive update with new balance
    let updated = tokio::time::timeout(Duration::from_secs(3), stream.next())
        .await
        .unwrap()
        .unwrap();

    assert!(updated.errors.is_empty());
    let updated_data = updated.data.into_json().unwrap();
    // Balance is in wei (10^18 per HOPR), so 2000 HOPR = 2000 * 10^18 wei
    assert_eq!(
        updated_data["channelUpdated"]["balance"].as_str().unwrap(),
        updated_balance.to_string()
    );
}

#[tokio::test]
async fn test_channel_subscription_receives_status_transition_open_to_pending() {
    let db = BlokliDb::new_in_memory().await.unwrap();
    let indexer_state = IndexerState::new(10, 100);

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

    // Create OPEN channel
    let balance = HoprBalance::from_str("1000 wxHOPR").unwrap();
    let channel = ChannelEntry::new(addr1, addr2, balance, 0, ChannelStatus::Open, 1);
    let channel_id = channel.get_id().to_hex();
    db.upsert_channel(None, channel, 100, 0, 0).await.unwrap();

    // Update watermark to include all test data
    update_watermark(&db, 1000, 0, 0).await;

    let schema = create_test_schema_with_state(&db, indexer_state.clone());

    let query = r#"
        subscription {
            channelUpdated(sourceKeyId: 1) {
                status
                closureTime
            }
        }
    "#;

    let mut stream = schema.execute_stream(query).boxed();

    // Receive initial OPEN state
    let initial = tokio::time::timeout(Duration::from_secs(2), stream.next())
        .await
        .unwrap()
        .unwrap();

    assert!(initial.errors.is_empty());
    let initial_data = initial.data.into_json().unwrap();
    assert_eq!(initial_data["channelUpdated"]["status"].as_str().unwrap(), "OPEN");
    assert!(initial_data["channelUpdated"]["closureTime"].is_null());

    // Transition to PENDINGTOCLOSE
    let closure_time = SystemTime::now() + Duration::from_secs(1000);
    let pending_channel = ChannelEntry::new(addr1, addr2, balance, 0, ChannelStatus::PendingToClose(closure_time), 1);
    db.upsert_channel(None, pending_channel, 110, 0, 0).await.unwrap();

    // Publish event to trigger Phase 2 update
    let channel_update = create_channel_update_event(&db, &channel_id).await.unwrap();
    indexer_state.publish_event(IndexerEvent::ChannelUpdated(Box::new(channel_update)));

    // Should receive update with PENDINGTOCLOSE status
    let updated = tokio::time::timeout(Duration::from_secs(3), stream.next())
        .await
        .unwrap()
        .unwrap();

    assert!(updated.errors.is_empty());
    let updated_data = updated.data.into_json().unwrap();
    assert_eq!(
        updated_data["channelUpdated"]["status"].as_str().unwrap(),
        "PENDINGTOCLOSE"
    );
    assert!(!updated_data["channelUpdated"]["closureTime"].is_null());
}

#[tokio::test]
async fn test_channel_subscription_receives_status_transition_pending_to_closed() {
    let db = BlokliDb::new_in_memory().await.unwrap();
    let indexer_state = IndexerState::new(10, 100);

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

    // Create PENDINGTOCLOSE channel
    let balance = HoprBalance::from_str("1000 wxHOPR").unwrap();
    let closure_time = SystemTime::now() + Duration::from_secs(1000);
    let channel = ChannelEntry::new(addr1, addr2, balance, 0, ChannelStatus::PendingToClose(closure_time), 1);
    let channel_id = channel.get_id().to_hex();
    db.upsert_channel(None, channel, 100, 0, 0).await.unwrap();

    // Update watermark to include all test data
    update_watermark(&db, 1000, 0, 0).await;

    let schema = create_test_schema_with_state(&db, indexer_state.clone());

    let query = r#"
        subscription {
            channelUpdated(sourceKeyId: 1) {
                status
            }
        }
    "#;

    let mut stream = schema.execute_stream(query).boxed();

    // Receive initial PENDINGTOCLOSE state
    let initial = tokio::time::timeout(Duration::from_secs(2), stream.next())
        .await
        .unwrap()
        .unwrap();

    assert!(initial.errors.is_empty());
    let initial_data = initial.data.into_json().unwrap();
    assert_eq!(
        initial_data["channelUpdated"]["status"].as_str().unwrap(),
        "PENDINGTOCLOSE"
    );

    // Transition to CLOSED
    let closed_channel = ChannelEntry::new(addr1, addr2, balance, 0, ChannelStatus::Closed, 1);
    db.upsert_channel(None, closed_channel, 110, 0, 0).await.unwrap();

    // Publish event to trigger Phase 2 update
    let channel_update = create_channel_update_event(&db, &channel_id).await.unwrap();
    indexer_state.publish_event(IndexerEvent::ChannelUpdated(Box::new(channel_update)));

    // Should receive update with CLOSED status
    let updated = tokio::time::timeout(Duration::from_secs(3), stream.next())
        .await
        .unwrap()
        .unwrap();

    assert!(updated.errors.is_empty());
    let updated_data = updated.data.into_json().unwrap();
    assert_eq!(updated_data["channelUpdated"]["status"].as_str().unwrap(), "CLOSED");
}

#[tokio::test]
async fn test_channel_subscription_receives_epoch_update() {
    let db = BlokliDb::new_in_memory().await.unwrap();
    let indexer_state = IndexerState::new(10, 100);

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

    // Create channel with epoch 1
    let balance = HoprBalance::from_str("1000 wxHOPR").unwrap();
    let channel = ChannelEntry::new(addr1, addr2, balance, 0, ChannelStatus::Open, 1);
    let channel_id = channel.get_id().to_hex();
    db.upsert_channel(None, channel, 100, 0, 0).await.unwrap();

    // Update watermark to include all test data
    update_watermark(&db, 1000, 0, 0).await;

    let schema = create_test_schema_with_state(&db, indexer_state.clone());

    let query = r#"
        subscription {
            channelUpdated(sourceKeyId: 1) {
                epoch
            }
        }
    "#;

    let mut stream = schema.execute_stream(query).boxed();

    // Receive initial epoch 1
    let initial = tokio::time::timeout(Duration::from_secs(2), stream.next())
        .await
        .unwrap()
        .unwrap();

    assert!(initial.errors.is_empty());
    let initial_data = initial.data.into_json().unwrap();
    assert_eq!(initial_data["channelUpdated"]["epoch"].as_i64().unwrap(), 1);

    // Update channel to epoch 2
    let updated_channel = ChannelEntry::new(addr1, addr2, balance, 0, ChannelStatus::Open, 2);
    db.upsert_channel(None, updated_channel, 110, 0, 0).await.unwrap();

    // Publish event to trigger Phase 2 update
    let channel_update = create_channel_update_event(&db, &channel_id).await.unwrap();
    indexer_state.publish_event(IndexerEvent::ChannelUpdated(Box::new(channel_update)));

    // Should receive update with epoch 2
    let updated = tokio::time::timeout(Duration::from_secs(3), stream.next())
        .await
        .unwrap()
        .unwrap();

    assert!(updated.errors.is_empty());
    let updated_data = updated.data.into_json().unwrap();
    assert_eq!(updated_data["channelUpdated"]["epoch"].as_i64().unwrap(), 2);
}

#[tokio::test]
async fn test_channel_subscription_filter_excludes_non_matching_channels() {
    let db = BlokliDb::new_in_memory().await.unwrap();
    let indexer_state = IndexerState::new(10, 100);

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

    // Create two channels
    let balance1 = HoprBalance::from_str("1000 wxHOPR").unwrap();
    let channel1 = ChannelEntry::new(addr1, addr2, balance1, 0, ChannelStatus::Open, 1);
    let channel1_id = channel1.get_id().to_hex();
    db.upsert_channel(None, channel1, 100, 0, 0).await.unwrap();

    let balance2 = HoprBalance::from_str("2000 wxHOPR").unwrap();
    let channel2 = ChannelEntry::new(addr2, addr3, balance2, 0, ChannelStatus::Open, 1);
    let channel2_id = channel2.get_id().to_hex();
    db.upsert_channel(None, channel2, 101, 0, 0).await.unwrap();

    // Update watermark to include all test data
    update_watermark(&db, 1000, 0, 0).await;

    let schema = create_test_schema_with_state(&db, indexer_state.clone());

    // Subscribe with sourceKeyId filter for channel 1 only
    let query = r#"
        subscription {
            channelUpdated(sourceKeyId: 1) {
                source
                destination
            }
        }
    "#;

    let mut stream = schema.execute_stream(query).boxed();

    // Should receive channel 1 from Phase 1
    let result1 = tokio::time::timeout(Duration::from_secs(2), stream.next())
        .await
        .unwrap()
        .unwrap();

    let data1 = result1.data.into_json().unwrap();
    assert_eq!(data1["channelUpdated"]["source"].as_i64().unwrap(), 1);
    assert_eq!(data1["channelUpdated"]["destination"].as_i64().unwrap(), 2);

    // Update channel 2 and publish event - should NOT appear in subscription (filtered out)
    let updated_balance2 = HoprBalance::from_str("3000 wxHOPR").unwrap();
    let updated_channel2 = ChannelEntry::new(addr2, addr3, updated_balance2, 0, ChannelStatus::Open, 1);
    db.upsert_channel(None, updated_channel2, 110, 0, 0).await.unwrap();

    let channel2_update = create_channel_update_event(&db, &channel2_id).await.unwrap();
    indexer_state.publish_event(IndexerEvent::ChannelUpdated(Box::new(channel2_update)));

    // Update channel 1 and publish event - should appear in subscription
    let updated_balance1 = HoprBalance::from_str("1500 wxHOPR").unwrap();
    let updated_channel1 = ChannelEntry::new(addr1, addr2, updated_balance1, 0, ChannelStatus::Open, 1);
    db.upsert_channel(None, updated_channel1, 111, 0, 0).await.unwrap();

    let channel1_update = create_channel_update_event(&db, &channel1_id).await.unwrap();
    indexer_state.publish_event(IndexerEvent::ChannelUpdated(Box::new(channel1_update)));

    // Should receive channel 1 update (not channel 2)
    let result2 = tokio::time::timeout(Duration::from_secs(3), stream.next())
        .await
        .unwrap()
        .unwrap();

    let data2 = result2.data.into_json().unwrap();
    assert_eq!(
        data2["channelUpdated"]["source"].as_i64().unwrap(),
        1,
        "Should only emit channel 1, never channel 2"
    );
}

#[tokio::test]
async fn test_channel_subscription_handles_empty_database() {
    let db = BlokliDb::new_in_memory().await.unwrap();

    // Update watermark to include all test data
    update_watermark(&db, 1000, 0, 0).await;

    let schema = create_test_schema(&db);

    let query = r#"
        subscription {
            channelUpdated {
                source
            }
        }
    "#;

    let mut stream = schema.execute_stream(query).boxed();

    // Should not receive any channels within timeout (empty database)
    let no_result = tokio::time::timeout(Duration::from_millis(1500), stream.next()).await;
    assert!(no_result.is_err(), "Should not emit channels from empty database");
}

#[tokio::test]
async fn test_channel_subscription_multiple_concurrent_subscribers() {
    let db = BlokliDb::new_in_memory().await.unwrap();
    let indexer_state = IndexerState::new(10, 100);

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
    let balance = HoprBalance::from_str("1000 wxHOPR").unwrap();
    let channel = ChannelEntry::new(addr1, addr2, balance, 0, ChannelStatus::Open, 1);
    let channel_id = channel.get_id().to_hex();
    db.upsert_channel(None, channel, 100, 0, 0).await.unwrap();

    // Update watermark to include all test data
    update_watermark(&db, 1000, 0, 0).await;

    let schema = create_test_schema_with_state(&db, indexer_state.clone());

    let query = r#"
        subscription {
            channelUpdated(sourceKeyId: 1) {
                source
                balance
            }
        }
    "#;

    // Create two concurrent subscribers
    let mut stream1 = schema.execute_stream(query).boxed();
    let mut stream2 = schema.execute_stream(query).boxed();

    // Both should receive initial channel
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

    // Update channel balance
    let updated_balance = HoprBalance::from_str("2000 wxHOPR").unwrap();
    let updated_channel = ChannelEntry::new(addr1, addr2, updated_balance, 0, ChannelStatus::Open, 1);
    db.upsert_channel(None, updated_channel, 110, 0, 0).await.unwrap();

    // Publish event to trigger Phase 2 update for both subscribers
    let channel_update = create_channel_update_event(&db, &channel_id).await.unwrap();
    indexer_state.publish_event(IndexerEvent::ChannelUpdated(Box::new(channel_update)));

    // Both should receive the update
    let update1 = tokio::time::timeout(Duration::from_secs(3), stream1.next())
        .await
        .unwrap()
        .unwrap();
    let data1 = update1.data.into_json().unwrap();
    assert_eq!(
        data1["channelUpdated"]["balance"].as_str().unwrap(),
        updated_balance.to_string()
    );

    let update2 = tokio::time::timeout(Duration::from_secs(3), stream2.next())
        .await
        .unwrap()
        .unwrap();
    let data2 = update2.data.into_json().unwrap();
    assert_eq!(
        data2["channelUpdated"]["balance"].as_str().unwrap(),
        updated_balance.to_string()
    );
}

#[tokio::test]
async fn test_channel_subscription_with_combined_filters() {
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
    let balance = HoprBalance::from_str("1000 wxHOPR").unwrap();
    let channel = ChannelEntry::new(addr1, addr2, balance, 0, ChannelStatus::Open, 1);
    db.upsert_channel(None, channel, 100, 0, 0).await.unwrap();

    // Update watermark to include all test data
    update_watermark(&db, 1000, 0, 0).await;

    let schema = create_test_schema(&db);

    // Subscribe with multiple filters (sourceKeyId and destinationKeyId)
    let query = r#"
        subscription {
            channelUpdated(sourceKeyId: 1, destinationKeyId: 2) {
                source
                destination
                status
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
    assert_eq!(data["channelUpdated"]["source"].as_i64().unwrap(), 1);
    assert_eq!(data["channelUpdated"]["destination"].as_i64().unwrap(), 2);
    assert_eq!(data["channelUpdated"]["status"].as_str().unwrap(), "OPEN");
}
