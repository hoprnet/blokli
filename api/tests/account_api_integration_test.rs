//! Integration tests for account API queries (accounts and accountCount)
//!
//! These tests verify end-to-end functionality by:
//! - Creating test accounts in an in-memory database
//! - Executing GraphQL queries against the schema
//! - Verifying filtering, validation, and type conversions

use blokli_api::query::QueryRoot;
use blokli_chain_types::ContractAddresses;
use blokli_db::{BlokliDbGeneralModelOperations, TargetDb, accounts::BlokliDbAccountOperations, db::BlokliDb};
use blokli_db_entity::prelude::Account as AccountEntity;
use hopr_crypto_types::prelude::{ChainKeypair, Keypair, OffchainKeypair};
use hopr_primitive_types::{prelude::Address, traits::ToHex};
use multiaddr::Multiaddr;
use sea_orm::{EntityTrait, PaginatorTrait};

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

/// Helper to extract accounts from union result
fn extract_accounts_from_result(data: &serde_json::Value) -> anyhow::Result<Vec<serde_json::Value>> {
    // Check the __typename to determine which variant we got
    match data["accounts"]["__typename"].as_str() {
        Some("AccountsList") => {
            let accounts = data["accounts"]["accounts"]
                .as_array()
                .ok_or_else(|| anyhow::anyhow!("Expected accounts array"))?
                .clone();
            Ok(accounts)
        }
        Some("MissingFilterError") => {
            anyhow::bail!(
                "MissingFilterError: {}",
                data["accounts"]["message"].as_str().unwrap_or("unknown")
            )
        }
        Some("QueryFailedError") => {
            anyhow::bail!(
                "QueryFailedError: {}",
                data["accounts"]["message"].as_str().unwrap_or("unknown")
            )
        }
        Some(other) => anyhow::bail!("Unexpected typename: {}", other),
        None => anyhow::bail!("Missing __typename in response"),
    }
}

/// Helper to extract count from union result
fn extract_count_from_result(data: &serde_json::Value, query_name: &str) -> anyhow::Result<i64> {
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

#[tokio::test]
async fn test_accounts_query_with_filters() -> anyhow::Result<()> {
    let db = BlokliDb::new_in_memory().await?;

    // Create test accounts with different keys
    let keypair1 = random_keypair();
    let offchain1 = random_offchain_keypair();
    let chain_key1 = keypair1.public().to_address();
    let packet_key1 = offchain1.public();

    let keypair2 = random_keypair();
    let offchain2 = random_offchain_keypair();
    let chain_key2 = keypair2.public().to_address();
    let packet_key2 = offchain2.public();

    // Create account 1 with safe address
    let safe_address1 = Address::from([0x01; 20]);
    db.upsert_account(None, 1, chain_key1, *packet_key1, Some(safe_address1), 100, 0, 0)
        .await?;

    // Create account 2 without safe address
    db.upsert_account(None, 2, chain_key2, *packet_key2, None, 101, 0, 0)
        .await?;

    // Verify DB has the records
    let count = AccountEntity::find().count(db.conn(TargetDb::Index)).await?;
    assert_eq!(count, 2, "DB should have 2 accounts");

    let schema = async_graphql::Schema::build(
        QueryRoot,
        async_graphql::EmptyMutation,
        async_graphql::EmptySubscription,
    )
    .data(db.conn(TargetDb::Index).clone())
    .data(ContractAddresses::default())
    .data(100u64) // chain_id
    .data("test".to_string()) // network
    .finish();

    // Test 1: Query by keyid
    {
        let query = r#"
            query {
                accounts(keyid: 1) {
                    __typename
                    ... on AccountsList {
                        accounts {
                            keyid
                            chainKey
                            packetKey
                            safeAddress
                            multiAddresses
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
        assert!(response.errors.is_empty(), "Errors: {:?}", response.errors);

        let data = response.data.into_json()?;
        let accounts = extract_accounts_from_result(&data)?;
        assert_eq!(accounts.len(), 1);
        assert_eq!(accounts[0]["keyid"], 1);
        assert_eq!(accounts[0]["chainKey"], chain_key1.to_hex());
        assert!(
            packet_key1
                .to_hex()
                .contains(accounts[0]["packetKey"].as_str().unwrap())
        );
        assert_eq!(accounts[0]["safeAddress"], safe_address1.to_hex());
    }

    // Test 2: Query by packet key
    {
        let query = format!(
            r#"
            query {{
                accounts(packetKey: "{}") {{
                    __typename
                    ... on AccountsList {{
                        accounts {{
                            keyid
                            chainKey
                            packetKey
                        }}
                    }}
                    ... on MissingFilterError {{ code message }}
                    ... on QueryFailedError {{ code message }}
                }}
            }}
        "#,
            packet_key2.to_hex()
        );

        let response = execute_graphql_query(&schema, &query).await;
        assert!(response.errors.is_empty());

        let data = response.data.into_json()?;
        let accounts = extract_accounts_from_result(&data)?;
        assert_eq!(accounts.len(), 1);
        assert_eq!(accounts[0]["keyid"], 2);
        assert_eq!(accounts[0]["chainKey"], chain_key2.to_hex());
        assert!(
            packet_key2
                .to_hex()
                .contains(accounts[0]["packetKey"].as_str().unwrap())
        );
    }

    // Test 3: Query by chain key
    {
        let query = format!(
            r#"
            query {{
                accounts(chainKey: "{}") {{
                    __typename
                    ... on AccountsList {{
                        accounts {{
                            keyid
                            chainKey
                            safeAddress
                        }}
                    }}
                    ... on MissingFilterError {{ code message }}
                    ... on QueryFailedError {{ code message }}
                }}
            }}
        "#,
            chain_key1.to_hex()
        );

        let response = execute_graphql_query(&schema, &query).await;
        assert!(response.errors.is_empty());

        let data = response.data.into_json()?;
        let accounts = extract_accounts_from_result(&data)?;
        assert_eq!(accounts.len(), 1);
        assert_eq!(accounts[0]["keyid"], 1);
        assert_eq!(accounts[0]["safeAddress"], safe_address1.to_hex());
    }

    Ok(())
}

#[tokio::test]
async fn test_accounts_query_requires_filter() -> anyhow::Result<()> {
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

    // Test: Query without any filters should return MissingFilterError variant
    let query = r#"
        query {
            accounts {
                __typename
                ... on AccountsList {
                    accounts { keyid }
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
    assert!(response.errors.is_empty(), "Should not have GraphQL errors");

    let data = response.data.into_json()?;
    assert_eq!(
        data["accounts"]["__typename"].as_str(),
        Some("MissingFilterError"),
        "Should return MissingFilterError variant"
    );
    assert_eq!(data["accounts"]["code"].as_str(), Some("MISSING_FILTER"));
    assert!(
        data["accounts"]["message"]
            .as_str()
            .unwrap()
            .contains("Missing required filter")
    );

    Ok(())
}

#[tokio::test]
async fn test_accounts_query_invalid_chain_key() -> anyhow::Result<()> {
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

    // Test: Query with invalid chain key should return QueryFailedError
    let query = r#"
        query {
            accounts(chainKey: "notvalidhex") {
                __typename
                ... on AccountsList {
                    accounts { keyid }
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
    assert!(response.errors.is_empty(), "Should not have GraphQL errors");

    let data = response.data.into_json()?;
    assert_eq!(
        data["accounts"]["__typename"].as_str(),
        Some("QueryFailedError"),
        "Should return QueryFailedError variant for invalid address"
    );
    assert_eq!(data["accounts"]["code"].as_str(), Some("INVALID_ADDRESS"));
    assert!(data["accounts"]["message"].as_str().unwrap().contains("Invalid"));

    Ok(())
}

#[tokio::test]
async fn test_accounts_query_combined_filters() -> anyhow::Result<()> {
    let db = BlokliDb::new_in_memory().await?;

    // Create test account
    let keypair = random_keypair();
    let offchain = random_offchain_keypair();
    let chain_key = keypair.public().to_address();
    let packet_key = offchain.public();

    db.upsert_account(None, 1, chain_key, *packet_key, None, 100, 0, 0)
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

    // Test: Query with multiple filters (keyid + chainKey)
    let query = format!(
        r#"
        query {{
            accounts(keyid: 1, chainKey: "{}") {{
                __typename
                ... on AccountsList {{
                    accounts {{
                        keyid
                        chainKey
                        packetKey
                    }}
                }}
                ... on MissingFilterError {{ code message }}
                ... on QueryFailedError {{ code message }}
            }}
        }}
    "#,
        chain_key.to_hex()
    );

    let response = execute_graphql_query(&schema, &query).await;
    assert!(response.errors.is_empty());

    let data = response.data.into_json()?;
    let accounts = extract_accounts_from_result(&data)?;
    assert_eq!(accounts.len(), 1);
    assert_eq!(accounts[0]["keyid"], 1);
    assert_eq!(accounts[0]["chainKey"], chain_key.to_hex());

    Ok(())
}

#[tokio::test]
async fn test_accounts_query_with_multiaddresses() -> anyhow::Result<()> {
    let db = BlokliDb::new_in_memory().await?;

    // Create test account
    let keypair = random_keypair();
    let offchain = random_offchain_keypair();
    let chain_key = keypair.public().to_address();
    let packet_key = offchain.public();

    db.upsert_account(None, 1, chain_key, *packet_key, None, 100, 0, 0)
        .await?;

    // Add multiaddress announcement
    let multiaddr_str = "/ip4/127.0.0.1/tcp/9091/p2p/16Uiu2HAmUsJwbECMroQUC29LQZZWsYpYZx1oaM1H9DBoZHLkYn12";
    let multiaddr: Multiaddr = multiaddr_str.parse().unwrap();
    db.insert_announcement(None, chain_key, multiaddr.clone(), 100).await?;

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

    // Test: Query should include multiaddresses
    let query = r#"
        query {
            accounts(keyid: 1) {
                __typename
                ... on AccountsList {
                    accounts {
                        keyid
                        multiAddresses
                    }
                }
                ... on MissingFilterError { code message }
                ... on QueryFailedError { code message }
            }
        }
    "#;

    let response = execute_graphql_query(&schema, query).await;
    assert!(response.errors.is_empty());

    let data = response.data.into_json()?;
    let accounts = extract_accounts_from_result(&data)?;
    assert_eq!(accounts.len(), 1);
    assert_eq!(accounts[0]["keyid"], 1);

    let multi_addresses = accounts[0]["multiAddresses"].as_array().unwrap();
    assert_eq!(multi_addresses.len(), 1);
    assert_eq!(multi_addresses[0].as_str().unwrap(), multiaddr_str);

    Ok(())
}

#[tokio::test]
async fn test_account_count_no_filters() -> anyhow::Result<()> {
    let db = BlokliDb::new_in_memory().await?;

    // Create multiple test accounts
    for i in 1..=3 {
        let keypair = random_keypair();
        let offchain = random_offchain_keypair();
        let chain_key = keypair.public().to_address();
        let packet_key = offchain.public();

        db.upsert_account(None, i, chain_key, *packet_key, None, 100 + i, 0, 0)
            .await?;
    }

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

    // Test: Count all accounts without filters
    let query = r#"
        query {
            accountCount {
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
    let count = extract_count_from_result(&data, "accountCount")?;
    assert_eq!(count, 3);

    Ok(())
}

#[tokio::test]
async fn test_account_count_with_filters() -> anyhow::Result<()> {
    let db = BlokliDb::new_in_memory().await?;

    // Create test accounts
    let keypair1 = random_keypair();
    let offchain1 = random_offchain_keypair();
    let chain_key1 = keypair1.public().to_address();
    let packet_key1 = offchain1.public();

    let keypair2 = random_keypair();
    let offchain2 = random_offchain_keypair();
    let chain_key2 = keypair2.public().to_address();
    let packet_key2 = offchain2.public();

    db.upsert_account(None, 1, chain_key1, *packet_key1, None, 100, 0, 0)
        .await?;
    db.upsert_account(None, 2, chain_key2, *packet_key2, None, 101, 0, 0)
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

    // Test 1: Count by keyid
    {
        let query = r#"
            query {
                accountCount(keyid: 1) {
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
        let count = extract_count_from_result(&data, "accountCount")?;
        assert_eq!(count, 1);
    }

    // Test 2: Count by packetKey
    {
        let query = format!(
            r#"
            query {{
                accountCount(packetKey: "{}") {{
                    __typename
                    ... on Count {{
                        count
                    }}
                    ... on MissingFilterError {{
                        code
                        message
                    }}
                    ... on QueryFailedError {{
                        code
                        message
                    }}
                }}
            }}
        "#,
            packet_key2.to_hex()
        );

        let response = execute_graphql_query(&schema, &query).await;
        assert!(response.errors.is_empty(), "GraphQL errors: {:?}", response.errors);

        let data = response.data.into_json()?;
        let count = extract_count_from_result(&data, "accountCount")?;
        assert_eq!(count, 1);
    }

    // Test 3: Count by chainKey
    {
        let query = format!(
            r#"
            query {{
                accountCount(chainKey: "{}") {{
                    __typename
                    ... on Count {{
                        count
                    }}
                    ... on MissingFilterError {{
                        code
                        message
                    }}
                    ... on QueryFailedError {{
                        code
                        message
                    }}
                }}
            }}
        "#,
            chain_key1.to_hex()
        );

        let response = execute_graphql_query(&schema, &query).await;
        assert!(response.errors.is_empty(), "GraphQL errors: {:?}", response.errors);

        let data = response.data.into_json()?;
        let count = extract_count_from_result(&data, "accountCount")?;
        assert_eq!(count, 1);
    }

    Ok(())
}

#[tokio::test]
async fn test_account_count_invalid_chain_key() -> anyhow::Result<()> {
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

    // Test: Count with invalid chain key
    let query = r#"
        query {
            accountCount(chainKey: "0xinvalid") {
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

    // Should return QueryFailedError variant
    assert_eq!(data["accountCount"]["__typename"], "QueryFailedError");
    assert_eq!(data["accountCount"]["code"], "INVALID_ADDRESS");
    assert!(
        data["accountCount"]["message"].as_str().unwrap().contains("Invalid"),
        "Error message should mention invalid address"
    );

    Ok(())
}

#[tokio::test]
async fn test_account_count_nonexistent_account() -> anyhow::Result<()> {
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

    // Test: Count for non-existent keyid
    let query = r#"
        query {
            accountCount(keyid: 999) {
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
    let count = extract_count_from_result(&data, "accountCount")?;
    assert_eq!(count, 0);

    Ok(())
}

#[tokio::test]
async fn test_accounts_query_with_union_result_types() -> anyhow::Result<()> {
    let db = BlokliDb::new_in_memory().await?;

    // Create test account
    let keypair = random_keypair();
    let offchain = random_offchain_keypair();
    let chain_key = keypair.public().to_address();
    let packet_key = offchain.public();
    let safe_address = Address::from([0x01; 20]);

    db.upsert_account(None, 1, chain_key, *packet_key, Some(safe_address), 100, 0, 0)
        .await?;

    // Add multiaddress announcement
    let multiaddr_str = "/ip4/127.0.0.1/tcp/9091/p2p/16Uiu2HAmUsJwbECMroQUC29LQZZWsYpYZx1oaM1H9DBoZHLkYn12";
    let multiaddr: Multiaddr = multiaddr_str.parse().unwrap();
    db.insert_announcement(None, chain_key, multiaddr.clone(), 100).await?;

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

    // Test: Query with union result types as defined in target schema
    let query = format!(
        r#"
        query {{
            accounts(chainKey: "{}") {{
                ... on AccountsList {{
                    __typename
                    accounts {{
                        chainKey
                        keyid
                        multiAddresses
                        packetKey
                        safeAddress
                    }}
                }}
                ... on MissingFilterError {{
                    __typename
                    code
                    message
                }}
                ... on QueryFailedError {{
                    __typename
                    code
                    message
                }}
            }}
        }}
    "#,
        chain_key.to_hex()
    );

    let response = execute_graphql_query(&schema, &query).await;

    // Assert no GraphQL errors
    assert!(
        response.errors.is_empty(),
        "GraphQL query should not return errors: {:?}",
        response.errors
    );

    // Convert response data to JSON
    let data = response.data.into_json()?;

    // Verify it's the AccountsList variant
    assert_eq!(
        data["accounts"]["__typename"].as_str(),
        Some("AccountsList"),
        "Should return AccountsList variant"
    );

    // Extract and validate accounts array
    let accounts = data["accounts"]["accounts"].as_array().unwrap();
    assert_eq!(accounts.len(), 1);

    // Validate account fields
    assert_eq!(accounts[0]["keyid"], 1);
    assert_eq!(accounts[0]["chainKey"], chain_key.to_hex());
    assert!(packet_key.to_hex().contains(accounts[0]["packetKey"].as_str().unwrap()));
    assert_eq!(accounts[0]["safeAddress"], safe_address.to_hex());

    // Validate multiAddresses
    let multi_addresses = accounts[0]["multiAddresses"].as_array().unwrap();
    assert_eq!(multi_addresses.len(), 1);
    assert_eq!(multi_addresses[0].as_str().unwrap(), multiaddr_str);

    Ok(())
}
