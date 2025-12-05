use std::{sync::Arc, time::Duration};

use blokli_api::query::{QueryRoot, SafeResult};
use blokli_api_types::Safe;
use blokli_chain_types::ContractAddresses;
use blokli_db::{
    BlokliDbGeneralModelOperations,
    db::BlokliDb,
    safe_contracts::BlokliDbSafeContractOperations,
};
use hopr_primitive_types::{prelude::Address, traits::ToHex};
use sea_orm::{EntityTrait, PaginatorTrait}; // Import EntityTrait and PaginatorTrait
use tokio::time::sleep;

#[tokio::test]
async fn test_safe_queries() -> anyhow::Result<()> {
    let db = BlokliDb::new_in_memory().await?;

    // Helper to generate random address
    fn random_address() -> Address {
        use rand::RngCore;
        let mut rng = rand::rng();
        let mut bytes = [0u8; 20];
        rng.fill_bytes(&mut bytes);
        Address::from(bytes)
    }

    // Create test data
    let safe_address = random_address();
    let module_address = random_address();
    let chain_key = random_address();

    db.create_safe_contract(
        None,
        safe_address,
        module_address,
        chain_key,
        100,
        0,
        0,
    )
    .await?;

    // Verify DB has the record
    let count = blokli_db_entity::hopr_safe_contract::Entity::find().count(db.conn(blokli_db::TargetDb::Index)).await?;
    assert_eq!(count, 1, "DB should have 1 safe contract");
    
    let stored_safe = blokli_db_entity::hopr_safe_contract::Entity::find().one(db.conn(blokli_db::TargetDb::Index)).await?.unwrap();
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
    .data(100u64) // chain_id
    .data("test".to_string()) // network
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
    assert_eq!(safes_list.len(), 1);
    assert_eq!(safes_list[0]["address"], safe_address.to_hex());

    Ok(())
}
