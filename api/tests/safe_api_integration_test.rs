use blokli_api::{
    query::QueryRoot,
    schema::{ChainId, NetworkName},
};
use blokli_chain_types::ContractAddresses;
use blokli_db::{BlokliDbGeneralModelOperations, db::BlokliDb, safe_contracts::BlokliDbSafeContractOperations};
use blokli_db_entity::hopr_safe_contract::{Column as SafeColumn, Entity as SafeEntity};
use hopr_primitive_types::{prelude::Address, traits::ToHex};
use rand::RngCore;
use sea_orm::{ColumnTrait, EntityTrait, PaginatorTrait, QueryFilter};

// Helper to generate random address
fn random_address() -> Address {
    let mut rng = rand::rng();
    let mut bytes = [0u8; 20];
    rng.fill_bytes(&mut bytes);
    Address::from(bytes)
}

#[tokio::test]
async fn test_safe_queries() -> anyhow::Result<()> {
    let db = BlokliDb::new_in_memory().await?;

    // Create test data
    let safe_address = random_address();
    let module_address = random_address();
    let chain_key = random_address();

    db.create_safe_contract(None, safe_address, module_address, chain_key, 100, 0, 0)
        .await?;

    // Verify DB has the record
    let count = SafeEntity::find()
        .filter(SafeColumn::Address.eq(safe_address.as_ref().to_vec()))
        .count(db.conn(blokli_db::TargetDb::Index))
        .await?;
    assert_eq!(count, 1, "DB should have 1 safe contract");

    let stored_safe = SafeEntity::find()
        .one(db.conn(blokli_db::TargetDb::Index))
        .await?
        .unwrap();
    println!("Stored safe: {:?}", stored_safe);
    println!("Safe address hex: {}", safe_address.to_hex());

    // We don't need RPC for safe queries, so we don't inject it.
    let schema = async_graphql::Schema::build(
        QueryRoot,
        async_graphql::EmptyMutation,
        async_graphql::EmptySubscription,
    )
    .data(db.conn(blokli_db::TargetDb::Index).clone())
    .data(ContractAddresses::default())
    .data(ChainId(100)) // chain_id
    .data(NetworkName("test".to_string())) // network
    .finish();

    // Test safe(address) query
    let query = format!(
        r#"
        query {{
            safe(address: "{}") {{
                ... on Safe {{
                    address
                    moduleAddress
                    chainKey
                }}
                ... on QueryFailedError {{
                    message
                }}
                ... on InvalidAddressError {{
                    message
                }}
            }}
        }}
        "#,
        safe_address.to_hex()
    );

    let response = schema.execute(query).await;
    assert!(response.errors.is_empty(), "Errors: {:?}", response.errors);

    let data = response.data.into_json().unwrap();
    let safe_data = data["safe"].as_object().unwrap();
    assert_eq!(safe_data["address"], safe_address.to_hex());
    assert_eq!(safe_data["moduleAddress"], module_address.to_hex());
    assert_eq!(safe_data["chainKey"], chain_key.to_hex());

    // Test safeByChainKey(chainKey) query
    let query = format!(
        r#"
        query {{
            safeByChainKey(chainKey: "{}") {{
                ... on Safe {{
                    address
                    moduleAddress
                    chainKey
                }}
            }}
        }}
        "#,
        chain_key.to_hex()
    );

    let response = schema.execute(query).await;
    assert!(response.errors.is_empty());

    let data = response.data.into_json().unwrap();
    let safe_data = data["safeByChainKey"].as_object().unwrap();
    assert_eq!(safe_data["address"], safe_address.to_hex());
    assert_eq!(safe_data["moduleAddress"], module_address.to_hex());
    assert_eq!(safe_data["chainKey"], chain_key.to_hex());

    // Test safes() list query
    let query = r#"
        query {
            safes {
                ... on SafesList {
                    safes {
                        address
                    }
                }
            }
        }
    "#;

    let response = schema.execute(query).await;
    assert!(response.errors.is_empty());

    let data = response.data.into_json().unwrap();
    let safes_list = data["safes"]["safes"].as_array().unwrap();
    assert!(!safes_list.is_empty());
    assert!(safes_list.iter().any(|s| s["address"] == safe_address.to_hex()));

    Ok(())
}

#[tokio::test]
async fn test_safe_query_invalid_address() -> anyhow::Result<()> {
    let db = BlokliDb::new_in_memory().await?;

    let schema = async_graphql::Schema::build(
        QueryRoot,
        async_graphql::EmptyMutation,
        async_graphql::EmptySubscription,
    )
    .data(db.conn(blokli_db::TargetDb::Index).clone())
    .data(ContractAddresses::default())
    .data(ChainId(100)) // chain_id
    .data(NetworkName("test".to_string())) // network
    .finish();

    // Test with invalid hex format (not 0x prefixed)
    let query = r#"
        query {
            safe(address: "notvalidhex") {
                ... on Safe {
                    address
                }
                ... on InvalidAddressError {
                    code
                    message
                    address
                }
            }
        }
    "#;

    let response = schema.execute(query).await;
    assert!(response.errors.is_empty(), "Should not have GraphQL errors");

    let data = response.data.into_json().unwrap();
    let safe_data = data["safe"].as_object().unwrap();
    assert_eq!(safe_data["code"], "INVALID_ADDRESS");
    assert_eq!(safe_data["address"], "notvalidhex");
    assert!(safe_data["message"].as_str().unwrap().contains("Invalid"));

    // Test with invalid length (too short)
    let query = r#"
        query {
            safe(address: "0x123") {
                ... on InvalidAddressError {
                    code
                    message
                }
            }
        }
    "#;

    let response = schema.execute(query).await;
    assert!(response.errors.is_empty());

    let data = response.data.into_json().unwrap();
    let safe_data = data["safe"].as_object().unwrap();
    assert_eq!(safe_data["code"], "INVALID_ADDRESS");

    Ok(())
}

#[tokio::test]
async fn test_safe_query_not_found() -> anyhow::Result<()> {
    let db = BlokliDb::new_in_memory().await?;

    let schema = async_graphql::Schema::build(
        QueryRoot,
        async_graphql::EmptyMutation,
        async_graphql::EmptySubscription,
    )
    .data(db.conn(blokli_db::TargetDb::Index).clone())
    .data(ContractAddresses::default())
    .data(ChainId(100)) // chain_id
    .data(NetworkName("test".to_string())) // network
    .finish();

    // Query for a valid address that doesn't exist in DB
    let nonexistent_address = "0x1234567890123456789012345678901234567890";

    let query = format!(
        r#"
        query {{
            safe(address: "{}") {{
                ... on Safe {{
                    address
                }}
                ... on QueryFailedError {{
                    code
                    message
                }}
            }}
        }}
        "#,
        nonexistent_address
    );

    let response = schema.execute(query).await;
    assert!(response.errors.is_empty());

    let data = response.data.into_json().unwrap();
    // When safe is not found, the query returns null (GraphQL null for Option<SafeResult>)
    assert!(data["safe"].is_null(), "Safe should be null when not found");

    Ok(())
}

#[tokio::test]
async fn test_safe_by_chain_key_invalid_address() -> anyhow::Result<()> {
    let db = BlokliDb::new_in_memory().await?;

    let schema = async_graphql::Schema::build(
        QueryRoot,
        async_graphql::EmptyMutation,
        async_graphql::EmptySubscription,
    )
    .data(db.conn(blokli_db::TargetDb::Index).clone())
    .data(ContractAddresses::default())
    .data(ChainId(100)) // chain_id
    .data(NetworkName("test".to_string())) // network
    .finish();

    // Test with invalid chain key format
    let query = r#"
        query {
            safeByChainKey(chainKey: "invalid_key") {
                ... on Safe {
                    address
                }
                ... on InvalidAddressError {
                    code
                    message
                    address
                }
            }
        }
    "#;

    let response = schema.execute(query).await;
    assert!(response.errors.is_empty());

    let data = response.data.into_json().unwrap();
    let safe_data = data["safeByChainKey"].as_object().unwrap();
    assert_eq!(safe_data["code"], "INVALID_ADDRESS");
    assert_eq!(safe_data["address"], "invalid_key");

    Ok(())
}

#[tokio::test]
async fn test_safe_by_chain_key_not_found() -> anyhow::Result<()> {
    let db = BlokliDb::new_in_memory().await?;

    let schema = async_graphql::Schema::build(
        QueryRoot,
        async_graphql::EmptyMutation,
        async_graphql::EmptySubscription,
    )
    .data(db.conn(blokli_db::TargetDb::Index).clone())
    .data(ContractAddresses::default())
    .data(ChainId(100)) // chain_id
    .data(NetworkName("test".to_string())) // network
    .finish();

    // Query with valid address but not in DB
    let nonexistent_key = "0x9876543210987654321098765432109876543210";

    let query = format!(
        r#"
        query {{
            safeByChainKey(chainKey: "{}") {{
                ... on Safe {{
                    address
                }}
            }}
        }}
        "#,
        nonexistent_key
    );

    let response = schema.execute(query).await;
    assert!(response.errors.is_empty());

    let data = response.data.into_json().unwrap();
    assert!(
        data["safeByChainKey"].is_null(),
        "Safe should be null when chain key not found"
    );

    Ok(())
}
