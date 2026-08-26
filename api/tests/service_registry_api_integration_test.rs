//! End-to-end test of the service registry against a real chain.
//!
//! The chain part is genuine: contracts are deployed on Anvil through `ContractInstances`, the
//! service type and the entry are written by real transactions, and the logs are read back over
//! RPC. Those logs then go through the production handler, so the ABI decoding, the database
//! writes and the GraphQL projection are all exercised against bytes the contract produced rather
//! than against a fixture.
//!
//! The registry gates `selfRegister` on `nodeSafeRegistry.nodeToSafe(node) == msg.sender`. The
//! registry never inspects the bound address beyond that equality, so a plain EOA stands in for
//! the Safe here and keeps the test free of the Safe deployment machinery.

mod common;

use std::time::Duration;

use anyhow::Context as _;
use blokli_chain_indexer::{handlers::ContractEventHandlers, traits::ChainLogHandler};
use blokli_chain_rpc::{HoprIndexerRpcOperations, client::create_rpc_client_to_anvil};
use blokli_chain_types::{AlloyAddressExt, ContractAddresses, ContractInstances, utils::create_anvil};
use blokli_db::{db::BlokliDb, info::BlokliDbInfoOperations, services::BlokliDbServiceOperations};
use futures::StreamExt;
use hopr_bindings::{
    exports::alloy::primitives::{Address as AlloyAddress, B256, Bytes, U256},
    hopr_node_safe_registry::HoprNodeSafeRegistry::HoprNodeSafeRegistryInstance,
    hopr_service_registry::HoprServiceRegistry::HoprServiceRegistryInstance,
};
use hopr_types::{
    crypto::keypairs::{ChainKeypair, Keypair},
    internal::prelude::ServiceType,
    primitive::traits::ToHex,
};

/// Sorts the keys of every object in a GraphQL response.
///
/// async-graphql resolves fields concurrently, so the key order of a response is not stable
/// between runs and a snapshot of the raw value would be flaky.
fn sorted(value: serde_json::Value) -> serde_json::Value {
    match value {
        serde_json::Value::Object(map) => {
            let mut entries: Vec<_> = map.into_iter().map(|(key, value)| (key, sorted(value))).collect();
            entries.sort_by(|(left, _), (right, _)| left.cmp(right));
            serde_json::Value::Object(entries.into_iter().collect())
        }
        serde_json::Value::Array(items) => serde_json::Value::Array(items.into_iter().map(sorted).collect()),
        other => other,
    }
}

#[tokio::test]
async fn test_service_registration_reaches_query_and_subscription() -> anyhow::Result<()> {
    let block_time = Duration::from_secs(1);
    let anvil = create_anvil(Some(block_time));

    let deployer = ChainKeypair::from_secret(anvil.keys()[0].to_bytes().as_ref())?;
    let node = ChainKeypair::from_secret(anvil.keys()[1].to_bytes().as_ref())?;
    let safe = ChainKeypair::from_secret(anvil.keys()[2].to_bytes().as_ref())?;

    let deployer_client = create_rpc_client_to_anvil(&anvil, &deployer);
    let instances = ContractInstances::deploy_for_testing(deployer_client.clone(), &deployer).await?;
    let addresses = ContractAddresses::from(&instances);

    let node_address = node.public().to_address();
    let safe_address = safe.public().to_address();
    let service_type = ServiceType::GVPN_EXIT;

    // The deployer holds MANAGER_ROLE, so it can drop the type registration fee to zero and spare
    // the test a wxHOPR approval that has nothing to do with what is under test.
    instances
        .service_registry
        .setTypeRegistrationFee(U256::ZERO)
        .send()
        .await?
        .get_receipt()
        .await?
        .status()
        .then_some(())
        .context("setting the type registration fee must succeed")?;

    instances
        .service_registry
        .registerServiceType(
            B256::from(service_type.as_encoded()),
            AlloyAddress::ZERO,
            U256::ZERO,
            U256::ZERO,
        )
        .send()
        .await?
        .get_receipt()
        .await?
        .status()
        .then_some(())
        .context("registering the service type must succeed")?;

    // Bind the node to its stand-in Safe, which is what `selfRegister` checks.
    let node_client = create_rpc_client_to_anvil(&anvil, &node);
    HoprNodeSafeRegistryInstance::new(
        AlloyAddress::from_hopr_address(addresses.node_safe_registry),
        node_client,
    )
    .registerSafeByNode(AlloyAddress::from_hopr_address(safe_address))
    .send()
    .await?
    .get_receipt()
    .await?
    .status()
    .then_some(())
    .context("binding the node to its Safe must succeed")?;

    let safe_client = create_rpc_client_to_anvil(&anvil, &safe);
    let metadata = b"{\"v\":1}".to_vec();
    HoprServiceRegistryInstance::new(AlloyAddress::from_hopr_address(addresses.service_registry), safe_client)
        .selfRegister(
            B256::from(service_type.as_encoded()),
            AlloyAddress::from_hopr_address(node_address),
            Bytes::from(metadata.clone()),
        )
        .send()
        .await?
        .get_receipt()
        .await?
        .status()
        .then_some(())
        .context("registering the service entry must succeed")?;

    // Feed the real logs through the production handler.
    let db = BlokliDb::new_in_memory().await?;
    let (schema, indexer_state) = common::create_test_schema(&db);
    let rpc_operations = common::rpc_operations_for(&anvil, addresses, block_time)?;

    let handlers = ContractEventHandlers::new(
        addresses,
        db.clone(),
        rpc_operations.clone(),
        indexer_state.clone(),
        false,
        false,
    );
    let topics = handlers.contract_address_topics(addresses.service_registry);

    // Anvil rejects a range that runs past the chain head, so the scan stops at the current block.
    let head = rpc_operations.block_number().await?;
    let logs = rpc_operations
        .get_logs_for_address(addresses.service_registry, topics, 0, head)
        .await?;
    assert!(!logs.is_empty(), "the registry must have emitted logs");

    // The stream is live-only by design, so it must be listening before the first event is
    // published.
    let subscription = schema.execute_stream(
        r#"
        subscription {
            serviceUpdated(serviceType: "gvpn:exit") {
                kind
                serviceType
                node
                entry { serviceType node safe metadata registeredAt updatedAt }
            }
        }
        "#,
    );
    let mut subscription = subscription.boxed();

    // The subscription resolver reaches the event bus only while the stream is being polled, and
    // it awaits a database read on the way, so the two run concurrently: the log processing waits
    // briefly to let the stream finish subscribing before the first event is published.
    let log_count = logs.len();
    let (update, processed) = tokio::join!(
        tokio::time::timeout(Duration::from_secs(10), subscription.next()),
        async {
            tokio::time::sleep(Duration::from_millis(500)).await;
            for log in logs {
                handlers.collect_log_event(log.into(), true).await?;
            }
            Ok::<_, anyhow::Error>(())
        }
    );
    processed?;

    // Separates a decoding or storage failure from a subscription-delivery failure.
    let stored = db.get_service_entry(None, service_type, node_address).await?;
    assert!(
        stored.is_some(),
        "the registration must be indexed; {log_count} registry logs were processed"
    );
    db.set_indexer_state_info(None, u32::try_from(head)?).await?;

    let update = update
        .context("serviceUpdated must emit within the timeout")?
        .context("the subscription stream must not end")?;
    assert!(update.errors.is_empty(), "subscription errors: {:?}", update.errors);

    insta::assert_yaml_snapshot!("service_updated_subscription", sorted(update.data.into_json()?), {
        ".serviceUpdated.node" => "[node]",
        ".serviceUpdated.entry.node" => "[node]",
        ".serviceUpdated.entry.safe" => "[safe]",
        ".serviceUpdated.entry.registeredAt" => "[timestamp]",
        ".serviceUpdated.entry.updatedAt" => "[timestamp]",
    });

    let query = schema
        .execute(format!(
            r#"
            query {{
                services(node: "{}") {{
                    __typename
                    ... on ServicesList {{
                        services {{ serviceType node safe metadata }}
                    }}
                }}
                serviceCount(serviceType: "gvpn:exit") {{
                    __typename
                    ... on Count {{ count }}
                }}
                serviceTypes {{
                    __typename
                    ... on ServiceTypesList {{
                        serviceTypes {{ serviceType owner requirement registrationBurn updateBurn }}
                    }}
                }}
                serviceRegistryConfig {{
                    __typename
                    ... on ServiceRegistryConfig {{ typeRegistrationFee nodeSafeRegistry }}
                }}
            }}
            "#,
            node_address.to_hex()
        ))
        .await;
    assert!(query.errors.is_empty(), "query errors: {:?}", query.errors);

    insta::assert_yaml_snapshot!("service_registry_queries", sorted(query.data.into_json()?), {
        ".services.services[].node" => "[node]",
        ".services.services[].safe" => "[safe]",
        ".serviceTypes.serviceTypes[].owner" => "[owner]",
        ".serviceRegistryConfig.nodeSafeRegistry" => "[node_safe_registry]",
    });

    Ok(())
}

/// A bare `services` call is safe because it returns one bounded page at a stable watermark.
#[tokio::test]
async fn test_services_without_a_filter_is_paginated() -> anyhow::Result<()> {
    let db = BlokliDb::new_in_memory().await?;
    let (schema, _indexer_state) = common::create_test_schema(&db);

    let response = schema
        .execute(
            r#"
            query {
                services {
                    __typename
                    ... on ServicesList { services { node } watermark nextCursor }
                }
            }
            "#,
        )
        .await;

    assert!(response.errors.is_empty(), "query errors: {:?}", response.errors);
    assert_eq!(response.data.into_json()?["services"]["__typename"], "ServicesList");

    Ok(())
}

/// Both the prefixed and the bare form of a node address select the same entries, matching how
/// `accounts(chainKey:)` accepts its filter.
#[tokio::test]
async fn test_service_filters_accept_both_address_forms() -> anyhow::Result<()> {
    let db = BlokliDb::new_in_memory().await?;
    let (schema, _indexer_state) = common::create_test_schema(&db);

    for node in [
        "0x1111111111111111111111111111111111111111",
        "1111111111111111111111111111111111111111",
    ] {
        let response = schema
            .execute(format!(r#"query {{ services(node: "{node}") {{ __typename }} }}"#))
            .await;
        assert!(response.errors.is_empty(), "query errors: {:?}", response.errors);
        assert_eq!(
            response.data.into_json()?["services"]["__typename"],
            "ServicesList",
            "node filter {node} must be accepted"
        );
    }

    Ok(())
}

/// A service type filter is accepted as its ASCII name and as the equivalent 32-byte hex id.
#[tokio::test]
async fn test_service_type_filter_accepts_name_and_hex() -> anyhow::Result<()> {
    let db = BlokliDb::new_in_memory().await?;
    let (schema, _indexer_state) = common::create_test_schema(&db);

    for service_type in [
        "gvpn:exit",
        "0x6776706e3a657869740000000000000000000000000000000000000000000000",
    ] {
        let response = schema
            .execute(format!(
                r#"query {{ serviceTypes(serviceType: "{service_type}") {{ __typename }} }}"#
            ))
            .await;
        assert!(response.errors.is_empty(), "query errors: {:?}", response.errors);
        assert_eq!(
            response.data.into_json()?["serviceTypes"]["__typename"],
            "ServiceTypesList",
            "service type filter {service_type} must be accepted"
        );
    }

    let rejected = schema
        .execute(r#"query { serviceTypes(serviceType: "gvpn exit") { __typename ... on QueryFailedError { code } } }"#)
        .await;
    assert!(rejected.errors.is_empty(), "query errors: {:?}", rejected.errors);
    insta::assert_yaml_snapshot!("service_type_filter_rejected", sorted(rejected.data.into_json()?));

    Ok(())
}
