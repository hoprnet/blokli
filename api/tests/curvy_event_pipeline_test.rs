//! End-to-end Curvy event indexing and GraphQL subscription test.
//!
//! The test deploys the Curvy development contracts to Anvil, retrieves the
//! resulting `TokenRegistration` log from RPC, processes it through the Blokli
//! event handler, and verifies both database persistence and GraphQL delivery.

use std::time::Duration;

use async_graphql::{EmptyMutation, Schema};
use blokli_api::{query::QueryRoot, schema::GasMultiplier, subscription::SubscriptionRoot};
use blokli_chain_indexer::{IndexerState, handlers::ContractEventHandlers, traits::ChainLogHandler};
use blokli_chain_rpc::{
    Log,
    rpc::{RpcOperations, RpcOperationsConfig},
    transport::ReqwestClient,
};
use blokli_chain_types::{AlloyAddressExt, ContractAddresses};
use blokli_db::{BlokliDbGeneralModelOperations, TargetDb, db::BlokliDb};
use blokli_db_entity::{chain_info, curvy_token_registration};
use curvy_bindings::{config::CurvyContractInstances, curvy_vault_v2::CurvyVaultV2::TokenRegistration};
use futures::StreamExt;
use hopr_bindings::exports::alloy::{
    node_bindings::Anvil,
    providers::{Provider, ProviderBuilder},
    rpc::{client::ClientBuilder, types::Filter},
    signers::local::PrivateKeySigner,
    sol_types::SolEvent,
    transports::http::ReqwestTransport,
};
use hopr_types::primitive::{prelude::SerializableLog, traits::ToHex};
use sea_orm::{ActiveModelTrait, EntityTrait, PaginatorTrait, Set};
use serde_json::json;

const RESPONSE_TIMEOUT: Duration = Duration::from_secs(10);

#[tokio::test]
async fn test_curvy_token_registration_is_indexed_and_streamed() -> anyhow::Result<()> {
    let anvil = Anvil::new().spawn();
    let signer: PrivateKeySigner = anvil.keys()[0].clone().into();
    let deployer_address = signer.address();
    let provider = ProviderBuilder::new().wallet(signer).connect_http(anvil.endpoint_url());

    let curvy = CurvyContractInstances::deploy_for_testing(provider.clone(), deployer_address).await?;
    let curvy_addresses = curvy.get_contract_addresses();
    let latest_block = provider.get_block_number().await?;
    let filter = Filter::new()
        .address(curvy_addresses.vault_proxy)
        .event_signature(TokenRegistration::SIGNATURE_HASH)
        .from_block(0)
        .to_block(latest_block);
    let mut matching_logs = provider.get_logs(&filter).await?;
    let chain_log = matching_logs
        .pop()
        .ok_or_else(|| anyhow::anyhow!("Curvy deployment did not emit TokenRegistration"))?;
    let chain_log = Log::try_from(chain_log)?;
    let serializable_log = SerializableLog::from(chain_log.clone());

    let contract_addresses = ContractAddresses {
        curvy_aggregator: curvy_addresses.aggregator_proxy.to_hopr_address(),
        curvy_vault: curvy_addresses.vault_proxy.to_hopr_address(),
        curvy_portal_factory: curvy_addresses.portal_factory.to_hopr_address(),
        ..Default::default()
    };

    let transport = ReqwestTransport::new(anvil.endpoint_url());
    let rpc_client = ClientBuilder::default().transport(transport.clone(), transport.guess_local());
    let rpc_operations = RpcOperations::new(
        rpc_client,
        ReqwestClient::new(),
        RpcOperationsConfig {
            chain_id: 31_337,
            contract_addrs: contract_addresses,
            ..Default::default()
        },
        None,
    )?;

    let db = BlokliDb::new_in_memory().await?;
    chain_info::ActiveModel {
        id: Set(1),
        last_indexed_block: Set(i64::try_from(chain_log.block_number.saturating_sub(1))?),
        last_indexed_tx_index: Set(Some(0)),
        last_indexed_log_index: Set(Some(0)),
        ..Default::default()
    }
    .update(db.conn(TargetDb::Index))
    .await?;

    let indexer_state = IndexerState::new(10, 100);
    let schema = Schema::build(QueryRoot, EmptyMutation, SubscriptionRoot)
        .data(db.conn(TargetDb::Index).clone())
        .data(indexer_state.clone())
        .data(GasMultiplier(1.0))
        .finish();
    let handlers = ContractEventHandlers::new(contract_addresses, db.clone(), rpc_operations, indexer_state, true);

    let subscription = r#"
        subscription {
            curvyTokenRegistered {
                position {
                    block
                    logIndex
                    transactionHash
                    transactionIndex
                }
                tokenAddress
                tokenId
            }
        }
    "#;
    let mut stream = schema.execute_stream(subscription);

    let handlers_for_task = handlers.clone();
    let log_for_task = serializable_log.clone();
    let processing_task = tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(100)).await;
        handlers_for_task.collect_log_event(log_for_task, true).await
    });

    let response = tokio::time::timeout(RESPONSE_TIMEOUT, stream.next())
        .await?
        .ok_or_else(|| anyhow::anyhow!("Curvy subscription ended before delivering an event"))?;
    processing_task.await??;
    let response = response
        .into_result()
        .map_err(|errors| anyhow::anyhow!("GraphQL subscription errors: {errors:?}"))?;
    let live_data = response.data.into_json()?;

    let expected_event = json!({
        "position": {
            "block": chain_log.block_number.to_string(),
            "logIndex": chain_log.log_index.to_string(),
            "transactionHash": chain_log.tx_hash.to_hex(),
            "transactionIndex": chain_log.tx_index.to_string(),
        },
        "tokenAddress": curvy_addresses.erc20_mock.to_hopr_address().to_hex(),
        "tokenId": "2",
    });
    assert_eq!(live_data["curvyTokenRegistered"], expected_event);

    let stored = curvy_token_registration::Entity::find()
        .one(db.conn(TargetDb::Index))
        .await?
        .ok_or_else(|| anyhow::anyhow!("Curvy token registration was not persisted"))?;
    insta::assert_yaml_snapshot!(json!({
        "token_address": format!("0x{}", hex::encode(&stored.token_address)),
        "token_id": format!("0x{}", hex::encode(&stored.token_id)),
        "chain_tx_hash": format!("0x{}", hex::encode(&stored.chain_tx_hash)),
        "published_block": stored.published_block,
        "published_tx_index": stored.published_tx_index,
        "published_log_index": stored.published_log_index,
    }));

    let history_response = schema
        .execute(
            r#"
                query {
                    curvyTokenRegistrations(first: 10) {
                        position {
                            block
                            logIndex
                            transactionHash
                            transactionIndex
                        }
                        tokenAddress
                        tokenId
                    }
                }
            "#,
        )
        .await
        .into_result()
        .map_err(|errors| anyhow::anyhow!("GraphQL query errors: {errors:?}"))?;
    let history_data = history_response.data.into_json()?;
    assert_eq!(history_data["curvyTokenRegistrations"], json!([expected_event]));

    handlers.collect_log_event(serializable_log, true).await?;
    assert_eq!(
        curvy_token_registration::Entity::find()
            .count(db.conn(TargetDb::Index))
            .await?,
        1,
        "replaying the same chain log must not duplicate the indexed row"
    );

    Ok(())
}
