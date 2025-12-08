//! Comprehensive integration tests for channels, channelCount, and chainInfo GraphQL queries
//!
//! These tests verify:
//! - Union type handling (ChannelsList, ChainInfo vs. errors)
//! - Required filter validation (channels query)
//! - Optional filter handling (channelCount query)
//! - Type conversions between database models and GraphQL types
//! - Filter combinations (sourceKeyId, destinationKeyId, concreteChannelId, status)

use std::str::FromStr;
use std::time::{Duration, SystemTime};

use anyhow::Result;
use blokli_api::query::QueryRoot;
use blokli_chain_types::ContractAddresses;
use blokli_db::{
    BlokliDbGeneralModelOperations, TargetDb, accounts::BlokliDbAccountOperations,
    channels::BlokliDbChannelOperations, db::BlokliDb,
};
use hopr_crypto_types::prelude::{ChainKeypair, Keypair, OffchainKeypair};
use hopr_internal_types::channels::{ChannelEntry, ChannelStatus};
use hopr_primitive_types::prelude::HoprBalance;
use sea_orm::{EntityTrait, Set};

/// Helper to extract channels from union result
fn extract_channels_from_result(data: &serde_json::Value) -> Result<Vec<serde_json::Value>> {
    match data["channels"]["__typename"].as_str() {
        Some("ChannelsList") => {
            let channels = data["channels"]["channels"]
                .as_array()
                .ok_or_else(|| anyhow::anyhow!("Expected channels array"))?
                .clone();
            Ok(channels)
        }
        Some("MissingFilterError") => {
            anyhow::bail!(
                "MissingFilterError: {}",
                data["channels"]["message"].as_str().unwrap_or("unknown")
            )
        }
        Some("QueryFailedError") => {
            anyhow::bail!(
                "QueryFailedError: {}",
                data["channels"]["message"].as_str().unwrap_or("unknown")
            )
        }
        Some(other) => anyhow::bail!("Unexpected typename: {}", other),
        None => anyhow::bail!("Missing __typename in response"),
    }
}

/// Helper to extract count from union result
fn extract_count_from_result(data: &serde_json::Value, query_name: &str) -> Result<i64> {
    match data[query_name]["__typename"].as_str() {
        Some("Count") => {
            let count = data[query_name]["count"]
                .as_i64()
                .ok_or_else(|| anyhow::anyhow!("Expected count value"))?;
            Ok(count)
        }
        Some("MissingFilterError") => {
            anyhow::bail!(
                "MissingFilterError: {}",
                data[query_name]["message"].as_str().unwrap_or("unknown")
            )
        }
        Some("QueryFailedError") => {
            anyhow::bail!(
                "QueryFailedError: {}",
                data[query_name]["message"].as_str().unwrap_or("unknown")
            )
        }
        Some(other) => anyhow::bail!("Unexpected typename: {}", other),
        None => anyhow::bail!("Missing __typename in response"),
    }
}

/// Helper to extract chainInfo from union result
fn extract_chain_info_from_result(data: &serde_json::Value) -> Result<serde_json::Value> {
    match data["chainInfo"]["__typename"].as_str() {
        Some("ChainInfo") => Ok(data["chainInfo"].clone()),
        Some("QueryFailedError") => {
            anyhow::bail!(
                "QueryFailedError: {}",
                data["chainInfo"]["message"].as_str().unwrap_or("unknown")
            )
        }
        Some(other) => anyhow::bail!("Unexpected typename: {}", other),
        None => anyhow::bail!("Missing __typename in response"),
    }
}

/// Helper to generate random keypair for testing
fn random_keypair() -> ChainKeypair {
    ChainKeypair::random()
}

/// Helper to generate random offchain keypair
fn random_offchain_keypair() -> OffchainKeypair {
    OffchainKeypair::random()
}

/// Execute a GraphQL query against the schema and return the response
async fn execute_graphql_query(
    schema: &async_graphql::Schema<QueryRoot, async_graphql::EmptyMutation, async_graphql::EmptySubscription>,
    query: &str,
) -> async_graphql::Response {
    schema.execute(query).await
}

/// Helper to setup test database with chain_info
async fn setup_chain_info(db: &BlokliDb) -> Result<()> {
    let chain_info = blokli_db_entity::chain_info::ActiveModel {
        id: Set(1),
        last_indexed_block: Set(1000),
        last_indexed_tx_index: Set(Some(0)),
        last_indexed_log_index: Set(Some(0)),
        ticket_price: Set(Some(vec![0u8; 12])),
        key_binding_fee: Set(Some(vec![0u8; 12])),
        min_incoming_ticket_win_prob: Set(0.5_f32),
        channels_dst: Set(Some(vec![1u8; 32])),
        ledger_dst: Set(Some(vec![2u8; 32])),
        safe_registry_dst: Set(Some(vec![3u8; 32])),
        channel_closure_grace_period: Set(Some(300)),
    };
    blokli_db_entity::chain_info::Entity::insert(chain_info)
        .exec(db.conn(TargetDb::Index))
        .await?;

    Ok(())
}

#[tokio::test]
async fn test_channels_query_by_source_keyid() -> Result<()> {
    let db = BlokliDb::new_in_memory().await?;

    let keypair1 = random_keypair();
    let keypair2 = random_keypair();
    let offchain1 = random_offchain_keypair();
    let offchain2 = random_offchain_keypair();
    let addr1 = keypair1.public().to_address();
    let addr2 = keypair2.public().to_address();

    db.upsert_account(None, 1, addr1, *offchain1.public(), None, 100, 0, 0)
        .await?;
    db.upsert_account(None, 2, addr2, *offchain2.public(), None, 101, 0, 0)
        .await?;

    let balance = HoprBalance::from_str("1000")?;
    let channel = ChannelEntry::new(addr1, addr2, balance, 0, ChannelStatus::Open, 1);
    db.upsert_channel(None, channel, 100, 0, 0).await?;

    let schema = async_graphql::Schema::build(
        QueryRoot,
        async_graphql::EmptyMutation,
        async_graphql::EmptySubscription,
    )
    .data(db.conn(TargetDb::Index).clone())
    .data(ContractAddresses::default())
    .data(100u64)
    .data("test".to_string())
    .finish();

    let query = r#"
        query {
            channels(sourceKeyId: 1) {
                __typename
                ... on ChannelsList {
                    channels {
                        source
                        destination
                        balance
                        status
                        epoch
                        ticketIndex
                    }
                }
                ... on MissingFilterError {
                    code
                    message
                }
                ... on QueryFailedError {
                    code
                    message
                }
            }
        }
    "#;

    let response = execute_graphql_query(&schema, query).await;
    let data: serde_json::Value = serde_json::from_str(&response.data.to_string())?;
    let channels = extract_channels_from_result(&data)?;

    assert_eq!(channels.len(), 1);
    assert_eq!(channels[0]["source"], 1);
    assert_eq!(channels[0]["destination"], 2);
    assert_eq!(channels[0]["status"], "OPEN");
    assert_eq!(channels[0]["epoch"], 1);

    Ok(())
}

#[tokio::test]
async fn test_channels_query_by_destination_keyid() -> Result<()> {
    let db = BlokliDb::new_in_memory().await?;

    let keypair1 = random_keypair();
    let keypair2 = random_keypair();
    let offchain1 = random_offchain_keypair();
    let offchain2 = random_offchain_keypair();
    let addr1 = keypair1.public().to_address();
    let addr2 = keypair2.public().to_address();

    db.upsert_account(None, 1, addr1, *offchain1.public(), None, 100, 0, 0)
        .await?;
    db.upsert_account(None, 2, addr2, *offchain2.public(), None, 101, 0, 0)
        .await?;

    let balance = HoprBalance::from_str("1000")?;
    let channel = ChannelEntry::new(addr1, addr2, balance, 0, ChannelStatus::Open, 1);
    db.upsert_channel(None, channel, 100, 0, 0).await?;

    let schema = async_graphql::Schema::build(
        QueryRoot,
        async_graphql::EmptyMutation,
        async_graphql::EmptySubscription,
    )
    .data(db.conn(TargetDb::Index).clone())
    .data(ContractAddresses::default())
    .data(100u64)
    .data("test".to_string())
    .finish();

    let query = r#"
        query {
            channels(destinationKeyId: 2) {
                __typename
                ... on ChannelsList {
                    channels {
                        source
                        destination
                        status
                    }
                }
                ... on MissingFilterError {
                    code
                    message
                }
                ... on QueryFailedError {
                    code
                    message
                }
            }
        }
    "#;

    let response = execute_graphql_query(&schema, query).await;
    let data: serde_json::Value = serde_json::from_str(&response.data.to_string())?;
    let channels = extract_channels_from_result(&data)?;

    assert_eq!(channels.len(), 1);
    assert_eq!(channels[0]["destination"], 2);

    Ok(())
}

#[tokio::test]
async fn test_channels_query_with_status_filter() -> Result<()> {
    let db = BlokliDb::new_in_memory().await?;

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
        .await?;
    db.upsert_account(None, 2, addr2, *offchain2.public(), None, 101, 0, 0)
        .await?;
    db.upsert_account(None, 3, addr3, *offchain3.public(), None, 102, 0, 0)
        .await?;

    // Open channel
    let balance1 = HoprBalance::from_str("1000")?;
    let channel1 = ChannelEntry::new(addr1, addr2, balance1, 0, ChannelStatus::Open, 1);
    db.upsert_channel(None, channel1, 100, 0, 0).await?;

    // Pending to close channel
    let balance2 = HoprBalance::from_str("2000")?;
    let channel2 = ChannelEntry::new(
        addr1,
        addr3,
        balance2,
        5,
        ChannelStatus::PendingToClose(SystemTime::now() + Duration::from_secs(1000)),
        2,
    );
    db.upsert_channel(None, channel2, 100, 0, 1).await?;

    let schema = async_graphql::Schema::build(
        QueryRoot,
        async_graphql::EmptyMutation,
        async_graphql::EmptySubscription,
    )
    .data(db.conn(TargetDb::Index).clone())
    .data(ContractAddresses::default())
    .data(100u64)
    .data("test".to_string())
    .finish();

    // Query for OPEN channels
    let query = r#"
        query {
            channels(sourceKeyId: 1, status: OPEN) {
                __typename
                ... on ChannelsList {
                    channels {
                        source
                        destination
                        status
                        closureTime
                    }
                }
                ... on MissingFilterError {
                    code
                    message
                }
                ... on QueryFailedError {
                    code
                    message
                }
            }
        }
    "#;

    let response = execute_graphql_query(&schema, query).await;
    let data: serde_json::Value = serde_json::from_str(&response.data.to_string())?;
    let channels = extract_channels_from_result(&data)?;
    assert_eq!(channels.len(), 1);
    assert_eq!(channels[0]["status"], "OPEN");
    assert!(channels[0]["closureTime"].is_null());

    // Query for PENDINGTOCLOSE channels
    let query2 = r#"
        query {
            channels(sourceKeyId: 1, status: PENDINGTOCLOSE) {
                __typename
                ... on ChannelsList {
                    channels {
                        status
                        closureTime
                    }
                }
                ... on MissingFilterError {
                    code
                    message
                }
                ... on QueryFailedError {
                    code
                    message
                }
            }
        }
    "#;

    let response = execute_graphql_query(&schema, query2).await;
    let data: serde_json::Value = serde_json::from_str(&response.data.to_string())?;
    let channels = extract_channels_from_result(&data)?;
    assert_eq!(channels.len(), 1);
    assert_eq!(channels[0]["status"], "PENDINGTOCLOSE");
    assert!(!channels[0]["closureTime"].is_null());

    Ok(())
}

#[tokio::test]
async fn test_channels_query_missing_filter_returns_error() -> Result<()> {
    let db = BlokliDb::new_in_memory().await?;

    let schema = async_graphql::Schema::build(
        QueryRoot,
        async_graphql::EmptyMutation,
        async_graphql::EmptySubscription,
    )
    .data(db.conn(TargetDb::Index).clone())
    .data(ContractAddresses::default())
    .data(100u64)
    .data("test".to_string())
    .finish();

    let query = r#"
        query {
            channels {
                __typename
                ... on ChannelsList {
                    channels {
                        source
                        destination
                    }
                }
                ... on MissingFilterError {
                    code
                    message
                }
                ... on QueryFailedError {
                    code
                    message
                }
            }
        }
    "#;

    let response = execute_graphql_query(&schema, query).await;
    let data: serde_json::Value = serde_json::from_str(&response.data.to_string())?;

    assert_eq!(data["channels"]["__typename"], "MissingFilterError");
    assert_eq!(data["channels"]["code"], "MISSING_FILTER");
    assert!(data["channels"]["message"]
        .as_str()
        .unwrap()
        .contains("At least one"));

    Ok(())
}

#[tokio::test]
async fn test_channel_count_with_filters() -> Result<()> {
    let db = BlokliDb::new_in_memory().await?;

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
        .await?;
    db.upsert_account(None, 2, addr2, *offchain2.public(), None, 101, 0, 0)
        .await?;
    db.upsert_account(None, 3, addr3, *offchain3.public(), None, 102, 0, 0)
        .await?;

    // Insert two channels from addr1
    let balance1 = HoprBalance::from_str("1000")?;
    let channel1 = ChannelEntry::new(addr1, addr2, balance1, 0, ChannelStatus::Open, 1);
    db.upsert_channel(None, channel1, 100, 0, 0).await?;

    let balance2 = HoprBalance::from_str("2000")?;
    let channel2 = ChannelEntry::new(addr1, addr3, balance2, 0, ChannelStatus::Open, 1);
    db.upsert_channel(None, channel2, 100, 0, 1).await?;

    let schema = async_graphql::Schema::build(
        QueryRoot,
        async_graphql::EmptyMutation,
        async_graphql::EmptySubscription,
    )
    .data(db.conn(TargetDb::Index).clone())
    .data(ContractAddresses::default())
    .data(100u64)
    .data("test".to_string())
    .finish();

    let query = r#"
        query {
            channelCount(sourceKeyId: 1) {
                __typename
                ... on Count {
                    count
                }
                ... on MissingFilterError {
                    code
                    message
                }
                ... on QueryFailedError {
                    code
                    message
                }
            }
        }
    "#;

    let response = execute_graphql_query(&schema, query).await;
    let data: serde_json::Value = serde_json::from_str(&response.data.to_string())?;
    let count = extract_count_from_result(&data, "channelCount")?;
    assert_eq!(count, 2);

    Ok(())
}

#[tokio::test]
async fn test_channel_count_all_channels() -> Result<()> {
    let db = BlokliDb::new_in_memory().await?;

    let keypair1 = random_keypair();
    let keypair2 = random_keypair();
    let offchain1 = random_offchain_keypair();
    let offchain2 = random_offchain_keypair();
    let addr1 = keypair1.public().to_address();
    let addr2 = keypair2.public().to_address();

    db.upsert_account(None, 1, addr1, *offchain1.public(), None, 100, 0, 0)
        .await?;
    db.upsert_account(None, 2, addr2, *offchain2.public(), None, 101, 0, 0)
        .await?;

    let balance = HoprBalance::from_str("1000")?;
    let channel = ChannelEntry::new(addr1, addr2, balance, 0, ChannelStatus::Open, 1);
    db.upsert_channel(None, channel, 100, 0, 0).await?;

    let schema = async_graphql::Schema::build(
        QueryRoot,
        async_graphql::EmptyMutation,
        async_graphql::EmptySubscription,
    )
    .data(db.conn(TargetDb::Index).clone())
    .data(ContractAddresses::default())
    .data(100u64)
    .data("test".to_string())
    .finish();

    let query = r#"
        query {
            channelCount {
                __typename
                ... on Count {
                    count
                }
                ... on MissingFilterError {
                    code
                    message
                }
                ... on QueryFailedError {
                    code
                    message
                }
            }
        }
    "#;

    let response = execute_graphql_query(&schema, query).await;
    let data: serde_json::Value = serde_json::from_str(&response.data.to_string())?;
    let count = extract_count_from_result(&data, "channelCount")?;
    assert_eq!(count, 1);

    Ok(())
}

#[tokio::test]
async fn test_chain_info_query() -> Result<()> {
    let db = BlokliDb::new_in_memory().await?;
    setup_chain_info(&db).await?;

    let schema = async_graphql::Schema::build(
        QueryRoot,
        async_graphql::EmptyMutation,
        async_graphql::EmptySubscription,
    )
    .data(db.conn(TargetDb::Index).clone())
    .data(ContractAddresses::default())
    .data(100u64)
    .data("anvil-localhost".to_string())
    .finish();

    let query = r#"
        query {
            chainInfo {
                __typename
                ... on ChainInfo {
                    blockNumber
                    chainId
                    network
                    ticketPrice
                    keyBindingFee
                    minTicketWinningProbability
                    channelDst
                    ledgerDst
                    safeRegistryDst
                    channelClosureGracePeriod
                }
                ... on QueryFailedError {
                    code
                    message
                }
            }
        }
    "#;

    let response = execute_graphql_query(&schema, query).await;
    let data: serde_json::Value = serde_json::from_str(&response.data.to_string())?;
    let chain_info = extract_chain_info_from_result(&data)?;

    assert_eq!(chain_info["blockNumber"], 1000);
    assert_eq!(chain_info["chainId"], 100);
    assert_eq!(chain_info["network"], "anvil-localhost");
    assert_eq!(chain_info["minTicketWinningProbability"], 0.5);
    assert_eq!(chain_info["channelClosureGracePeriod"], 300);
    assert!(!chain_info["channelDst"].is_null());
    assert!(!chain_info["ledgerDst"].is_null());
    assert!(!chain_info["safeRegistryDst"].is_null());

    Ok(())
}

#[tokio::test]
async fn test_channels_query_no_results() -> Result<()> {
    let db = BlokliDb::new_in_memory().await?;

    let keypair1 = random_keypair();
    let offchain1 = random_offchain_keypair();
    let addr1 = keypair1.public().to_address();

    db.upsert_account(None, 1, addr1, *offchain1.public(), None, 100, 0, 0)
        .await?;

    let schema = async_graphql::Schema::build(
        QueryRoot,
        async_graphql::EmptyMutation,
        async_graphql::EmptySubscription,
    )
    .data(db.conn(TargetDb::Index).clone())
    .data(ContractAddresses::default())
    .data(100u64)
    .data("test".to_string())
    .finish();

    let query = r#"
        query {
            channels(sourceKeyId: 999) {
                __typename
                ... on ChannelsList {
                    channels {
                        source
                    }
                }
                ... on MissingFilterError {
                    code
                    message
                }
                ... on QueryFailedError {
                    code
                    message
                }
            }
        }
    "#;

    let response = execute_graphql_query(&schema, query).await;
    let data: serde_json::Value = serde_json::from_str(&response.data.to_string())?;
    let channels = extract_channels_from_result(&data)?;

    assert_eq!(channels.len(), 0);

    Ok(())
}

#[tokio::test]
async fn test_channel_count_with_status_filter() -> Result<()> {
    let db = BlokliDb::new_in_memory().await?;

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
        .await?;
    db.upsert_account(None, 2, addr2, *offchain2.public(), None, 101, 0, 0)
        .await?;
    db.upsert_account(None, 3, addr3, *offchain3.public(), None, 102, 0, 0)
        .await?;

    // Open channel
    let balance1 = HoprBalance::from_str("1000")?;
    let channel1 = ChannelEntry::new(addr1, addr2, balance1, 0, ChannelStatus::Open, 1);
    db.upsert_channel(None, channel1, 100, 0, 0).await?;

    // Closed channel
    let balance2 = HoprBalance::from_str("0")?;
    let channel2 = ChannelEntry::new(addr1, addr3, balance2, 0, ChannelStatus::Closed, 1);
    db.upsert_channel(None, channel2, 100, 0, 1).await?;

    let schema = async_graphql::Schema::build(
        QueryRoot,
        async_graphql::EmptyMutation,
        async_graphql::EmptySubscription,
    )
    .data(db.conn(TargetDb::Index).clone())
    .data(ContractAddresses::default())
    .data(100u64)
    .data("test".to_string())
    .finish();

    // Count OPEN channels
    let query = r#"
        query {
            channelCount(status: OPEN) {
                __typename
                ... on Count {
                    count
                }
                ... on MissingFilterError {
                    code
                    message
                }
                ... on QueryFailedError {
                    code
                    message
                }
            }
        }
    "#;

    let response = execute_graphql_query(&schema, query).await;
    let data: serde_json::Value = serde_json::from_str(&response.data.to_string())?;
    let count = extract_count_from_result(&data, "channelCount")?;
    assert_eq!(count, 1);

    // Count CLOSED channels
    let query2 = r#"
        query {
            channelCount(status: CLOSED) {
                __typename
                ... on Count {
                    count
                }
                ... on MissingFilterError {
                    code
                    message
                }
                ... on QueryFailedError {
                    code
                    message
                }
            }
        }
    "#;

    let response = execute_graphql_query(&schema, query2).await;
    let data: serde_json::Value = serde_json::from_str(&response.data.to_string())?;
    let count = extract_count_from_result(&data, "channelCount")?;
    assert_eq!(count, 1);

    Ok(())
}
