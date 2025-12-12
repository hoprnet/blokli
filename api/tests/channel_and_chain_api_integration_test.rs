//! Comprehensive integration tests for channels, channelCount, and chainInfo GraphQL queries
//!
//! These tests verify:
//! - Union type handling (ChannelsList, ChainInfo vs. errors)
//! - Required filter validation (channels query)
//! - Optional filter handling (channelCount query)
//! - Type conversions between database models and GraphQL types
//! - Filter combinations (sourceKeyId, destinationKeyId, concreteChannelId, status)

use std::{
    str::FromStr,
    time::{Duration, SystemTime},
};

use anyhow::Result;
use blokli_api::query::QueryRoot;
use blokli_chain_types::ContractAddresses;
use blokli_db::{
    BlokliDbGeneralModelOperations, TargetDb, accounts::BlokliDbAccountOperations, channels::BlokliDbChannelOperations,
    db::BlokliDb,
};
use blokli_db_entity::chain_info;
use hopr_crypto_types::prelude::{ChainKeypair, Keypair, OffchainKeypair};
use hopr_internal_types::channels::{ChannelEntry, ChannelStatus};
use hopr_primitive_types::prelude::HoprBalance;
use sea_orm::{ActiveModelTrait, EntityTrait, ModelTrait, Set};

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
    // Update the existing chain_info row (id=1 is created by BlokliDb::new_in_memory())
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
    chain_info.update(db.conn(TargetDb::Index)).await?;

    Ok(())
}

#[test_log::test(tokio::test)]
async fn test_channels_query_by_source_keyid() -> Result<()> {
    let db = BlokliDb::new_in_memory().await?;

    let keypair1 = random_keypair();
    let keypair2 = random_keypair();
    let offchain1 = random_offchain_keypair();
    let offchain2 = random_offchain_keypair();
    let addr1 = keypair1.public().to_address();
    let addr2 = keypair2.public().to_address();

    eprintln!("Creating accounts with addr1={:?}, addr2={:?}", addr1, addr2);
    db.upsert_account(None, 1, addr1, *offchain1.public(), None, 100, 0, 0)
        .await?;
    db.upsert_account(None, 2, addr2, *offchain2.public(), None, 101, 0, 0)
        .await?;

    let balance = HoprBalance::from_str("1000 wxHOPR")?;
    eprintln!("Creating channel with balance={:?}", balance);
    let channel = ChannelEntry::new(addr1, addr2, balance, 0, ChannelStatus::Open, 1);
    db.upsert_channel(None, channel, 100, 0, 0).await?;
    eprintln!("Channel created successfully");

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

    // Debug: Print the response
    eprintln!("Response errors: {:?}", response.errors);
    eprintln!("Response data: {}", response.data);

    assert!(response.errors.is_empty(), "GraphQL errors: {:?}", response.errors);

    let data = response.data.into_json()?;
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

    let balance = HoprBalance::from_str("1000 wxHOPR")?;
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
    assert!(response.errors.is_empty(), "GraphQL errors: {:?}", response.errors);

    let data = response.data.into_json()?;
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
    let balance1 = HoprBalance::from_str("1000 wxHOPR")?;
    let channel1 = ChannelEntry::new(addr1, addr2, balance1, 0, ChannelStatus::Open, 1);
    db.upsert_channel(None, channel1, 100, 0, 0).await?;

    // Pending to close channel
    let balance2 = HoprBalance::from_str("2000 wxHOPR")?;
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
    assert!(response.errors.is_empty(), "GraphQL errors: {:?}", response.errors);

    let data = response.data.into_json()?;
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
    assert!(response.errors.is_empty(), "GraphQL errors: {:?}", response.errors);

    let data = response.data.into_json()?;
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
    assert!(response.errors.is_empty(), "GraphQL errors: {:?}", response.errors);

    let data = response.data.into_json()?;

    assert_eq!(data["channels"]["__typename"], "MissingFilterError");
    assert_eq!(data["channels"]["code"], "MISSING_FILTER");
    assert!(
        data["channels"]["message"]
            .as_str()
            .unwrap()
            .contains("Missing required filter")
    );

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
    let balance1 = HoprBalance::from_str("1000 wxHOPR")?;
    let channel1 = ChannelEntry::new(addr1, addr2, balance1, 0, ChannelStatus::Open, 1);
    db.upsert_channel(None, channel1, 100, 0, 0).await?;

    let balance2 = HoprBalance::from_str("2000 wxHOPR")?;
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
    assert!(response.errors.is_empty(), "GraphQL errors: {:?}", response.errors);

    let data = response.data.into_json()?;
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

    let balance = HoprBalance::from_str("1000 wxHOPR")?;
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
    assert!(response.errors.is_empty(), "GraphQL errors: {:?}", response.errors);

    let data = response.data.into_json()?;
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
                    contractAddresses
                }
                ... on QueryFailedError {
                    code
                    message
                }
            }
        }
    "#;

    let response = execute_graphql_query(&schema, query).await;
    assert!(response.errors.is_empty(), "GraphQL errors: {:?}", response.errors);

    let data = response.data.into_json()?;
    let chain_info = extract_chain_info_from_result(&data)?;

    // Verify scalar fields
    assert_eq!(chain_info["blockNumber"], 1000);
    assert_eq!(chain_info["chainId"], 100);
    assert_eq!(chain_info["network"], "anvil-localhost");
    assert_eq!(chain_info["minTicketWinningProbability"], 0.5);
    // channelClosureGracePeriod is now UInt64, which is represented as a string
    assert_eq!(chain_info["channelClosureGracePeriod"].as_str().unwrap(), "300");

    // Verify token values (0 balance represented as "0 wxHOPR")
    assert_eq!(chain_info["ticketPrice"].as_str().unwrap(), "0 wxHOPR");
    assert_eq!(chain_info["keyBindingFee"].as_str().unwrap(), "0 wxHOPR");

    // Verify domain separators are non-null hex strings
    assert!(!chain_info["channelDst"].is_null());
    assert!(!chain_info["ledgerDst"].is_null());
    assert!(!chain_info["safeRegistryDst"].is_null());

    // Verify domain separator format (should be 66 hex characters with 0x prefix)
    let channel_dst = chain_info["channelDst"].as_str().unwrap();
    let ledger_dst = chain_info["ledgerDst"].as_str().unwrap();
    let safe_registry_dst = chain_info["safeRegistryDst"].as_str().unwrap();
    assert_eq!(channel_dst.len(), 66, "channelDst should be 66 chars (0x + 64 hex)");
    assert_eq!(ledger_dst.len(), 66, "ledgerDst should be 66 chars (0x + 64 hex)");
    assert_eq!(
        safe_registry_dst.len(),
        66,
        "safeRegistryDst should be 66 chars (0x + 64 hex)"
    );

    // Verify all hex strings have 0x prefix and contain only valid hex characters
    assert!(channel_dst.starts_with("0x"));
    assert!(ledger_dst.starts_with("0x"));
    assert!(safe_registry_dst.starts_with("0x"));
    assert!(channel_dst[2..].chars().all(|c| c.is_ascii_hexdigit()));
    assert!(ledger_dst[2..].chars().all(|c| c.is_ascii_hexdigit()));
    assert!(safe_registry_dst[2..].chars().all(|c| c.is_ascii_hexdigit()));

    // Verify contractAddresses contains all required keys
    // contractAddresses is a stringified JSON object
    let contract_addresses_str = chain_info["contractAddresses"]
        .as_str()
        .expect("contractAddresses should be a string");
    let contract_addresses: serde_json::Map<String, serde_json::Value> =
        serde_json::from_str(contract_addresses_str).expect("contractAddresses should be valid JSON");

    let expected_keys = vec![
        "token",
        "channels",
        "announcements",
        "module_implementation",
        "node_safe_migration",
        "node_safe_registry",
        "ticket_price_oracle",
        "winning_probability_oracle",
        "node_stake_v2_factory",
    ];

    for key in expected_keys {
        assert!(
            contract_addresses.contains_key(key),
            "contractAddresses missing key: {}",
            key
        );
        // Verify each address is a non-empty string
        assert!(
            !contract_addresses[key].as_str().unwrap().is_empty(),
            "contractAddresses[{}] should not be empty",
            key
        );
    }

    Ok(())
}

#[tokio::test]
async fn test_chain_info_query_missing_data_returns_error() -> Result<()> {
    // Create empty database without setting up chain_info
    let db = BlokliDb::new_in_memory().await?;

    // Delete the default chain_info row that BlokliDb::new_in_memory() creates
    let chain_info_row = chain_info::Entity::find_by_id(1)
        .one(db.conn(TargetDb::Index))
        .await?
        .expect("Default chain_info row should exist");
    chain_info_row.delete(db.conn(TargetDb::Index)).await?;

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
                }
                ... on QueryFailedError {
                    code
                    message
                }
            }
        }
    "#;

    let response = execute_graphql_query(&schema, query).await;
    assert!(response.errors.is_empty(), "GraphQL errors: {:?}", response.errors);

    let data = response.data.into_json()?;

    // Verify we got QueryFailedError
    assert_eq!(data["chainInfo"]["__typename"], "QueryFailedError");
    assert_eq!(data["chainInfo"]["code"], "NOT_FOUND");

    let error_message = data["chainInfo"]["message"].as_str().unwrap();
    assert!(
        error_message.to_lowercase().contains("chain info"),
        "Error message should mention chain info: {}",
        error_message
    );

    Ok(())
}

#[tokio::test]
async fn test_chain_info_query_with_null_optional_fields() -> Result<()> {
    let db = BlokliDb::new_in_memory().await?;

    // Update chain_info with all optional fields set to NULL
    let chain_info = blokli_db_entity::chain_info::ActiveModel {
        id: Set(1),
        last_indexed_block: Set(500),
        last_indexed_tx_index: Set(Some(0)),
        last_indexed_log_index: Set(Some(0)),
        ticket_price: Set(None),                    // NULL
        key_binding_fee: Set(None),                 // NULL
        min_incoming_ticket_win_prob: Set(0.0_f32), // Zero value
        channels_dst: Set(None),                    // NULL
        ledger_dst: Set(None),                      // NULL
        safe_registry_dst: Set(None),               // NULL
        channel_closure_grace_period: Set(None),    // NULL
    };
    chain_info.update(db.conn(TargetDb::Index)).await?;

    let schema = async_graphql::Schema::build(
        QueryRoot,
        async_graphql::EmptyMutation,
        async_graphql::EmptySubscription,
    )
    .data(db.conn(TargetDb::Index).clone())
    .data(ContractAddresses::default())
    .data(200u64)
    .data("test-network".to_string())
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
    assert!(response.errors.is_empty(), "GraphQL errors: {:?}", response.errors);

    let data = response.data.into_json()?;
    let chain_info = extract_chain_info_from_result(&data)?;

    // Verify required fields are present
    assert_eq!(chain_info["blockNumber"], 500);
    assert_eq!(chain_info["chainId"], 200);
    assert_eq!(chain_info["network"], "test-network");
    assert_eq!(chain_info["minTicketWinningProbability"], 0.0);

    // Verify optional token values default to "0 wxHOPR" when not set (not null)
    assert_eq!(chain_info["ticketPrice"].as_str().unwrap(), "0 wxHOPR");
    assert_eq!(chain_info["keyBindingFee"].as_str().unwrap(), "0 wxHOPR");

    // Verify optional domain separators are null when not set
    assert!(chain_info["channelDst"].is_null(), "channelDst should be null");
    assert!(chain_info["ledgerDst"].is_null(), "ledgerDst should be null");
    assert!(
        chain_info["safeRegistryDst"].is_null(),
        "safeRegistryDst should be null"
    );

    // channelClosureGracePeriod should always be non-null with default value of 300
    assert_eq!(
        chain_info["channelClosureGracePeriod"].as_str().unwrap(),
        "300",
        "channelClosureGracePeriod should default to 300 when not explicitly set"
    );

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
    assert!(response.errors.is_empty(), "GraphQL errors: {:?}", response.errors);

    let data = response.data.into_json()?;
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
    let balance1 = HoprBalance::from_str("1000 wxHOPR")?;
    let channel1 = ChannelEntry::new(addr1, addr2, balance1, 0, ChannelStatus::Open, 1);
    db.upsert_channel(None, channel1, 100, 0, 0).await?;

    // Closed channel
    let balance2 = HoprBalance::from_str("0 wxHOPR")?;
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

    // Count OPEN channels (currently not implemented - expect error)
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
    assert!(response.errors.is_empty(), "GraphQL errors: {:?}", response.errors);

    let data = response.data.into_json()?;

    // Status filtering is not yet implemented, expect QueryFailedError
    assert_eq!(data["channelCount"]["__typename"], "QueryFailedError");
    assert_eq!(data["channelCount"]["code"], "NOT_IMPLEMENTED");

    // Count CLOSED channels (currently not implemented - expect error)
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
    assert!(response.errors.is_empty(), "GraphQL errors: {:?}", response.errors);

    let data = response.data.into_json()?;

    // Status filtering is not yet implemented, expect QueryFailedError
    assert_eq!(data["channelCount"]["__typename"], "QueryFailedError");
    assert_eq!(data["channelCount"]["code"], "NOT_IMPLEMENTED");

    Ok(())
}
