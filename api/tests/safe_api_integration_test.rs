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
    let safe_address_0 = random_address();
    let module_address_0 = random_address();
    let chain_key_0 = random_address();

    let safe_address_1 = random_address();
    let module_address_1 = random_address();
    let chain_key_1 = random_address();
    let owner_address_2 = random_address();

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
    db.record_safe_activity(
        None,
        safe_address_0,
        SafeActivityKind::SafeSetup,
        Hash::default(),
        None,
        None,
        Some("2".to_string()),
        None,
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

    // Verify DB has the record
    let count = SafeEntity::find()
        .filter(SafeColumn::Address.eq(safe_address_0.as_ref().to_vec()))
        .count(db.conn(blokli_db::TargetDb::Index))
        .await?;
    assert_eq!(count, 1, "DB should have 1 safe contract");

    let _ = SafeEntity::find().one(db.conn(blokli_db::TargetDb::Index)).await?;

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
    .data(blokli_api::schema::GasMultiplier(1.0))
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
    assert!(response.errors.is_empty(), "Errors: {:?}", response.errors);

    let data = response.data.into_json().unwrap();
    let safe_data = data["safe"].as_object().unwrap();
    assert_eq!(safe_data["address"], safe_address_0.to_hex());
    assert_eq!(safe_data["moduleAddress"], module_address_0.to_hex());
    assert_eq!(safe_data["chainKey"], chain_key_0.to_hex());
    assert_eq!(safe_data["threshold"], "3");
    assert_eq!(
        safe_data["owners"].as_array().unwrap().len(),
        3,
        "Owners should come from indexed Safe owner history"
    );

    // Test safeBy(selector: OWNER) query
    let query = format!(
        r#"
        query {{
            safeBy(selector: OWNER, address: "{}") {{
                ... on Safe {{
                    address
                    moduleAddress
                    chainKey
                    threshold
                }}
            }}
        }}
        "#,
        owner_address_2.to_hex()
    );

    let response = schema.execute(query).await;
    assert!(response.errors.is_empty());

    let data = response.data.into_json().unwrap();
    let safe_data = data["safeBy"].as_object().unwrap();
    assert_eq!(safe_data["address"], safe_address_0.to_hex());
    assert_eq!(safe_data["moduleAddress"], module_address_0.to_hex());
    assert_eq!(safe_data["chainKey"], chain_key_0.to_hex());
    assert_eq!(safe_data["threshold"], "3");

    // Test deprecated safeByChainKey(chainKey) alias, which now resolves by indexed owner membership
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
        owner_address_2.to_hex()
    );

    let response = schema.execute(query).await;
    assert!(response.errors.is_empty());

    let data = response.data.into_json().unwrap();
    let safe_data = data["safeByChainKey"].as_object().unwrap();
    assert_eq!(safe_data["address"], safe_address_0.to_hex());
    assert_eq!(safe_data["moduleAddress"], module_address_0.to_hex());
    assert_eq!(safe_data["chainKey"], chain_key_0.to_hex());
    assert_eq!(safe_data["threshold"], "3");

    // Test safes() list query
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
    assert!(response.errors.is_empty());

    let data = response.data.into_json().unwrap();
    let safes_list = data["safes"]["safes"].as_array().unwrap();
    assert!(!safes_list.is_empty());
    assert!(safes_list.iter().any(|s| s["address"] == safe_address_0.to_hex()));
    assert!(
        safes_list
            .iter()
            .any(|s| s["address"] == safe_address_0.to_hex() && s["threshold"] == "3")
    );

    let query = format!(
        r#"
        query {{
            ticketRedemptionStats(filter: {{ safeAddress: "{}" }}) {{
                ... on RedeemedStats {{
                    redeemedAmount
                    redemptionCount
                }}
            }}
        }}
        "#,
        safe_address_0.to_hex()
    );

    let response = schema.execute(query).await;
    assert!(response.errors.is_empty(), "Errors: {:?}", response.errors);
    let data = response.data.into_json().unwrap();
    let stats_data = data["ticketRedemptionStats"].as_object().unwrap();
    assert_eq!(stats_data["redeemedAmount"], "0.000000000000000023 wxHOPR");
    assert_eq!(stats_data["redemptionCount"], "3");

    let query = format!(
        r#"
        query {{
            ticketRedemptionStats(filter: {{ nodeAddress: "{}" }}) {{
                ... on RedeemedStats {{
                    redeemedAmount
                    redemptionCount
                }}
            }}
        }}
        "#,
        chain_key_0.to_hex()
    );

    let response = schema.execute(query).await;
    assert!(response.errors.is_empty(), "Errors: {:?}", response.errors);
    let data = response.data.into_json().unwrap();
    let stats_data = data["ticketRedemptionStats"].as_object().unwrap();
    assert_eq!(stats_data["redeemedAmount"], "0.000000000000000015 wxHOPR");
    assert_eq!(stats_data["redemptionCount"], "3");

    let query = format!(
        r#"
        query {{
            ticketRedemptionStats(filter: {{ safeAddress: "{}", nodeAddress: "{}" }}) {{
                ... on RedeemedStats {{
                    redeemedAmount
                    redemptionCount
                }}
            }}
        }}
        "#,
        safe_address_0.to_hex(),
        chain_key_0.to_hex()
    );

    let response = schema.execute(query).await;
    assert!(response.errors.is_empty(), "Errors: {:?}", response.errors);
    let data = response.data.into_json().unwrap();
    let stats_data = data["ticketRedemptionStats"].as_object().unwrap();
    assert_eq!(stats_data["redeemedAmount"], "0.000000000000000012 wxHOPR");
    assert_eq!(stats_data["redemptionCount"], "2");

    let query = format!(
        r#"
        query {{
            ticketRedemptionStats(filter: {{ safeAddress: "{}", nodeAddress: "{}" }}) {{
                ... on RedeemedStats {{
                    redeemedAmount
                    redemptionCount
                }}
            }}
        }}
        "#,
        safe_address_0.to_hex(),
        chain_key_1.to_hex()
    );

    let response = schema.execute(query).await;
    assert!(response.errors.is_empty(), "Errors: {:?}", response.errors);
    let data = response.data.into_json().unwrap();
    let stats_data = data["ticketRedemptionStats"].as_object().unwrap();
    assert_eq!(stats_data["redeemedAmount"], "0.000000000000000011 wxHOPR");
    assert_eq!(stats_data["redemptionCount"], "1");

    Ok(())
}

#[tokio::test]
async fn test_preseeded_safe_query_returns_owners_and_threshold() -> anyhow::Result<()> {
    let db = BlokliDb::new_in_memory().await?;
    let loaded = db.load_preseeded_safes(None, "jura").await?;
    assert!(loaded > 0, "expected jura pre-seeded safes to load");

    let schema = async_graphql::Schema::build(
        QueryRoot,
        async_graphql::EmptyMutation,
        async_graphql::EmptySubscription,
    )
    .data(db.conn(blokli_db::TargetDb::Index).clone())
    .data(ContractAddresses::default())
    .data(ChainId(100))
    .data(NetworkName("jura".to_string()))
    .data(blokli_api::schema::GasMultiplier(1.0))
    .finish();

    let safe_address = "0xf767efe1700e8c14d16e214920b26a0ce568ae5b";
    let expected_module_address = "0x4e0799e4b9dca722523b2a211135450a2755aa59";
    let expected_chain_key = "0x4ff4e61052a4dfb1be72866ab711ae08dd861976";

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
            }}
        }}
        "#,
        safe_address
    );

    let response = schema.execute(query).await;
    assert!(response.errors.is_empty(), "Errors: {:?}", response.errors);

    let data = response.data.into_json().unwrap();
    let safe_data = data["safe"].as_object().unwrap();
    assert_eq!(safe_data["address"], safe_address);
    assert_eq!(safe_data["moduleAddress"], expected_module_address);
    assert_eq!(safe_data["chainKey"], expected_chain_key);
    assert_eq!(safe_data["threshold"], "1");
    assert_eq!(
        safe_data["owners"].as_array().unwrap(),
        &vec![serde_json::Value::String(expected_chain_key.to_string())]
    );

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
    .data(blokli_api::schema::GasMultiplier(1.0))
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

    let query = r#"
        query {
            ticketRedemptionStats(filter: {}) {
                ... on RedeemedStats {
                    redeemedAmount
                }
                ... on MissingFilterError {
                    code
                }
            }
        }
    "#;

    let response = schema.execute(query).await;
    assert!(response.errors.is_empty());
    let data = response.data.into_json().unwrap();
    let error = data["ticketRedemptionStats"].as_object().unwrap();
    assert_eq!(error["code"], "MISSING_FILTER");

    let query = r#"
        query {
            ticketRedemptionStats(filter: { nodeAddress: "0x123" }) {
                ... on RedeemedStats {
                    redeemedAmount
                }
                ... on InvalidAddressError {
                    code
                }
            }
        }
    "#;

    let response = schema.execute(query).await;
    assert!(response.errors.is_empty());
    let data = response.data.into_json().unwrap();
    let error = data["ticketRedemptionStats"].as_object().unwrap();
    assert_eq!(error["code"], "INVALID_ADDRESS");

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
    .data(blokli_api::schema::GasMultiplier(1.0))
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

    let query = format!(
        r#"
        query {{
            ticketRedemptionStats(filter: {{ safeAddress: "{}" }}) {{
                ... on RedeemedStats {{
                    redeemedAmount
                    redemptionCount
                }}
                ... on QueryFailedError {{
                    code
                }}
            }}
        }}
        "#,
        nonexistent_address
    );

    let response = schema.execute(query).await;
    assert!(response.errors.is_empty());
    let data = response.data.into_json().unwrap();
    let stats = data["ticketRedemptionStats"].as_object().unwrap();
    assert_eq!(stats["redeemedAmount"], "0 wxHOPR");
    assert_eq!(stats["redemptionCount"], "0");

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
    .data(blokli_api::schema::GasMultiplier(1.0))
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
    .data(blokli_api::schema::GasMultiplier(1.0))
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
