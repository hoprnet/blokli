use blokli_api::{
    query::QueryRoot,
    schema::{ChainId, NetworkName},
};
use blokli_chain_types::ContractAddresses;
use blokli_db::{BlokliDbGeneralModelOperations, db::BlokliDb, safe_contracts::BlokliDbSafeContractOperations};
use blokli_db_entity::hopr_safe_contract::{Column as SafeColumn, Entity as SafeEntity};
use hopr_types::primitive::{prelude::Address, traits::ToHex};
use rand::RngCore;
use sea_orm::{ColumnTrait, EntityTrait, PaginatorTrait, QueryFilter};

// Helper to generate random address
fn random_address() -> Address {
    let mut rng = rand::rng();
    let mut bytes = [0u8; 20];
    rng.fill_bytes(&mut bytes);
    Address::from(bytes)
}

type TestSchema = async_graphql::Schema<QueryRoot, async_graphql::EmptyMutation, async_graphql::EmptySubscription>;

fn build_test_schema(db: &BlokliDb) -> TestSchema {
    async_graphql::Schema::build(
        QueryRoot,
        async_graphql::EmptyMutation,
        async_graphql::EmptySubscription,
    )
    .data(db.conn(blokli_db::TargetDb::Index).clone())
    .data(ContractAddresses::default())
    .data(ChainId(100))
    .data(NetworkName("test".to_string()))
    .data(blokli_api::schema::GasMultiplier(1.0))
    .finish()
}

#[tokio::test]
async fn test_safe_query() -> anyhow::Result<()> {
    let db = BlokliDb::new_in_memory().await?;

    let safe_address = random_address();
    let module_address = random_address();
    let chain_key = random_address();

    db.create_safe_contract(None, safe_address, module_address, chain_key, 100, 0, 0)
        .await?;

    let count = SafeEntity::find()
        .filter(SafeColumn::Address.eq(safe_address.as_ref().to_vec()))
        .count(db.conn(blokli_db::TargetDb::Index))
        .await?;
    assert_eq!(count, 1, "DB should have 1 safe contract");

    let schema = build_test_schema(&db);

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
    insta::assert_yaml_snapshot!(response, {
        ".data.safe.address" => "[address]",
        ".data.safe.moduleAddress" => "[address]",
        ".data.safe.chainKey" => "[address]",
    });

    Ok(())
}

#[tokio::test]
async fn test_safe_by_chain_key_query() -> anyhow::Result<()> {
    let db = BlokliDb::new_in_memory().await?;

    let safe_address = random_address();
    let module_address = random_address();
    let chain_key = random_address();

    db.create_safe_contract(None, safe_address, module_address, chain_key, 100, 0, 0)
        .await?;

    let schema = build_test_schema(&db);

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
    insta::assert_yaml_snapshot!(response, {
        ".data.safeByChainKey.address" => "[address]",
        ".data.safeByChainKey.moduleAddress" => "[address]",
        ".data.safeByChainKey.chainKey" => "[address]",
    });

    Ok(())
}

#[tokio::test]
async fn test_safes_list_query() -> anyhow::Result<()> {
    let db = BlokliDb::new_in_memory().await?;

    let safe_address = random_address();
    let module_address = random_address();
    let chain_key = random_address();

    db.create_safe_contract(None, safe_address, module_address, chain_key, 100, 0, 0)
        .await?;

    let schema = build_test_schema(&db);

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
    insta::assert_yaml_snapshot!(response, {
        ".data.safes.safes[].address" => "[address]",
    });

    Ok(())
}

#[tokio::test]
async fn test_safe_query_invalid_hex_format() -> anyhow::Result<()> {
    let db = BlokliDb::new_in_memory().await?;
    let schema = build_test_schema(&db);

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
    insta::assert_yaml_snapshot!(response);

    Ok(())
}

#[tokio::test]
async fn test_safe_query_invalid_address_length() -> anyhow::Result<()> {
    let db = BlokliDb::new_in_memory().await?;
    let schema = build_test_schema(&db);

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
    insta::assert_yaml_snapshot!(response);

    Ok(())
}

#[tokio::test]
async fn test_safe_query_not_found() -> anyhow::Result<()> {
    let db = BlokliDb::new_in_memory().await?;
    let schema = build_test_schema(&db);

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
    insta::assert_yaml_snapshot!(response);

    Ok(())
}

#[tokio::test]
async fn test_safe_by_chain_key_invalid_address() -> anyhow::Result<()> {
    let db = BlokliDb::new_in_memory().await?;
    let schema = build_test_schema(&db);

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
    insta::assert_yaml_snapshot!(response);

    Ok(())
}

#[tokio::test]
async fn test_safe_by_chain_key_not_found() -> anyhow::Result<()> {
    let db = BlokliDb::new_in_memory().await?;
    let schema = build_test_schema(&db);

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
    insta::assert_yaml_snapshot!(response);

    Ok(())
}
