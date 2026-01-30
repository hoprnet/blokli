use blokli_api::{
    query::QueryRoot,
    schema::{ChainId, ExpectedBlockTime, Finality, NetworkName},
};
use blokli_chain_types::ContractAddresses;
use blokli_db::{BlokliDbGeneralModelOperations, TargetDb, db::BlokliDb, info::BlokliDbInfoOperations};

async fn execute_graphql_query(
    schema: &async_graphql::Schema<QueryRoot, async_graphql::EmptyMutation, async_graphql::EmptySubscription>,
    query: &str,
) -> async_graphql::Response {
    schema.execute(query).await
}

#[tokio::test]
async fn test_channel_closure_grace_period_always_non_null() {
    let db = BlokliDb::new_in_memory()
        .await
        .expect("Failed to create in-memory database");

    let schema = async_graphql::Schema::build(
        QueryRoot,
        async_graphql::EmptyMutation,
        async_graphql::EmptySubscription,
    )
    .data(db.conn(TargetDb::Index).clone())
    .data(ContractAddresses::default())
    .data(ChainId(100))
    .data(NetworkName("test-network".to_string()))
    .data(ExpectedBlockTime(1))
    .data(Finality(8))
    .finish();

    let query = r#"
        query {
            chainInfo {
                __typename
                ... on ChainInfo {
                    blockNumber
                    channelClosureGracePeriod
                }
            }
        }
    "#;

    let response = execute_graphql_query(&schema, query).await;
    assert!(
        response.errors.is_empty(),
        "Query should have no errors: {:?}",
        response.errors
    );

    let data = response.data.into_json().expect("Should have data");
    let chain_info_result = &data["chainInfo"];

    // Verify we got ChainInfo (not error)
    let type_name = chain_info_result.get("__typename").and_then(|v| v.as_str());
    assert_eq!(
        type_name,
        Some("ChainInfo"),
        "Should return ChainInfo type, got: {:?}",
        chain_info_result
    );

    let grace_period = &chain_info_result["channelClosureGracePeriod"];

    // Verify it's not null
    assert!(
        !grace_period.is_null(),
        "channelClosureGracePeriod should never be null"
    );

    // Verify it's a valid string representation of a UInt64
    let period_str = grace_period.as_str().expect("Should be a string");
    let period: u64 = period_str.parse().expect("Should parse as u64");

    // Verify it's the default value
    assert_eq!(period, 300, "Should default to 300 seconds when not set in database");
}

#[tokio::test]
async fn test_channel_closure_grace_period_with_custom_value() {
    let db = BlokliDb::new_in_memory()
        .await
        .expect("Failed to create in-memory database");

    // Set a custom grace period value
    let grace_period = 600u64; // 10 minutes

    db.set_channel_closure_grace_period(None, grace_period)
        .await
        .expect("Should set grace period");

    let schema = async_graphql::Schema::build(
        QueryRoot,
        async_graphql::EmptyMutation,
        async_graphql::EmptySubscription,
    )
    .data(db.conn(TargetDb::Index).clone())
    .data(ContractAddresses::default())
    .data(ChainId(100))
    .data(NetworkName("test-network".to_string()))
    .data(ExpectedBlockTime(1))
    .data(Finality(8))
    .finish();

    let query = r#"
        query {
            chainInfo {
                ... on ChainInfo {
                    channelClosureGracePeriod
                }
            }
        }
    "#;

    let response = execute_graphql_query(&schema, query).await;
    assert!(
        response.errors.is_empty(),
        "Query should have no errors: {:?}",
        response.errors
    );

    let data = response.data.into_json().expect("Should have data");
    let grace_period_str = data["chainInfo"]["channelClosureGracePeriod"]
        .as_str()
        .expect("Should be a string");

    assert_eq!(grace_period_str, "600", "Should return the custom grace period value");
}

#[tokio::test]
async fn test_channel_closure_grace_period_schema_non_nullable() {
    // This test verifies that the schema export shows channelClosureGracePeriod as non-nullable
    let db = BlokliDb::new_in_memory()
        .await
        .expect("Failed to create in-memory database");

    let schema = async_graphql::Schema::build(
        QueryRoot,
        async_graphql::EmptyMutation,
        async_graphql::EmptySubscription,
    )
    .data(db.conn(TargetDb::Index).clone())
    .data(ContractAddresses::default())
    .data(ChainId(100))
    .data(NetworkName("test-network".to_string()))
    .data(ExpectedBlockTime(1))
    .finish();

    let sdl = schema.sdl();

    // Verify that the schema includes channelClosureGracePeriod with non-nullable marker (!)
    assert!(
        sdl.contains("channelClosureGracePeriod: UInt64!"),
        "Schema should define channelClosureGracePeriod as non-nullable UInt64!, found:\n{}",
        sdl
    );
}
