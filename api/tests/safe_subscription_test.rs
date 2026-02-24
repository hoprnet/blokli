use std::time::Duration;

use async_graphql::{Schema, futures_util::StreamExt};
use blokli_api::{query::QueryRoot, subscription::SubscriptionRoot};
use blokli_chain_indexer::{IndexerState, state::IndexerEvent};
use blokli_db::{BlokliDbGeneralModelOperations, db::BlokliDb, safe_contracts::BlokliDbSafeContractOperations};
use hopr_primitive_types::{prelude::Address, traits::ToHex};
use rand::RngCore;
use sea_orm::ActiveModelTrait;

// Helper to generate random address
fn random_address() -> Address {
    let mut rng = rand::rng();
    let mut bytes = [0u8; 20];
    rng.fill_bytes(&mut bytes);
    Address::from(bytes)
}

#[tokio::test]
async fn test_safe_deployed_subscription() -> anyhow::Result<()> {
    let db = BlokliDb::new_in_memory().await?;
    let indexer_state = IndexerState::new(10, 100);

    // Create dummy chain_info to satisfy watermark check
    let chain_info = blokli_db_entity::chain_info::ActiveModel {
        id: sea_orm::Set(1),
        last_indexed_block: sea_orm::Set(100),
        ..Default::default()
    };

    // Use update instead of insert because ensure_singletons() already created it
    chain_info.update(db.conn(blokli_db::TargetDb::Index)).await?;

    let schema = Schema::build(QueryRoot, async_graphql::EmptyMutation, SubscriptionRoot)
        .data(db.conn(blokli_db::TargetDb::Index).clone())
        .data(indexer_state.clone())
        .data(blokli_api::schema::GasMultiplier(1.0))
        .finish();

    // Start subscription
    let query = "subscription { safeDeployed { address moduleAddress chainKey } }";
    let mut stream = schema.execute_stream(query);

    // Simulate safe deployment event

    let safe_address = random_address();
    let module_address = random_address();
    let chain_key = random_address();

    // 1. Create safe in DB first (subscription needs to look it up)
    db.create_safe_contract(None, safe_address, module_address, chain_key, 101, 0, 0)
        .await?;

    // 2. Publish event
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(100)).await;
        indexer_state.publish_event(IndexerEvent::SafeDeployed(safe_address));
    });

    // 3. Verify subscription received event
    let response = stream.next().await.expect("Stream should return result");
    let data = response
        .into_result()
        .map_err(|e| anyhow::anyhow!("GraphQL errors: {:?}", e))?
        .data;

    let safe_data = data.into_json().unwrap();
    let safe = safe_data["safeDeployed"].as_object().unwrap();

    assert_eq!(safe["address"], safe_address.to_hex());
    assert_eq!(safe["moduleAddress"], module_address.to_hex());
    assert_eq!(safe["chainKey"], chain_key.to_hex());

    Ok(())
}
