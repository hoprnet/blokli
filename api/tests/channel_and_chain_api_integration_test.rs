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
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::{Duration, SystemTime},
};

use anyhow::Result;
use async_trait::async_trait;
use blokli_api::{
    cache::ChannelStatsCache,
    loader::channels_by_safe_loader,
    query::QueryRoot,
    schema::{
        ChainId, ChainTipTrackerContext, ConfirmationRule, ExpectedBlockTime, Finality, GasMultiplier, NetworkName,
    },
};
use blokli_chain_indexer::IndexerState;
use blokli_chain_rpc::tip::{
    ChainTipTracker, ConfirmationConfig, ConfirmationMode as ChainConfirmationMode, TipObservation, TipSource,
};
use blokli_chain_types::ContractAddresses;
use blokli_db::{
    BlokliDbGeneralModelOperations, TargetDb, accounts::BlokliDbAccountOperations, channels::BlokliDbChannelOperations,
    db::BlokliDb,
};
use blokli_db_entity::chain_info;
use hopr_types::{
    crypto::prelude::{ChainKeypair, Keypair, OffchainKeypair},
    internal::channels::{ChannelEntry, ChannelStatus},
    primitive::prelude::{Address, HoprBalance, ToHex},
};
use sea_orm::{ActiveModelTrait, Database, DatabaseConnection, EntityTrait, ModelTrait, Set};

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

/// Execute a GraphQL query against the schema and return the response
async fn execute_graphql_query(
    schema: &async_graphql::Schema<QueryRoot, async_graphql::EmptyMutation, async_graphql::EmptySubscription>,
    query: &str,
) -> async_graphql::Response {
    schema.execute(query).await
}

/// Tip source that always returns the same observation.
#[derive(Clone)]
struct FixedTipSource {
    observation: TipObservation,
}

#[async_trait]
impl TipSource for FixedTipSource {
    async fn observe(&self) -> blokli_chain_rpc::errors::Result<TipObservation> {
        Ok(self.observation)
    }
}

fn tip_tracker(observation: TipObservation) -> Arc<ChainTipTracker> {
    Arc::new(ChainTipTracker::new(
        Arc::new(FixedTipSource { observation }),
        ConfirmationConfig::default(),
        Duration::from_secs(1),
        Duration::from_secs(1),
    ))
}

fn tip_observation(latest: u64, safe: Option<u64>, finalized: Option<u64>) -> TipObservation {
    TipObservation {
        latest,
        latest_timestamp: Some(1_700_000_000 + latest),
        safe,
        finalized,
    }
}

async fn chain_info_schema(
    db: &BlokliDb,
    mode: ChainConfirmationMode,
    tracker: Arc<ChainTipTracker>,
) -> async_graphql::Schema<QueryRoot, async_graphql::EmptyMutation, async_graphql::EmptySubscription> {
    async_graphql::Schema::build(
        QueryRoot,
        async_graphql::EmptyMutation,
        async_graphql::EmptySubscription,
    )
    .data(db.conn(TargetDb::Index).clone())
    .data(ContractAddresses::default())
    .data(ChainId(100))
    .data(NetworkName("anvil-localhost".to_string()))
    .data(ExpectedBlockTime(1))
    .data(Finality(3))
    .data(ConfirmationRule(mode))
    .data(ChainTipTrackerContext(tracker))
    .data(GasMultiplier(1.0))
    .finish()
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
        min_incoming_ticket_win_prob: Set(0.5_f64),
        channels_dst: Set(Some(vec![1u8; 32])),
        ledger_dst: Set(Some(vec![2u8; 32])),
        safe_registry_dst: Set(Some(vec![3u8; 32])),
        channel_closure_grace_period: Set(Some(300)),
        last_verified_block: Set(None),
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
    let channel = build_channel_entry(addr1, addr2, balance, 0, ChannelStatus::Open, 1);
    db.upsert_channel(None, channel, 100, 0, 0).await?;
    eprintln!("Channel created successfully");

    let schema = async_graphql::Schema::build(
        QueryRoot,
        async_graphql::EmptyMutation,
        async_graphql::EmptySubscription,
    )
    .data(db.conn(TargetDb::Index).clone())
    .data(ContractAddresses::default())
    .data(ChainId(100))
    .data(NetworkName("test".to_string()))
    .data(GasMultiplier(1.0))
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
    let channel = build_channel_entry(addr1, addr2, balance, 0, ChannelStatus::Open, 1);
    db.upsert_channel(None, channel, 100, 0, 0).await?;

    let schema = async_graphql::Schema::build(
        QueryRoot,
        async_graphql::EmptyMutation,
        async_graphql::EmptySubscription,
    )
    .data(db.conn(TargetDb::Index).clone())
    .data(ContractAddresses::default())
    .data(ChainId(100))
    .data(NetworkName("test".to_string()))
    .data(GasMultiplier(1.0))
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
    let channel1 = build_channel_entry(addr1, addr2, balance1, 0, ChannelStatus::Open, 1);
    db.upsert_channel(None, channel1, 100, 0, 0).await?;

    // Pending to close channel
    let balance2 = HoprBalance::from_str("2000 wxHOPR")?;
    let channel2 = build_channel_entry(
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
    .data(ChainId(100))
    .data(NetworkName("test".to_string()))
    .data(GasMultiplier(1.0))
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
    .data(ChainId(100))
    .data(NetworkName("test".to_string()))
    .data(GasMultiplier(1.0))
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
    let channel1 = build_channel_entry(addr1, addr2, balance1, 0, ChannelStatus::Open, 1);
    db.upsert_channel(None, channel1, 100, 0, 0).await?;

    let balance2 = HoprBalance::from_str("2000 wxHOPR")?;
    let channel2 = build_channel_entry(addr1, addr3, balance2, 0, ChannelStatus::Open, 1);
    db.upsert_channel(None, channel2, 100, 0, 1).await?;

    let schema = async_graphql::Schema::build(
        QueryRoot,
        async_graphql::EmptyMutation,
        async_graphql::EmptySubscription,
    )
    .data(db.conn(TargetDb::Index).clone())
    .data(ContractAddresses::default())
    .data(ChainId(100))
    .data(NetworkName("test".to_string()))
    .data(GasMultiplier(1.0))
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
    let channel = build_channel_entry(addr1, addr2, balance, 0, ChannelStatus::Open, 1);
    db.upsert_channel(None, channel, 100, 0, 0).await?;

    let schema = async_graphql::Schema::build(
        QueryRoot,
        async_graphql::EmptyMutation,
        async_graphql::EmptySubscription,
    )
    .data(db.conn(TargetDb::Index).clone())
    .data(ContractAddresses::default())
    .data(ChainId(100))
    .data(NetworkName("test".to_string()))
    .data(GasMultiplier(1.0))
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
    .data(ChainId(100))
    .data(NetworkName("anvil-localhost".to_string()))
    .data(ExpectedBlockTime(1))
    .data(Finality(3))
    .data(ConfirmationRule(ChainConfirmationMode::Depth))
    .data(GasMultiplier(1.0))
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
                    gasPrice
                    maxFeePerGas
                    maxPriorityFeePerGas
                    minTicketWinningProbability
                    channelDst
                    ledgerDst
                    safeRegistryDst
                    channelClosureGracePeriod
                    expectedBlockTime
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
    // expectedBlockTime is UInt64, represented as a string
    assert_eq!(chain_info["expectedBlockTime"].as_str().unwrap(), "1");

    // Verify token values (0 balance represented as "0 wxHOPR")
    assert_eq!(chain_info["ticketPrice"].as_str().unwrap(), "0 wxHOPR");
    assert_eq!(chain_info["keyBindingFee"].as_str().unwrap(), "0 wxHOPR");
    assert!(chain_info["gasPrice"].is_null());
    assert!(chain_info["maxFeePerGas"].is_null());
    assert!(chain_info["maxPriorityFeePerGas"].is_null());

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
        "node_stake_factory",
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
async fn test_chain_info_reports_confirmation_mode_and_fcr_status() -> Result<()> {
    let db = BlokliDb::new_in_memory().await?;
    setup_chain_info(&db).await?;

    let query = r#"
        query {
            chainInfo {
                __typename
                ... on ChainInfo {
                    confirmationMode
                    fcrStatus
                }
                ... on QueryFailedError {
                    code
                    message
                }
            }
        }
    "#;

    let tracker = tip_tracker(tip_observation(1000, Some(999), Some(980)));
    let schema = chain_info_schema(&db, ChainConfirmationMode::SafeOrDepth, tracker.clone()).await;

    // No tip has been published yet.
    let response = execute_graphql_query(&schema, query).await;
    assert!(response.errors.is_empty(), "GraphQL errors: {:?}", response.errors);
    let data = response.data.into_json()?;
    let chain_info = extract_chain_info_from_result(&data)?;
    assert_eq!(chain_info["confirmationMode"], "SAFE_OR_DEPTH");
    assert_eq!(chain_info["fcrStatus"], "UNAVAILABLE");

    // The status follows the tracker without being frozen by a cached chain info value.
    tracker.refresh().await?;
    let response = execute_graphql_query(&schema, query).await;
    assert!(response.errors.is_empty(), "GraphQL errors: {:?}", response.errors);
    let data = response.data.into_json()?;
    let chain_info = extract_chain_info_from_result(&data)?;
    assert_eq!(chain_info["confirmationMode"], "SAFE_OR_DEPTH");
    assert_eq!(chain_info["fcrStatus"], "HEALTHY");

    let degraded_tracker = tip_tracker(tip_observation(1000, Some(960), Some(940)));
    degraded_tracker.refresh().await?;
    let degraded = chain_info_schema(&db, ChainConfirmationMode::Finalized, degraded_tracker).await;
    let response = execute_graphql_query(&degraded, query).await;
    assert!(response.errors.is_empty(), "GraphQL errors: {:?}", response.errors);
    let data = response.data.into_json()?;
    let chain_info = extract_chain_info_from_result(&data)?;
    assert_eq!(chain_info["confirmationMode"], "FINALIZED");
    assert_eq!(chain_info["fcrStatus"], "DEGRADED");

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
    .data(ChainId(100))
    .data(NetworkName("anvil-localhost".to_string()))
    .data(ExpectedBlockTime(1))
    .data(Finality(3))
    .data(ConfirmationRule(ChainConfirmationMode::Depth))
    .data(GasMultiplier(1.0))
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
        min_incoming_ticket_win_prob: Set(0.0_f64), // Zero value
        channels_dst: Set(None),                    // NULL
        ledger_dst: Set(None),                      // NULL
        safe_registry_dst: Set(None),               // NULL
        channel_closure_grace_period: Set(None),    // NULL
        last_verified_block: Set(None),
    };
    chain_info.update(db.conn(TargetDb::Index)).await?;

    let schema = async_graphql::Schema::build(
        QueryRoot,
        async_graphql::EmptyMutation,
        async_graphql::EmptySubscription,
    )
    .data(db.conn(TargetDb::Index).clone())
    .data(ContractAddresses::default())
    .data(ChainId(200))
    .data(NetworkName("test-network".to_string()))
    .data(ExpectedBlockTime(5))
    .data(Finality(3))
    .data(ConfirmationRule(ChainConfirmationMode::Depth))
    .data(GasMultiplier(1.0))
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
    .data(ChainId(100))
    .data(NetworkName("test".to_string()))
    .data(GasMultiplier(1.0))
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
    let channel1 = build_channel_entry(addr1, addr2, balance1, 0, ChannelStatus::Open, 1);
    db.upsert_channel(None, channel1, 100, 0, 0).await?;

    // Closed channel
    let balance2 = HoprBalance::from_str("0 wxHOPR")?;
    let channel2 = build_channel_entry(addr1, addr3, balance2, 0, ChannelStatus::Closed, 1);
    db.upsert_channel(None, channel2, 100, 0, 1).await?;

    let schema = async_graphql::Schema::build(
        QueryRoot,
        async_graphql::EmptyMutation,
        async_graphql::EmptySubscription,
    )
    .data(db.conn(TargetDb::Index).clone())
    .data(ContractAddresses::default())
    .data(ChainId(100))
    .data(NetworkName("test".to_string()))
    .data(GasMultiplier(1.0))
    .finish();

    // Count OPEN channels — should return 1
    let query = r#"
        query {
            channelCount(status: OPEN) {
                __typename
                ... on Count {
                    count
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
    assert_eq!(data["channelCount"]["__typename"], "Count");
    assert_eq!(data["channelCount"]["count"], 1);

    // Count CLOSED channels — should return 1
    let query2 = r#"
        query {
            channelCount(status: CLOSED) {
                __typename
                ... on Count {
                    count
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
    assert_eq!(data["channelCount"]["__typename"], "Count");
    assert_eq!(data["channelCount"]["count"], 1);

    // Combined filter: sourceKeyId=1, status=OPEN — should return 1 (channel1: addr1→addr2, open)
    let query3 = r#"
        query {
            channelCount(sourceKeyId: 1, status: OPEN) {
                __typename
                ... on Count {
                    count
                }
                ... on QueryFailedError {
                    code
                    message
                }
            }
        }
    "#;

    let response = execute_graphql_query(&schema, query3).await;
    assert!(response.errors.is_empty(), "GraphQL errors: {:?}", response.errors);

    let data = response.data.into_json()?;
    assert_eq!(data["channelCount"]["__typename"], "Count");
    assert_eq!(data["channelCount"]["count"], 1);

    // Combined filter: sourceKeyId=1, status=PENDINGTOCLOSE — should return 0
    let query4 = r#"
        query {
            channelCount(sourceKeyId: 1, status: PENDINGTOCLOSE) {
                __typename
                ... on Count {
                    count
                }
                ... on QueryFailedError {
                    code
                    message
                }
            }
        }
    "#;

    let response = execute_graphql_query(&schema, query4).await;
    assert!(response.errors.is_empty(), "GraphQL errors: {:?}", response.errors);

    let data = response.data.into_json()?;
    assert_eq!(data["channelCount"]["__typename"], "Count");
    assert_eq!(data["channelCount"]["count"], 0);

    Ok(())
}

/// Build a query schema that resolves `channels(safeAddress:)` through the batch loader
fn build_schema_with_channel_loader(
    conn: DatabaseConnection,
) -> async_graphql::Schema<QueryRoot, async_graphql::EmptyMutation, async_graphql::EmptySubscription> {
    async_graphql::Schema::build(
        QueryRoot,
        async_graphql::EmptyMutation,
        async_graphql::EmptySubscription,
    )
    .data(channels_by_safe_loader(conn.clone()))
    .data(conn)
    .data(ContractAddresses::default())
    .data(ChainId(100))
    .data(NetworkName("test".to_string()))
    .data(GasMultiplier(1.0))
    .finish()
}

/// Build one `channels` field with the given alias and arguments
fn aliased_channels_field(alias: &str, arguments: &str) -> String {
    format!(
        r#"{alias}: channels({arguments}) {{
            __typename
            ... on ChannelsList {{ channels {{ concreteChannelId source destination balance status epoch ticketIndex }} }}
            ... on MissingFilterError {{ code message }}
            ... on InvalidAddressError {{ code message }}
            ... on QueryFailedError {{ code message }}
        }}"#
    )
}

/// The channels of one alias, ordered by channel ID so that two results can be compared
fn sorted_channels(data: &serde_json::Value, alias: &str) -> Vec<serde_json::Value> {
    let mut channels = data[alias]["channels"].as_array().cloned().unwrap_or_default();
    channels.sort_by_key(|channel| channel["concreteChannelId"].as_str().map(str::to_string));
    channels
}

const SAFE_A: [u8; 20] = [0xa1; 20];
const SAFE_B: [u8; 20] = [0xb2; 20];

/// Create accounts 1 and 2 in safe A, account 3 in safe B and account 4 without a safe, with one
/// open channel from each of them
async fn create_safe_channels(db: &BlokliDb) -> Result<()> {
    let safes = [
        Some(Address::from(SAFE_A)),
        Some(Address::from(SAFE_A)),
        Some(Address::from(SAFE_B)),
        None,
    ];
    let mut chain_keys = Vec::new();
    for (keyid, safe) in (1u32..).zip(safes) {
        let chain_key = random_keypair().public().to_address();
        db.upsert_account(
            None,
            keyid,
            chain_key,
            *random_offchain_keypair().public(),
            safe,
            100,
            0,
            0,
        )
        .await?;
        chain_keys.push(chain_key);
    }

    let destinations = [chain_keys[3], chain_keys[3], chain_keys[3], chain_keys[0]];
    for (log_index, (source, destination)) in (0u32..).zip(chain_keys.iter().zip(destinations)) {
        let channel = build_channel_entry(*source, destination, HoprBalance::from(1000), 1, ChannelStatus::Open, 1);
        db.upsert_channel(None, channel, 100, 0, log_index).await?;
    }

    Ok(())
}

#[tokio::test]
async fn test_aliased_safe_channel_lookups_share_one_statement_pair() -> Result<()> {
    let db = BlokliDb::new_in_memory().await?;
    create_safe_channels(&db).await?;

    let statements = Arc::new(AtomicUsize::new(0));
    let mut conn = db.conn(TargetDb::Index).clone();
    conn.set_metric_callback({
        let statements = statements.clone();
        move |_| {
            statements.fetch_add(1, Ordering::SeqCst);
        }
    });
    let schema = build_schema_with_channel_loader(conn);

    let query = format!(
        "query {{ {} {} {} }}",
        aliased_channels_field("a", &format!(r#"safeAddress: "{}""#, Address::from(SAFE_A).to_hex())),
        aliased_channels_field("b", &format!(r#"safeAddress: "{}""#, Address::from(SAFE_B).to_hex())),
        aliased_channels_field(
            "unknown",
            &format!(r#"safeAddress: "{}""#, Address::from([0x42; 20]).to_hex())
        ),
    );

    let response = execute_graphql_query(&schema, &query).await;
    assert!(response.errors.is_empty(), "Errors: {:?}", response.errors);

    let data = response.data.into_json()?;
    let sources = |alias: &str| -> Vec<i64> {
        let mut sources: Vec<i64> = sorted_channels(&data, alias)
            .iter()
            .filter_map(|channel| channel["source"].as_i64())
            .collect();
        sources.sort_unstable();
        sources
    };
    assert_eq!(sources("a"), vec![1, 2]);
    assert_eq!(sources("b"), vec![3]);
    assert_eq!(data["unknown"]["__typename"], "ChannelsList");
    assert!(sources("unknown").is_empty());

    // One statement for the accounts of the safes and one for their channels, however many aliases
    assert_eq!(statements.load(Ordering::SeqCst), 2);

    Ok(())
}

#[tokio::test]
async fn test_aliased_safe_channel_lookups_match_unbatched_results() -> Result<()> {
    let db = BlokliDb::new_in_memory().await?;
    create_safe_channels(&db).await?;
    let schema = build_schema_with_channel_loader(db.conn(TargetDb::Index).clone());

    // A lookup with a second filter does not go through the batch loader. Every channel is open,
    // so the status filter does not change the result.
    let mut fields = Vec::new();
    for (name, safe) in [("a", SAFE_A), ("b", SAFE_B)] {
        let safe = Address::from(safe).to_hex();
        fields.push(aliased_channels_field(
            &format!("batched_{name}"),
            &format!(r#"safeAddress: "{safe}""#),
        ));
        fields.push(aliased_channels_field(
            &format!("unbatched_{name}"),
            &format!(r#"safeAddress: "{safe}", status: OPEN"#),
        ));
    }
    let query = format!("query {{ {} }}", fields.join("\n"));

    let response = execute_graphql_query(&schema, &query).await;
    assert!(response.errors.is_empty(), "Errors: {:?}", response.errors);

    let data = response.data.into_json()?;
    for (name, expected) in [("a", 2), ("b", 1)] {
        let batched = sorted_channels(&data, &format!("batched_{name}"));
        assert_eq!(batched.len(), expected, "safe {name}");
        assert_eq!(
            batched,
            sorted_channels(&data, &format!("unbatched_{name}")),
            "safe {name}"
        );
    }

    Ok(())
}

#[tokio::test]
async fn test_invalid_safe_address_alias_does_not_fail_other_aliases() -> Result<()> {
    let db = BlokliDb::new_in_memory().await?;
    create_safe_channels(&db).await?;
    let schema = build_schema_with_channel_loader(db.conn(TargetDb::Index).clone());

    let query = format!(
        "query {{ {} {} }}",
        aliased_channels_field(
            "valid",
            &format!(r#"safeAddress: "{}""#, Address::from(SAFE_B).to_hex())
        ),
        aliased_channels_field("invalid", r#"safeAddress: "notvalidhex""#),
    );

    let response = execute_graphql_query(&schema, &query).await;
    assert!(response.errors.is_empty(), "Errors: {:?}", response.errors);

    let data = response.data.into_json()?;
    assert_eq!(sorted_channels(&data, "valid").len(), 1);
    assert_eq!(data["invalid"]["__typename"], "InvalidAddressError");

    Ok(())
}

#[tokio::test]
async fn test_failed_safe_channel_batch_fails_every_alias_in_it() -> Result<()> {
    // A database without the schema makes the batch statement fail
    let conn = Database::connect("sqlite::memory:").await?;
    let schema = build_schema_with_channel_loader(conn);

    let query = format!(
        "query {{ {} {} }}",
        aliased_channels_field("a", &format!(r#"safeAddress: "{}""#, Address::from(SAFE_A).to_hex())),
        aliased_channels_field("b", &format!(r#"safeAddress: "{}""#, Address::from(SAFE_B).to_hex())),
    );

    let response = execute_graphql_query(&schema, &query).await;
    assert!(response.errors.is_empty(), "Errors: {:?}", response.errors);

    let data = response.data.into_json()?;
    for alias in ["a", "b"] {
        assert_eq!(data[alias]["__typename"], "QueryFailedError", "alias {alias}");
        assert_eq!(data[alias]["code"], "QUERY_FAILED", "alias {alias}");
    }

    Ok(())
}

#[tokio::test]
async fn test_safe_channel_lookup_without_loader_reports_context_error() -> Result<()> {
    let db = BlokliDb::new_in_memory().await?;
    let schema = async_graphql::Schema::build(
        QueryRoot,
        async_graphql::EmptyMutation,
        async_graphql::EmptySubscription,
    )
    .data(db.conn(TargetDb::Index).clone())
    .finish();

    let query = format!(
        "query {{ {} }}",
        aliased_channels_field("a", &format!(r#"safeAddress: "{}""#, Address::from(SAFE_A).to_hex())),
    );

    let response = execute_graphql_query(&schema, &query).await;
    assert!(response.errors.is_empty(), "Errors: {:?}", response.errors);

    let data = response.data.into_json()?;
    assert_eq!(data["a"]["__typename"], "QueryFailedError");
    assert_eq!(data["a"]["code"], "CONTEXT_ERROR");

    Ok(())
}

#[tokio::test]
async fn test_channel_stats_cached_until_reorganization_advances_epoch() -> Result<()> {
    let db = BlokliDb::new_in_memory().await?;
    setup_chain_info(&db).await?;

    let keypair1 = random_keypair();
    let keypair2 = random_keypair();
    let keypair3 = random_keypair();
    let addr1 = keypair1.public().to_address();
    let addr2 = keypair2.public().to_address();
    let addr3 = keypair3.public().to_address();
    for (id, addr) in [(1, addr1), (2, addr2), (3, addr3)] {
        db.upsert_account(None, id, addr, *random_offchain_keypair().public(), None, 100, 0, 0)
            .await?;
    }

    let balance = HoprBalance::from_str("1000 wxHOPR")?;
    db.upsert_channel(
        None,
        build_channel_entry(addr1, addr2, balance, 0, ChannelStatus::Open, 1),
        100,
        0,
        0,
    )
    .await?;

    let indexer_state = IndexerState::new(10, 100);
    let schema = async_graphql::Schema::build(
        QueryRoot,
        async_graphql::EmptyMutation,
        async_graphql::EmptySubscription,
    )
    .data(db.conn(TargetDb::Index).clone())
    .data(ChannelStatsCache::default())
    .data(indexer_state.clone())
    .finish();

    let all = "query { channelStats { __typename ... on ChannelStats { count balance } } }";
    let count_of = |data: &serde_json::Value| data["channelStats"]["count"].as_i64();

    let first = execute_graphql_query(&schema, all).await.data.into_json()?;
    assert_eq!(count_of(&first), Some(1));

    // A second channel is written without the indexed block advancing: the cached value is served.
    db.upsert_channel(
        None,
        build_channel_entry(addr2, addr3, balance, 0, ChannelStatus::Open, 1),
        101,
        0,
        0,
    )
    .await?;
    let stale = execute_graphql_query(&schema, all).await.data.into_json()?;
    assert_eq!(count_of(&stale), Some(1));

    // A reorganization rewrites channel rows while the indexed block stays where it was.
    indexer_state.signal_shutdown();
    let fresh = execute_graphql_query(&schema, all).await.data.into_json()?;
    assert_eq!(count_of(&fresh), Some(2));
    assert_eq!(fresh["channelStats"]["balance"], "2000 wxHOPR");

    Ok(())
}

#[tokio::test]
async fn test_channel_stats_cached_until_indexed_block_advances() -> Result<()> {
    let db = BlokliDb::new_in_memory().await?;
    setup_chain_info(&db).await?;

    let keypair1 = random_keypair();
    let keypair2 = random_keypair();
    let keypair3 = random_keypair();
    let addr1 = keypair1.public().to_address();
    let addr2 = keypair2.public().to_address();
    let addr3 = keypair3.public().to_address();
    for (id, addr) in [(1, addr1), (2, addr2), (3, addr3)] {
        db.upsert_account(None, id, addr, *random_offchain_keypair().public(), None, 100, 0, 0)
            .await?;
    }

    let balance = HoprBalance::from_str("1000 wxHOPR")?;
    db.upsert_channel(
        None,
        build_channel_entry(addr1, addr2, balance, 0, ChannelStatus::Open, 1),
        100,
        0,
        0,
    )
    .await?;

    let schema = async_graphql::Schema::build(
        QueryRoot,
        async_graphql::EmptyMutation,
        async_graphql::EmptySubscription,
    )
    .data(db.conn(TargetDb::Index).clone())
    .data(ChannelStatsCache::default())
    .finish();

    let all = "query { channelStats { __typename ... on ChannelStats { count balance } } }";
    let open = "query { channelStats(status: OPEN) { __typename ... on ChannelStats { count balance } } }";
    let count_of = |data: &serde_json::Value| data["channelStats"]["count"].as_i64();

    let first = execute_graphql_query(&schema, all).await.data.into_json()?;
    assert_eq!(count_of(&first), Some(1));
    assert_eq!(first["channelStats"]["balance"], "1000 wxHOPR");

    // A second channel is written without the indexed block advancing: the cached value is served.
    db.upsert_channel(
        None,
        build_channel_entry(addr2, addr3, balance, 0, ChannelStatus::Open, 1),
        101,
        0,
        0,
    )
    .await?;
    let stale = execute_graphql_query(&schema, all).await.data.into_json()?;
    assert_eq!(count_of(&stale), Some(1));

    // A different filter tuple is its own entry and is computed fresh.
    let other = execute_graphql_query(&schema, open).await.data.into_json()?;
    assert_eq!(count_of(&other), Some(2));

    // Advancing the indexed block invalidates the cached value.
    chain_info::ActiveModel {
        id: Set(1),
        last_indexed_block: Set(1001),
        ..Default::default()
    }
    .update(db.conn(TargetDb::Index))
    .await?;
    let fresh = execute_graphql_query(&schema, all).await.data.into_json()?;
    assert_eq!(count_of(&fresh), Some(2));
    assert_eq!(fresh["channelStats"]["balance"], "2000 wxHOPR");

    Ok(())
}
