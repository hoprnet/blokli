use blokli_api::{
    query::QueryRoot,
    schema::{ChainId, NetworkName},
};
use blokli_chain_types::ContractAddresses;
use blokli_db::{
    BlokliDbGeneralModelOperations,
    db::BlokliDb,
    safe_contracts::BlokliDbSafeContractOperations,
    safe_history::{BlokliDbSafeHistoryOperations, SafeActivityKind},
    safe_redeemed_stats::BlokliDbSafeRedeemedStatsOperations,
};
use blokli_db_entity::hopr_safe_contract::{Column as SafeColumn, Entity as SafeEntity};
use hopr_types::{
    crypto::types::Hash,
    primitive::{
        prelude::{Address, Balance, WxHOPR},
        traits::ToHex,
    },
};
use sea_orm::{ColumnTrait, EntityTrait, PaginatorTrait, QueryFilter};

type TestSchema = async_graphql::Schema<QueryRoot, async_graphql::EmptyMutation, async_graphql::EmptySubscription>;

fn safe_address() -> Address {
    Address::new(&[0x11; 20])
}

fn module_address() -> Address {
    Address::new(&[0x22; 20])
}

fn chain_key_address() -> Address {
    Address::new(&[0x33; 20])
}

fn secondary_safe_address() -> Address {
    Address::new(&[0x44; 20])
}

fn secondary_module_address() -> Address {
    Address::new(&[0x55; 20])
}

fn secondary_chain_key_address() -> Address {
    Address::new(&[0x66; 20])
}

fn owner_address() -> Address {
    Address::new(&[0x77; 20])
}

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

    let safe_address_0 = safe_address();
    let module_address_0 = module_address();
    let chain_key_0 = chain_key_address();

    let safe_address_1 = secondary_safe_address();
    let module_address_1 = secondary_module_address();
    let chain_key_1 = secondary_chain_key_address();
    let owner_address_2 = owner_address();

    db.create_safe_contract(None, safe_address_0, module_address_0, chain_key_0, 100, 0, 0)
        .await?;
    db.create_safe_contract(None, safe_address_1, module_address_1, chain_key_1, 100, 0, 1)
        .await?;
    db.upsert_safe_owner_state(None, safe_address_0, chain_key_0, true, 100, 0, 0)
        .await?;
    db.upsert_safe_owner_state(None, safe_address_0, chain_key_1, true, 101, 0, 0)
        .await?;
    db.upsert_safe_owner_state(None, safe_address_0, owner_address_2, true, 102, 0, 0)
        .await?;
    db.upsert_safe_owner_state(None, safe_address_1, chain_key_1, true, 100, 0, 1)
        .await?;
    db.record_safe_setup(
        None,
        safe_address_0,
        Hash::default(),
        vec![chain_key_0, chain_key_1, owner_address_2],
        "2".to_string(),
        Some(chain_key_0),
        100,
        0,
        0,
    )
    .await?;
    db.record_safe_activity(
        None,
        safe_address_0,
        SafeActivityKind::ChangedThreshold,
        Hash::default(),
        None,
        None,
        Some("3".to_string()),
        None,
        None,
        105,
        0,
        0,
    )
    .await?;
    db.record_safe_ticket_redeemed(
        None,
        safe_address_0,
        chain_key_0,
        Balance::<WxHOPR>::from(7_u64),
        110,
        1,
        0,
    )
    .await?;
    db.record_safe_ticket_redeemed(
        None,
        safe_address_0,
        chain_key_0,
        Balance::<WxHOPR>::from(5_u64),
        120,
        1,
        1,
    )
    .await?;
    db.record_safe_ticket_redeemed(
        None,
        safe_address_0,
        chain_key_1,
        Balance::<WxHOPR>::from(11_u64),
        121,
        1,
        2,
    )
    .await?;
    db.record_safe_ticket_redeemed(
        None,
        safe_address_1,
        chain_key_0,
        Balance::<WxHOPR>::from(3_u64),
        130,
        2,
        0,
    )
    .await?;

    let count = SafeEntity::find()
        .filter(SafeColumn::Address.eq(safe_address_0.as_ref().to_vec()))
        .count(db.conn(blokli_db::TargetDb::Index))
        .await?;
    assert_eq!(count, 1, "DB should have 1 safe contract");

    let _ = SafeEntity::find().one(db.conn(blokli_db::TargetDb::Index)).await?;
    let schema = build_test_schema(&db);

    let query = format!(
        r#"
        query {{
            safe(address: "{}") {{
                ... on Safe {{
                    address
                    moduleAddress
                    chainKey
                    threshold
                    owners
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
        safe_address_0.to_hex()
    );

    let response = schema.execute(query).await;
    insta::assert_yaml_snapshot!(response);

    Ok(())
}

#[tokio::test]
async fn test_safe_by_chain_key_query() -> anyhow::Result<()> {
    let db = BlokliDb::new_in_memory().await?;

    let safe_address = safe_address();
    let module_address = module_address();
    let chain_key = chain_key_address();
    let owner_address = owner_address();

    db.create_safe_contract(None, safe_address, module_address, chain_key, 100, 0, 0)
        .await?;
    db.upsert_safe_owner_state(None, safe_address, owner_address, true, 101, 0, 0)
        .await?;
    db.record_safe_activity(
        None,
        safe_address,
        SafeActivityKind::ChangedThreshold,
        Hash::default(),
        None,
        None,
        Some("2".to_string()),
        None,
        None,
        105,
        0,
        0,
    )
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
                    threshold
                }}
            }}
        }}
        "#,
        owner_address.to_hex()
    );

    let response = schema.execute(query).await;
    insta::assert_yaml_snapshot!(response);
    Ok(())
}

#[tokio::test]
async fn test_safes_list_query() -> anyhow::Result<()> {
    let db = BlokliDb::new_in_memory().await?;

    let safe_address = safe_address();
    let module_address = module_address();
    let chain_key = chain_key_address();

    db.create_safe_contract(None, safe_address, module_address, chain_key, 100, 0, 0)
        .await?;
    db.record_safe_setup(
        None,
        safe_address,
        Hash::default(),
        vec![],
        "4".to_string(),
        Some(chain_key),
        100,
        0,
        0,
    )
    .await?;

    let schema = build_test_schema(&db);

    let query = r#"
        query {
            safes {
                ... on SafesList {
                    safes {
                        address
                        threshold
                    }
                }
            }
        }
    "#;

    let response = schema.execute(query).await;
    insta::assert_yaml_snapshot!(response);

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
