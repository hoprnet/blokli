//! Integration tests for openedChannelGraphUpdated subscription
//!
//! These tests verify the channel graph subscription functionality:
//! - Initial graph emission with all open channels (one entry per channel)
//! - Graph updates when channels are opened
//! - Graph updates when channels are closed
//! - Account information is included for all channel participants
//! - Initial snapshots include only OPEN channels; later updates may include CLOSED removal signals
//! - Each emission contains one channel with its source and destination accounts
//! - Every entry after the snapshot, and the last snapshot entry, carries a resume cursor
//! - A subscription resumed with `after` sends every channel changed after the cursor, closed ones included
//! - The subscription ends when it falls behind the event bus, so the client reconnects and gets a fresh snapshot

mod common;

use std::{
    collections::HashSet,
    str::FromStr,
    time::{Duration, UNIX_EPOCH},
};

use async_graphql::Response;
use blokli_api_types::{Account, Channel, ChannelStatus as ApiChannelStatus, TokenValueString, UInt64};
use blokli_chain_indexer::state::IndexerEvent;
use blokli_db::{
    BlokliDbGeneralModelOperations, TargetDb, accounts::BlokliDbAccountOperations, channels::BlokliDbChannelOperations,
    db::BlokliDb,
};
use blokli_db_entity::{
    chain_info, channel, channel_state, conversions::account_aggregation::fetch_accounts_by_keyids,
};
use chrono::Utc;
use futures::{StreamExt, stream::BoxStream};
use hopr_types::{
    crypto::prelude::{ChainKeypair, Keypair, OffchainKeypair},
    internal::channels::{ChannelEntry, ChannelStatus},
    primitive::{
        prelude::{Address, Balance as PrimitiveBalance, HoprBalance, ToHex, WxHOPR},
        traits::IntoEndian,
    },
};
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter, QueryOrder, Set, sea_query::OnConflict};
use serde_json::{Value, json};

/// Helper to generate random keypair for testing
fn random_keypair() -> ChainKeypair {
    ChainKeypair::random()
}

/// Helper to generate random offchain keypair
fn random_offchain_keypair() -> OffchainKeypair {
    OffchainKeypair::random()
}

fn build_channel_entry(
    source: Address,
    destination: Address,
    balance: HoprBalance,
    ticket_index: u64,
    status: ChannelStatus,
    channel_epoch: u32,
) -> ChannelEntry {
    ChannelEntry::builder()
        .between(source, destination)
        .balance(balance)
        .ticket_index(ticket_index)
        .status(status)
        .epoch(channel_epoch)
        .build()
        .expect("valid channel entry")
}

/// Helper to create a ChannelUpdate event from database data for testing
async fn create_channel_update_event(
    db: &BlokliDb,
    channel_id: &str,
) -> Result<blokli_api_types::ChannelUpdate, Box<dyn std::error::Error>> {
    // Find the channel
    let channel_model = channel::Entity::find()
        .filter(channel::Column::ConcreteChannelId.eq(channel_id.strip_prefix("0x").unwrap_or(channel_id)))
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
        balance: TokenValueString(PrimitiveBalance::<WxHOPR>::from_be_bytes(&state.balance).to_string()),
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
        position: Some(blokli_api_types::ChannelStatePosition::from_published(
            state.published_block,
            state.published_tx_index,
            state.published_log_index,
        )?),
    })
}

/// Helper to update chain_info watermark for tests
async fn update_watermark(db: &BlokliDb, block: i64, tx_index: i64, log_index: i64) {
    let chain_info_model = chain_info::ActiveModel {
        id: Set(1),
        last_indexed_block: Set(block),
        last_indexed_tx_index: Set(Some(tx_index)),
        last_indexed_log_index: Set(Some(log_index)),
        ticket_price: Set(None),
        channels_dst: Set(None),
        ledger_dst: Set(None),
        safe_registry_dst: Set(None),
        min_incoming_ticket_win_prob: Set(0.0),
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
        .exec(db.conn(blokli_db::TargetDb::Index))
        .await
        .unwrap();
}

#[tokio::test]
async fn test_opened_channel_graph_subscription_emits_initial_entry() {
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
    let channel = build_channel_entry(addr1, addr2, balance, 0, ChannelStatus::Open, 1);
    db.upsert_channel(None, channel, 100, 0, 0).await.unwrap();

    // Update watermark to include all test data
    update_watermark(&db, 1000, 0, 0).await;

    // Create GraphQL schema
    let (schema, _indexer_state) = common::create_test_schema(&db);

    // Execute subscription query with new schema structure
    let query = r#"
        subscription {
            openedChannelGraphUpdated {
                channel {
                    source
                    destination
                    status
                }
                source {
                    keyid
                    chainKey
                    packetKey
                }
                destination {
                    keyid
                    chainKey
                    packetKey
                }
            }
        }
    "#;

    let mut stream = schema.execute_stream(query).boxed();

    // Should receive one entry for the single open channel
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
    let entry = &data["openedChannelGraphUpdated"];

    // Verify channel
    let channel = &entry["channel"];
    assert_eq!(channel["source"].as_i64().unwrap(), 1);
    assert_eq!(channel["destination"].as_i64().unwrap(), 2);
    assert_eq!(channel["status"].as_str().unwrap(), "OPEN");

    // Verify source account
    let source = &entry["source"];
    assert_eq!(source["keyid"].as_i64().unwrap(), 1);

    // Verify destination account
    let destination = &entry["destination"];
    assert_eq!(destination["keyid"].as_i64().unwrap(), 2);
}

#[tokio::test]
async fn test_opened_channel_graph_subscription_emits_multiple_entries() {
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
    let channel1 = build_channel_entry(addr1, addr2, balance1, 0, ChannelStatus::Open, 1);
    db.upsert_channel(None, channel1, 100, 0, 0).await.unwrap();

    let balance2 = HoprBalance::from_str("2000 wxHOPR").unwrap();
    let channel2 = build_channel_entry(addr2, addr3, balance2, 0, ChannelStatus::Open, 1);
    db.upsert_channel(None, channel2, 101, 0, 0).await.unwrap();

    // Update watermark to include all test data
    update_watermark(&db, 1000, 0, 0).await;

    let (schema, _indexer_state) = common::create_test_schema(&db);

    let query = r#"
        subscription {
            openedChannelGraphUpdated {
                channel {
                    source
                    destination
                }
                source {
                    keyid
                }
                destination {
                    keyid
                }
            }
        }
    "#;

    let mut stream = schema.execute_stream(query).boxed();

    // Collect first 2 entries (one for each channel)
    let mut entries = Vec::new();
    for _ in 0..2 {
        let result = tokio::time::timeout(Duration::from_secs(2), stream.next())
            .await
            .unwrap()
            .unwrap();
        assert!(result.errors.is_empty());
        entries.push(result.data.into_json().unwrap());
    }

    // Should have received 2 entries
    assert_eq!(entries.len(), 2);

    // Collect all source/destination pairs
    let mut channel_pairs = HashSet::new();
    for entry_data in &entries {
        let entry = &entry_data["openedChannelGraphUpdated"];
        let source = entry["channel"]["source"].as_i64().unwrap();
        let destination = entry["channel"]["destination"].as_i64().unwrap();
        channel_pairs.insert((source, destination));
    }

    // Verify we got both channels
    assert!(channel_pairs.contains(&(1, 2)));
    assert!(channel_pairs.contains(&(2, 3)));
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
    let open_channel = build_channel_entry(addr1, addr2, balance1, 0, ChannelStatus::Open, 1);
    db.upsert_channel(None, open_channel, 100, 0, 0).await.unwrap();

    let balance2 = HoprBalance::from_str("2000 wxHOPR").unwrap();
    let closed_channel = build_channel_entry(addr2, addr3, balance2, 0, ChannelStatus::Closed, 1);
    db.upsert_channel(None, closed_channel, 101, 0, 0).await.unwrap();

    // Update watermark to include all test data
    update_watermark(&db, 1000, 0, 0).await;

    let (schema, _indexer_state) = common::create_test_schema(&db);

    let query = r#"
        subscription {
            openedChannelGraphUpdated {
                channel {
                    source
                    destination
                    status
                }
            }
        }
    "#;

    let mut stream = schema.execute_stream(query).boxed();

    // Should receive only one entry for the open channel
    let result = tokio::time::timeout(Duration::from_secs(2), stream.next())
        .await
        .unwrap()
        .unwrap();

    assert!(result.errors.is_empty());
    let data = result.data.into_json().unwrap();
    let entry = &data["openedChannelGraphUpdated"];

    // Verify it's the open channel
    let channel = &entry["channel"];
    assert_eq!(channel["source"].as_i64().unwrap(), 1);
    assert_eq!(channel["destination"].as_i64().unwrap(), 2);
    assert_eq!(channel["status"].as_str().unwrap(), "OPEN");
}

#[tokio::test]
async fn test_opened_channel_graph_subscription_receives_new_channel_entry() {
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
    let channel1 = build_channel_entry(addr1, addr2, balance1, 0, ChannelStatus::Open, 1);
    db.upsert_channel(None, channel1, 100, 0, 0).await.unwrap();

    // Update watermark to include all test data
    update_watermark(&db, 1000, 0, 0).await;

    let (schema, indexer_state) = common::create_test_schema(&db);

    let query = r#"
        subscription {
            openedChannelGraphUpdated {
                channel {
                    source
                    destination
                }
            }
        }
    "#;

    let mut stream = schema.execute_stream(query).boxed();

    // Receive initial entry for first channel
    let initial = tokio::time::timeout(Duration::from_secs(2), stream.next())
        .await
        .unwrap()
        .unwrap();

    assert!(initial.errors.is_empty());
    let initial_data = initial.data.into_json().unwrap();
    let initial_entry = &initial_data["openedChannelGraphUpdated"];
    assert_eq!(initial_entry["channel"]["source"].as_i64().unwrap(), 1);
    assert_eq!(initial_entry["channel"]["destination"].as_i64().unwrap(), 2);

    // Add a new open channel
    let balance2 = HoprBalance::from_str("2000 wxHOPR").unwrap();
    let channel2 = build_channel_entry(addr2, addr3, balance2, 0, ChannelStatus::Open, 1);
    let channel2_id = channel2.get_id().to_hex();
    db.upsert_channel(None, channel2, 110, 0, 0).await.unwrap();

    // Publish event for the new channel to the event bus
    let channel2_update = create_channel_update_event(&db, &channel2_id).await.unwrap();
    indexer_state.publish_event(IndexerEvent::ChannelUpdated(Box::new(channel2_update)));

    // Should receive entry for the new channel
    let new_channel = tokio::time::timeout(Duration::from_secs(3), stream.next())
        .await
        .unwrap()
        .unwrap();

    assert!(new_channel.errors.is_empty());
    let new_data = new_channel.data.into_json().unwrap();
    let new_entry = &new_data["openedChannelGraphUpdated"];

    // Verify we received the new channel (2, 3)
    assert_eq!(new_entry["channel"]["source"].as_i64().unwrap(), 2);
    assert_eq!(new_entry["channel"]["destination"].as_i64().unwrap(), 3);
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
    let channel = build_channel_entry(addr1, addr2, balance, 0, ChannelStatus::Open, 1);
    db.upsert_channel(None, channel, 100, 0, 0).await.unwrap();

    // Update watermark to include all test data
    update_watermark(&db, 1000, 0, 0).await;

    let (schema, _indexer_state) = common::create_test_schema(&db);

    let query = r#"
        subscription {
            openedChannelGraphUpdated {
                channel {
                    source
                    destination
                }
            }
        }
    "#;

    let mut stream = schema.execute_stream(query).boxed();

    // Receive initial entry with 1 channel
    let initial = tokio::time::timeout(Duration::from_secs(2), stream.next())
        .await
        .unwrap()
        .unwrap();

    assert!(initial.errors.is_empty());

    // Close the channel
    let closed_channel = build_channel_entry(addr1, addr2, balance, 0, ChannelStatus::Closed, 1);
    db.upsert_channel(None, closed_channel, 110, 0, 0).await.unwrap();

    // Update watermark to include the channel closure
    update_watermark(&db, 1001, 0, 0).await;

    // Next poll should return no entries (empty result set, no stream items)
    // The subscription will timeout because there are no open channels to emit
    let updated = tokio::time::timeout(Duration::from_secs(3), stream.next()).await;

    // Either we timeout (no entries emitted) or we get an empty response
    // Both are acceptable since there are no open channels
    if let Ok(Some(response)) = updated {
        assert!(response.errors.is_empty());
        // If we do get a response, it should not contain the closed channel
        // (implementation detail: polling may not emit anything when result set is empty)
    }
}

#[tokio::test]
async fn test_opened_channel_graph_subscription_handles_empty_database() {
    let db = BlokliDb::new_in_memory().await.unwrap();

    // Update watermark to include all test data
    update_watermark(&db, 1000, 0, 0).await;

    let (schema, _indexer_state) = common::create_test_schema(&db);

    let query = r#"
        subscription {
            openedChannelGraphUpdated {
                channel {
                    source
                }
            }
        }
    "#;

    let mut stream = schema.execute_stream(query).boxed();

    // With empty database, subscription should timeout (no entries to emit)
    let result = tokio::time::timeout(Duration::from_secs(2), stream.next()).await;

    // Timeout is expected since there are no channels to emit
    assert!(result.is_err(), "Should timeout when no channels exist");
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
    let channel = build_channel_entry(addr1, addr2, balance, 0, ChannelStatus::Open, 1);
    db.upsert_channel(None, channel, 100, 0, 0).await.unwrap();

    // Update watermark to include all test data
    update_watermark(&db, 1000, 0, 0).await;

    let (schema, _indexer_state) = common::create_test_schema(&db);

    let query = r#"
        subscription {
            openedChannelGraphUpdated {
                channel {
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
    let entry = &data["openedChannelGraphUpdated"];
    // Balance includes token identifier (e.g., "1234 wxHOPR")
    assert_eq!(entry["channel"]["balance"].as_str().unwrap(), "1234 wxHOPR");
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
    let channel = build_channel_entry(addr1, addr2, initial_balance, 0, ChannelStatus::Open, 1);
    let channel_id = channel.get_id().to_hex();
    db.upsert_channel(None, channel, 100, 0, 0).await.unwrap();

    // Update watermark to include all test data
    update_watermark(&db, 1000, 0, 0).await;

    let (schema, indexer_state) = common::create_test_schema(&db);

    let query = r#"
        subscription {
            openedChannelGraphUpdated {
                channel {
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
    let initial_balance_str = initial_data["openedChannelGraphUpdated"]["channel"]["balance"]
        .as_str()
        .unwrap();
    assert_eq!(initial_balance_str, "1000 wxHOPR");

    // Update channel balance
    let updated_balance = HoprBalance::from_str("2000 wxHOPR").unwrap();
    let updated_channel = build_channel_entry(addr1, addr2, updated_balance, 0, ChannelStatus::Open, 1);
    db.upsert_channel(None, updated_channel, 110, 0, 0).await.unwrap();

    // Publish event for the updated channel to the event bus
    let channel_update = create_channel_update_event(&db, &channel_id).await.unwrap();
    indexer_state.publish_event(IndexerEvent::ChannelUpdated(Box::new(channel_update)));

    // Should receive updated balance entry
    let updated = tokio::time::timeout(Duration::from_secs(3), stream.next())
        .await
        .unwrap()
        .unwrap();

    assert!(updated.errors.is_empty());
    let updated_data = updated.data.into_json().unwrap();
    let updated_balance_str = updated_data["openedChannelGraphUpdated"]["channel"]["balance"]
        .as_str()
        .unwrap();
    assert_eq!(updated_balance_str, "2000 wxHOPR");
}

async fn assert_opened_channel_graph_subscription_emits_status_update(status: ChannelStatus, expected_status: &str) {
    let db = BlokliDb::new_in_memory().await.unwrap();

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

    let balance = HoprBalance::from_str("1000 wxHOPR").unwrap();
    let open_channel = build_channel_entry(addr1, addr2, balance, 0, ChannelStatus::Open, 1);
    let channel_id = open_channel.get_id().to_hex();
    db.upsert_channel(None, open_channel, 100, 0, 0).await.unwrap();

    update_watermark(&db, 1000, 0, 0).await;

    let (schema, indexer_state) = common::create_test_schema(&db);
    let query = r#"
        subscription {
            openedChannelGraphUpdated {
                channel {
                    concreteChannelId
                    status
                }
                source {
                    keyid
                }
                destination {
                    keyid
                }
            }
        }
    "#;

    let mut stream = schema.execute_stream(query).boxed();

    let initial = tokio::time::timeout(Duration::from_secs(2), stream.next())
        .await
        .unwrap()
        .unwrap();
    assert!(initial.errors.is_empty());
    let initial_data = initial.data.into_json().unwrap();
    assert_eq!(
        initial_data["openedChannelGraphUpdated"]["channel"]["status"]
            .as_str()
            .unwrap(),
        "OPEN"
    );

    let updated_channel = build_channel_entry(addr1, addr2, balance, 0, status, 1);
    db.upsert_channel(None, updated_channel, 110, 0, 0).await.unwrap();

    let channel_update = create_channel_update_event(&db, &channel_id).await.unwrap();
    indexer_state.publish_event(IndexerEvent::ChannelUpdated(Box::new(channel_update)));

    let updated = tokio::time::timeout(Duration::from_secs(3), stream.next())
        .await
        .unwrap()
        .unwrap();

    assert!(updated.errors.is_empty());
    let updated_data = updated.data.into_json().unwrap();
    let entry = &updated_data["openedChannelGraphUpdated"];
    assert_eq!(
        entry["channel"]["concreteChannelId"].as_str().unwrap(),
        channel_id.strip_prefix("0x").unwrap_or(&channel_id)
    );
    assert_eq!(entry["channel"]["status"].as_str().unwrap(), expected_status);
    assert_eq!(entry["source"]["keyid"].as_i64().unwrap(), 1);
    assert_eq!(entry["destination"]["keyid"].as_i64().unwrap(), 2);
}

#[tokio::test]
async fn test_opened_channel_graph_subscription_emits_pending_to_close_update() {
    assert_opened_channel_graph_subscription_emits_status_update(
        ChannelStatus::PendingToClose(UNIX_EPOCH),
        "PENDINGTOCLOSE",
    )
    .await;
}

#[tokio::test]
async fn test_opened_channel_graph_subscription_emits_closed_update() {
    assert_opened_channel_graph_subscription_emits_status_update(ChannelStatus::Closed, "CLOSED").await;
}

#[tokio::test]
async fn test_opened_channel_graph_subscription_ends_when_event_bus_overflows() {
    let db = BlokliDb::new_in_memory().await.unwrap();

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

    let balance = HoprBalance::from_str("1000 wxHOPR").unwrap();
    let open_channel = build_channel_entry(addr1, addr2, balance, 0, ChannelStatus::Open, 1);
    let channel_id = open_channel.get_id().to_hex();
    db.upsert_channel(None, open_channel, 100, 0, 0).await.unwrap();

    update_watermark(&db, 1000, 0, 0).await;

    // The event bus keeps a single event, so a second one published before the subscription reads makes it fall
    // behind.
    let (schema, indexer_state) = common::create_test_schema_with_event_bus_capacity(&db, 1);
    let query = r#"
        subscription {
            openedChannelGraphUpdated {
                channel {
                    status
                }
            }
        }
    "#;

    let mut stream = schema.execute_stream(query).boxed();

    let initial = tokio::time::timeout(Duration::from_secs(2), stream.next())
        .await
        .expect("snapshot entry within timeout")
        .expect("snapshot entry");
    assert!(initial.errors.is_empty(), "unexpected errors: {:?}", initial.errors);

    let pending_channel = build_channel_entry(addr1, addr2, balance, 0, ChannelStatus::PendingToClose(UNIX_EPOCH), 1);
    db.upsert_channel(None, pending_channel, 110, 0, 0).await.unwrap();
    let pending_update = create_channel_update_event(&db, &channel_id).await.unwrap();

    let closed_channel = build_channel_entry(addr1, addr2, balance, 0, ChannelStatus::Closed, 1);
    db.upsert_channel(None, closed_channel, 120, 0, 0).await.unwrap();
    let closed_update = create_channel_update_event(&db, &channel_id).await.unwrap();

    indexer_state.publish_event(IndexerEvent::ChannelUpdated(Box::new(pending_update)));
    indexer_state.publish_event(IndexerEvent::ChannelUpdated(Box::new(closed_update)));

    // Without ending, the subscription would skip the PendingToClose update and carry on as if nothing was lost.
    let next = tokio::time::timeout(Duration::from_secs(2), stream.next())
        .await
        .expect("subscription must end, not hang");
    assert!(
        next.is_none(),
        "subscription must end after the event bus overflowed, got {next:?}"
    );
}

/// Two accounts and an open channel between them, opened at block 100
async fn open_channel_fixture(db: &BlokliDb) -> (Address, Address, HoprBalance) {
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

    let balance = HoprBalance::from_str("1000 wxHOPR").unwrap();
    let channel = build_channel_entry(addr1, addr2, balance, 0, ChannelStatus::Open, 1);
    db.upsert_channel(None, channel, 100, 0, 0).await.unwrap();

    (addr1, addr2, balance)
}

const RESUMABLE_GRAPH_FIELDS: &str = r#"
    channel {
        source
        destination
        status
    }
    cursor {
        block
        transactionIndex
        logIndex
    }
"#;

async fn next_graph_entry(stream: &mut BoxStream<'static, Response>) -> Value {
    let response = tokio::time::timeout(Duration::from_secs(2), stream.next())
        .await
        .expect("entry within timeout")
        .expect("stream still open");
    assert!(response.errors.is_empty(), "unexpected errors: {:?}", response.errors);
    response.data.into_json().unwrap()["openedChannelGraphUpdated"].clone()
}

#[tokio::test]
async fn test_opened_channel_graph_snapshot_puts_the_cursor_on_the_last_entry_only() {
    let db = BlokliDb::new_in_memory().await.unwrap();
    let (addr1, addr2, balance) = open_channel_fixture(&db).await;
    let reverse = build_channel_entry(addr2, addr1, balance, 0, ChannelStatus::Open, 1);
    db.upsert_channel(None, reverse, 102, 0, 0).await.unwrap();
    update_watermark(&db, 1000, 3, 4).await;

    let (schema, _indexer_state) = common::create_test_schema(&db);
    let query = format!("subscription {{ openedChannelGraphUpdated {{ {RESUMABLE_GRAPH_FIELDS} }} }}");
    let mut stream = schema.execute_stream(query).boxed();

    let first = next_graph_entry(&mut stream).await;
    let last = next_graph_entry(&mut stream).await;

    assert!(
        first["cursor"].is_null(),
        "only the last snapshot entry has a cursor: {first}"
    );
    assert_eq!(
        last["cursor"],
        json!({ "block": "1000", "transactionIndex": "3", "logIndex": "4" })
    );
}

#[tokio::test]
async fn test_opened_channel_graph_live_updates_carry_their_position_as_cursor() {
    let db = BlokliDb::new_in_memory().await.unwrap();
    let (addr1, addr2, balance) = open_channel_fixture(&db).await;
    let channel_id = build_channel_entry(addr1, addr2, balance, 0, ChannelStatus::Open, 1)
        .get_id()
        .to_hex();
    update_watermark(&db, 1000, 0, 0).await;

    let (schema, indexer_state) = common::create_test_schema(&db);
    let query = format!("subscription {{ openedChannelGraphUpdated {{ {RESUMABLE_GRAPH_FIELDS} }} }}");
    let mut stream = schema.execute_stream(query).boxed();
    next_graph_entry(&mut stream).await;

    let closing = build_channel_entry(addr1, addr2, balance, 0, ChannelStatus::PendingToClose(UNIX_EPOCH), 1);
    db.upsert_channel(None, closing, 1001, 2, 7).await.unwrap();
    let update = create_channel_update_event(&db, &channel_id).await.unwrap();
    indexer_state.publish_event(IndexerEvent::ChannelUpdated(Box::new(update)));

    let entry = next_graph_entry(&mut stream).await;
    assert_eq!(entry["channel"]["status"], "PENDINGTOCLOSE");
    assert_eq!(
        entry["cursor"],
        json!({ "block": "1001", "transactionIndex": "2", "logIndex": "7" })
    );
}

#[tokio::test]
async fn test_opened_channel_graph_resumes_with_closures_missed_after_the_cursor() {
    let db = BlokliDb::new_in_memory().await.unwrap();
    let (addr1, addr2, balance) = open_channel_fixture(&db).await;

    // A second channel that did not change after the cursor is not sent again.
    let reverse = build_channel_entry(addr2, addr1, balance, 0, ChannelStatus::Open, 1);
    db.upsert_channel(None, reverse, 102, 0, 0).await.unwrap();

    // While the client was away, the first channel moved to pending to close and then closed.
    let closing = build_channel_entry(addr1, addr2, balance, 0, ChannelStatus::PendingToClose(UNIX_EPOCH), 1);
    db.upsert_channel(None, closing, 110, 1, 0).await.unwrap();
    let closed = build_channel_entry(addr1, addr2, balance, 0, ChannelStatus::Closed, 1);
    db.upsert_channel(None, closed, 120, 0, 3).await.unwrap();
    update_watermark(&db, 1000, 0, 0).await;

    let (schema, _indexer_state) = common::create_test_schema(&db);
    let query = format!(
        r#"subscription {{
            openedChannelGraphUpdated(after: {{ block: "105", transactionIndex: "0", logIndex: "0" }}) {{
                {RESUMABLE_GRAPH_FIELDS}
            }}
        }}"#
    );
    let mut stream = schema.execute_stream(query).boxed();

    let entry = next_graph_entry(&mut stream).await;
    assert_eq!(entry["channel"]["source"], 1);
    assert_eq!(entry["channel"]["status"], "CLOSED", "only the latest state is sent");
    assert_eq!(
        entry["cursor"],
        json!({ "block": "120", "transactionIndex": "0", "logIndex": "3" })
    );

    let nothing_else = tokio::time::timeout(Duration::from_millis(500), stream.next()).await;
    assert!(nothing_else.is_err(), "unchanged channels are not sent again");
}

#[tokio::test]
async fn test_opened_channel_graph_resume_sends_oldest_change_first_and_respects_log_order() {
    let db = BlokliDb::new_in_memory().await.unwrap();
    let (addr1, addr2, balance) = open_channel_fixture(&db).await;
    let reverse = build_channel_entry(addr2, addr1, balance, 0, ChannelStatus::Open, 1);
    db.upsert_channel(None, reverse, 102, 0, 0).await.unwrap();

    // Both change in block 200; the reverse channel's log comes first.
    let closing = build_channel_entry(addr1, addr2, balance, 0, ChannelStatus::PendingToClose(UNIX_EPOCH), 1);
    db.upsert_channel(None, closing, 200, 4, 1).await.unwrap();
    let reverse_closing = build_channel_entry(addr2, addr1, balance, 0, ChannelStatus::PendingToClose(UNIX_EPOCH), 1);
    db.upsert_channel(None, reverse_closing, 200, 2, 0).await.unwrap();
    update_watermark(&db, 1000, 0, 0).await;

    let (schema, _indexer_state) = common::create_test_schema(&db);
    // The cursor sits between the two changes, inside block 200.
    let query = format!(
        r#"subscription {{
            openedChannelGraphUpdated(after: {{ block: "200", transactionIndex: "3", logIndex: "0" }}) {{
                {RESUMABLE_GRAPH_FIELDS}
            }}
        }}"#
    );
    let mut stream = schema.execute_stream(query).boxed();

    let entry = next_graph_entry(&mut stream).await;
    assert_eq!(entry["channel"]["source"], 1);
    assert_eq!(
        entry["cursor"],
        json!({ "block": "200", "transactionIndex": "4", "logIndex": "1" })
    );
    let nothing_else = tokio::time::timeout(Duration::from_millis(500), stream.next()).await;
    assert!(nothing_else.is_err(), "the change before the cursor is not sent again");

    // From an earlier cursor, both come, oldest first.
    let query = format!(
        r#"subscription {{
            openedChannelGraphUpdated(after: {{ block: "150", transactionIndex: "0", logIndex: "0" }}) {{
                {RESUMABLE_GRAPH_FIELDS}
            }}
        }}"#
    );
    let mut stream = schema.execute_stream(query).boxed();
    assert_eq!(next_graph_entry(&mut stream).await["channel"]["source"], 2);
    assert_eq!(next_graph_entry(&mut stream).await["channel"]["source"], 1);
}

#[tokio::test]
async fn test_opened_channel_graph_resume_selects_latest_state_within_watermark() {
    let db = BlokliDb::new_in_memory().await.unwrap();
    let (addr1, addr2, balance) = open_channel_fixture(&db).await;
    // Many historical states must collapse to one result in the database.
    for block in 101..120 {
        let open = build_channel_entry(addr1, addr2, balance, 0, ChannelStatus::Open, 1);
        db.upsert_channel(None, open, block, 0, 0).await.unwrap();
    }
    let closing = build_channel_entry(addr1, addr2, balance, 0, ChannelStatus::PendingToClose(UNIX_EPOCH), 1);
    db.upsert_channel(None, closing, 120, 2, 3).await.unwrap();
    // Simulate a state committed after the captured watermark, before the history query runs.
    let closed = build_channel_entry(addr1, addr2, balance, 0, ChannelStatus::Closed, 1);
    db.upsert_channel(None, closed, 121, 0, 0).await.unwrap();
    update_watermark(&db, 120, 2, 3).await;

    let (schema, _indexer_state) = common::create_test_schema(&db);
    let query = format!(
        r#"subscription {{
            openedChannelGraphUpdated(after: {{ block: "0", transactionIndex: "0", logIndex: "0" }}) {{
                {RESUMABLE_GRAPH_FIELDS}
            }}
        }}"#
    );
    let mut stream = schema.execute_stream(query).boxed();
    insta::assert_yaml_snapshot!(next_graph_entry(&mut stream).await, @r#"
    channel:
      destination: 2
      source: 1
      status: PENDINGTOCLOSE
    cursor:
      block: "120"
      logIndex: "3"
      transactionIndex: "2"
    "#);
    assert!(
        tokio::time::timeout(Duration::from_millis(100), stream.next())
            .await
            .is_err()
    );
}

#[tokio::test]
async fn test_opened_channel_graph_resume_accepts_cursor_past_watermark() {
    let db = BlokliDb::new_in_memory().await.unwrap();
    let (addr1, addr2, balance) = open_channel_fixture(&db).await;
    update_watermark(&db, 100, 0, 0).await;
    let (schema, indexer_state) = common::create_test_schema(&db);
    let query = format!(
        r#"subscription {{
            openedChannelGraphUpdated(after: {{ block: "200", transactionIndex: "0", logIndex: "0" }}) {{
                {RESUMABLE_GRAPH_FIELDS}
            }}
        }}"#
    );
    let mut stream = schema.execute_stream(query).boxed();
    // Poll setup to completion and verify that no history or error is emitted.
    assert!(
        tokio::time::timeout(Duration::from_millis(100), stream.next())
            .await
            .is_err()
    );

    let closing = build_channel_entry(addr1, addr2, balance, 0, ChannelStatus::PendingToClose(UNIX_EPOCH), 1);
    let channel_id = closing.get_id().to_hex();
    db.upsert_channel(None, closing, 201, 1, 2).await.unwrap();
    let update = create_channel_update_event(&db, &channel_id).await.unwrap();
    indexer_state.publish_event(IndexerEvent::ChannelUpdated(Box::new(update)));
    insta::assert_yaml_snapshot!(next_graph_entry(&mut stream).await, @r#"
    channel:
      destination: 2
      source: 1
      status: PENDINGTOCLOSE
    cursor:
      block: "201"
      logIndex: "2"
      transactionIndex: "1"
    "#);
}

#[tokio::test]
async fn test_opened_channel_graph_resume_hands_off_to_live_with_increasing_cursors() {
    let db = BlokliDb::new_in_memory().await.unwrap();
    let (addr1, addr2, balance) = open_channel_fixture(&db).await;
    let reverse = build_channel_entry(addr2, addr1, balance, 0, ChannelStatus::Open, 1);
    db.upsert_channel(None, reverse, 110, 1, 0).await.unwrap();
    update_watermark(&db, 110, 1, 0).await;
    let (schema, indexer_state) = common::create_test_schema(&db);
    let query = format!(
        r#"subscription {{
            openedChannelGraphUpdated(after: {{ block: "0", transactionIndex: "0", logIndex: "0" }}) {{
                {RESUMABLE_GRAPH_FIELDS}
            }}
        }}"#
    );
    let mut stream = schema.execute_stream(query).boxed();
    let first = next_graph_entry(&mut stream).await;
    // Publish during replay: the registered receiver must retain this until phase 2.
    let closed = build_channel_entry(addr1, addr2, balance, 0, ChannelStatus::Closed, 1);
    let channel_id = closed.get_id().to_hex();
    db.upsert_channel(None, closed, 111, 0, 0).await.unwrap();
    let update = create_channel_update_event(&db, &channel_id).await.unwrap();
    indexer_state.publish_event(IndexerEvent::ChannelUpdated(Box::new(update)));
    let second = next_graph_entry(&mut stream).await;
    let live = next_graph_entry(&mut stream).await;
    insta::assert_yaml_snapshot!(vec![first, second, live], @r#"
    - channel:
        destination: 2
        source: 1
        status: OPEN
      cursor:
        block: "100"
        logIndex: "0"
        transactionIndex: "0"
    - channel:
        destination: 1
        source: 2
        status: OPEN
      cursor:
        block: "110"
        logIndex: "0"
        transactionIndex: "1"
    - channel:
        destination: 2
        source: 1
        status: CLOSED
      cursor:
        block: "111"
        logIndex: "0"
        transactionIndex: "0"
    "#);
}
