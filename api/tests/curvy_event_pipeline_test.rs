//! End-to-end Curvy event indexing and GraphQL subscription test.
//!
//! The test deploys the Curvy development contracts to Anvil and processes the
//! events emitted during deployment plus an `autoShield` call. Events that
//! require valid ZK proofs are encoded with the generated contract bindings so
//! their decoder, persistence, and GraphQL paths are still covered.

use std::{error::Error, future::Future, time::Duration};

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
use blokli_db_entity::{
    chain_info, curvy_commitment_gas_cost, curvy_commitment_gas_fee_root, curvy_committed_note,
    curvy_committed_nullifier, curvy_pending_note, curvy_token_registration,
};
use curvy_bindings::{
    config::CurvyContractInstances,
    constants::DEV_PORTAL_DEPLOYMENT_FEE,
    curvy_aggregator_alpha_v2::CurvyAggregatorAlphaV2::{
        CommitmentGasFeeRootUpdated, CommittedNotes, CommittedNullifiers, PendingNotes,
    },
    curvy_vault_v2::CurvyVaultV2::{CommitmentGasCostsUpdated, TokenRegistration},
    portal_factory::CurvyTypes::Note,
};
use futures::StreamExt;
use hopr_bindings::exports::alloy::{
    network::TransactionBuilder,
    node_bindings::Anvil,
    primitives::{B256, U256},
    providers::{Provider, ProviderBuilder},
    rpc::{
        client::ClientBuilder,
        types::{Filter, TransactionRequest},
    },
    signers::local::PrivateKeySigner,
    sol_types::SolEvent,
    transports::http::ReqwestTransport,
};
use hopr_types::primitive::{
    prelude::{Address, SerializableLog},
    traits::ToHex,
};
use sea_orm::{ActiveModelTrait, EntityTrait, PaginatorTrait, Set};
use serde_json::{Value, json};

const RESPONSE_TIMEOUT: Duration = Duration::from_secs(10);

type CurvySchema = Schema<QueryRoot, EmptyMutation, SubscriptionRoot>;

fn logs_with_signature(logs: &[Log], signature: B256) -> Vec<Log> {
    logs.iter()
        .filter(|log| {
            log.topics
                .first()
                .is_some_and(|topic| topic.as_ref() == signature.as_slice())
        })
        .cloned()
        .collect()
}

fn decode_event<E: SolEvent>(log: &Log) -> anyhow::Result<E> {
    E::decode_raw_log(
        log.topics.iter().map(|topic| B256::from_slice(topic.as_ref())),
        &log.data,
    )
    .map_err(Into::into)
}

fn event_position(log: &Log, event_item_index: usize) -> Value {
    json!({
        "block": log.block_number.to_string(),
        "eventItemIndex": event_item_index.to_string(),
        "logIndex": log.log_index.to_string(),
        "transactionHash": log.tx_hash.to_hex(),
        "transactionIndex": log.tx_index.to_string(),
    })
}

fn encoded_event_log<E: SolEvent>(address: Address, event: &E, block_number: u64, tx_hash_seed: u8) -> SerializableLog {
    let log_data = event.encode_log_data();
    SerializableLog {
        address,
        topics: log_data.topics().iter().map(|topic| topic.0).collect(),
        data: log_data.data.to_vec(),
        tx_hash: [tx_hash_seed; 32],
        block_hash: [tx_hash_seed.wrapping_add(1); 32],
        block_number,
        ..Default::default()
    }
}

async fn assert_live_events<F, E>(
    schema: &CurvySchema,
    subscription: &str,
    response_field: &str,
    expected: &[Value],
    process: F,
) -> anyhow::Result<()>
where
    F: Future<Output = Result<(), E>>,
    E: Error + Send + Sync + 'static,
{
    let mut stream = schema.execute_stream(subscription);
    let (responses, processing_result) = tokio::time::timeout(RESPONSE_TIMEOUT, async {
        tokio::join!(
            async {
                let mut responses = Vec::with_capacity(expected.len());
                for _ in expected {
                    let response = stream
                        .next()
                        .await
                        .ok_or_else(|| anyhow::anyhow!("Curvy subscription ended before delivering all events"))?;
                    let response = response
                        .into_result()
                        .map_err(|errors| anyhow::anyhow!("GraphQL subscription errors: {errors:?}"))?;
                    responses.push(response.data.into_json()?[response_field].clone());
                }
                anyhow::Ok(responses)
            },
            async {
                tokio::time::sleep(Duration::from_millis(100)).await;
                process.await
            }
        )
    })
    .await?;
    processing_result.map_err(anyhow::Error::new)?;
    assert_eq!(responses?, expected);
    Ok(())
}

#[tokio::test]
async fn test_curvy_events_are_indexed_and_streamed() -> anyhow::Result<()> {
    let anvil = Anvil::new().spawn();
    let signer: PrivateKeySigner = anvil.keys()[0].clone().into();
    let deployer_address = signer.address();
    let provider = ProviderBuilder::new().wallet(signer).connect_http(anvil.endpoint_url());

    let curvy = CurvyContractInstances::deploy_for_testing(provider.clone(), deployer_address).await?;
    let curvy_addresses = curvy.get_contract_addresses();

    let shielded_amount = U256::from(1_000_000_000_000_000_000_u64);
    let owner_hash = U256::from(11);
    let portal_address = curvy
        .portal_factory
        .getEntryPortalAddress(owner_hash, deployer_address)
        .call()
        .await?;
    provider
        .send_transaction(
            TransactionRequest::default()
                .with_to(portal_address)
                .with_value(shielded_amount + DEV_PORTAL_DEPLOYMENT_FEE),
        )
        .await?
        .watch()
        .await?;
    curvy
        .portal_factory
        .deployShieldPortal(
            Note {
                ownerHash: owner_hash,
                token: U256::ONE,
                amount: shielded_amount,
                ephemeralKey: [U256::from(22), U256::from(33)],
                viewTag: 44,
            },
            deployer_address,
        )
        .send()
        .await?
        .watch()
        .await?;

    let latest_block = provider.get_block_number().await?;
    let filter = Filter::new()
        .address(vec![curvy_addresses.aggregator_proxy, curvy_addresses.vault_proxy])
        .from_block(0)
        .to_block(latest_block);
    let chain_logs = provider
        .get_logs(&filter)
        .await?
        .into_iter()
        .map(Log::try_from)
        .collect::<Result<Vec<_>, _>>()?;

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
        last_indexed_block: Set(0),
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

    let token_logs = logs_with_signature(&chain_logs, TokenRegistration::SIGNATURE_HASH);
    anyhow::ensure!(
        !token_logs.is_empty(),
        "Curvy deployment did not emit TokenRegistration"
    );
    let mut expected_token_registrations = Vec::new();
    for log in &token_logs {
        let event = decode_event::<TokenRegistration>(log)?;
        let expected = json!({
            "position": event_position(log, 0),
            "tokenAddress": event.token_address.to_hopr_address().to_hex(),
            "tokenId": event.token_id.to_string(),
        });
        assert_live_events(
            &schema,
            r#"subscription { curvyTokenRegistered { position { block eventItemIndex logIndex transactionHash transactionIndex } tokenAddress tokenId } }"#,
            "curvyTokenRegistered",
            std::slice::from_ref(&expected),
            handlers.collect_log_event(SerializableLog::from(log.clone()), true),
        )
        .await?;
        expected_token_registrations.push(expected);
    }

    let gas_root_logs = logs_with_signature(&chain_logs, CommitmentGasFeeRootUpdated::SIGNATURE_HASH);
    anyhow::ensure!(
        gas_root_logs.len() == 1,
        "expected one CommitmentGasFeeRootUpdated event"
    );
    let gas_root_log = &gas_root_logs[0];
    let gas_root_event = decode_event::<CommitmentGasFeeRootUpdated>(gas_root_log)?;
    let expected_gas_root = json!({
        "position": event_position(gas_root_log, 0),
        "root": gas_root_event.root.to_string(),
    });
    assert_live_events(
        &schema,
        r#"subscription { curvyCommitmentGasFeeRootUpdated { position { block eventItemIndex logIndex transactionHash transactionIndex } root } }"#,
        "curvyCommitmentGasFeeRootUpdated",
        std::slice::from_ref(&expected_gas_root),
        handlers.collect_log_event(SerializableLog::from(gas_root_log.clone()), true),
    )
    .await?;

    let gas_cost_logs = logs_with_signature(&chain_logs, CommitmentGasCostsUpdated::SIGNATURE_HASH);
    anyhow::ensure!(gas_cost_logs.len() == 1, "expected one CommitmentGasCostsUpdated event");
    let gas_cost_log = &gas_cost_logs[0];
    let gas_cost_event = decode_event::<CommitmentGasCostsUpdated>(gas_cost_log)?;
    let expected_gas_costs = gas_cost_event
        .gasFees
        .iter()
        .enumerate()
        .map(|(item_index, fees)| {
            json!({
                "gasFees": {
                    "pendingNoteCommitment": fees.pendingNoteCommitment.to_string(),
                    "portalDeployment": fees.portalDeployment.to_string(),
                    "tokenId": fees.tokenId.to_string(),
                    "withdrawal": fees.withdrawal.to_string(),
                },
                "position": event_position(gas_cost_log, item_index),
                "root": gas_cost_event.root.to_string(),
            })
        })
        .collect::<Vec<_>>();
    assert_live_events(
        &schema,
        r#"subscription { curvyCommitmentGasCostsUpdated { gasFees { pendingNoteCommitment portalDeployment tokenId withdrawal } position { block eventItemIndex logIndex transactionHash transactionIndex } root } }"#,
        "curvyCommitmentGasCostsUpdated",
        &expected_gas_costs,
        handlers.collect_log_event(SerializableLog::from(gas_cost_log.clone()), true),
    )
    .await?;

    let pending_logs = logs_with_signature(&chain_logs, PendingNotes::SIGNATURE_HASH);
    anyhow::ensure!(
        pending_logs.len() == 1,
        "autoShield did not emit one PendingNotes event"
    );
    let pending_log = &pending_logs[0];
    let pending_event = decode_event::<PendingNotes>(pending_log)?;
    let expected_pending_notes = pending_event
        .noteIds
        .iter()
        .enumerate()
        .map(|(index, note_id)| {
            json!({
                "amount": pending_event.amounts[index].to_string(),
                "ephemeralKey": [
                    pending_event.ephemeralKeys[0][index].to_string(),
                    pending_event.ephemeralKeys[1][index].to_string(),
                ],
                "isPlaintext": pending_event.isPlaintext[index],
                "noteId": note_id.to_string(),
                "position": event_position(pending_log, index),
                "tokenId": pending_event.tokens[index].to_string(),
                "viewTag": pending_event.viewTags[index],
            })
        })
        .collect::<Vec<_>>();
    assert_live_events(
        &schema,
        r#"subscription { curvyPendingNote { amount ephemeralKey isPlaintext noteId position { block eventItemIndex logIndex transactionHash transactionIndex } tokenId viewTag } }"#,
        "curvyPendingNote",
        &expected_pending_notes,
        handlers.collect_log_event(SerializableLog::from(pending_log.clone()), true),
    )
    .await?;

    let filtered_committed_notes_log = encoded_event_log(
        contract_addresses.curvy_aggregator,
        &CommittedNotes {
            batchIndex: U256::from(6),
            noteIds: vec![U256::from(100)],
        },
        latest_block,
        0x30,
    );
    let committed_notes_event = CommittedNotes {
        batchIndex: U256::from(7),
        noteIds: vec![U256::from(101), U256::from(102)],
    };
    let committed_notes_log = encoded_event_log(
        contract_addresses.curvy_aggregator,
        &committed_notes_event,
        latest_block + 1,
        0x31,
    );
    let expected_committed_notes = committed_notes_event
        .noteIds
        .iter()
        .enumerate()
        .map(|(item_index, note_id)| {
            json!({
                "batchIndex": committed_notes_event.batchIndex.to_string(),
                "noteId": note_id.to_string(),
                "position": event_position(&Log::from(committed_notes_log.clone()), item_index),
            })
        })
        .collect::<Vec<_>>();
    assert_live_events(
        &schema,
        &format!(
            "subscription {{ curvyCommittedNote(fromBlock: \"{}\") {{ batchIndex noteId position {{ block eventItemIndex logIndex transactionHash transactionIndex }} }} }}",
            latest_block + 1
        ),
        "curvyCommittedNote",
        &expected_committed_notes,
        async {
            handlers
                .collect_log_event(filtered_committed_notes_log.clone(), true)
                .await?;
            handlers.collect_log_event(committed_notes_log.clone(), true).await
        },
    )
    .await?;

    let committed_nullifiers_event = CommittedNullifiers {
        batchIndex: U256::from(8),
        nullifiers: vec![U256::from(201), U256::from(202)],
    };
    let committed_nullifiers_log = encoded_event_log(
        contract_addresses.curvy_aggregator,
        &committed_nullifiers_event,
        latest_block + 2,
        0x32,
    );
    let expected_committed_nullifiers = committed_nullifiers_event
        .nullifiers
        .iter()
        .enumerate()
        .map(|(item_index, nullifier)| {
            json!({
                "batchIndex": committed_nullifiers_event.batchIndex.to_string(),
                "nullifier": nullifier.to_string(),
                "position": event_position(&Log::from(committed_nullifiers_log.clone()), item_index),
            })
        })
        .collect::<Vec<_>>();
    assert_live_events(
        &schema,
        r#"subscription { curvyCommittedNullifier { batchIndex nullifier position { block eventItemIndex logIndex transactionHash transactionIndex } } }"#,
        "curvyCommittedNullifier",
        &expected_committed_nullifiers,
        handlers.collect_log_event(committed_nullifiers_log.clone(), true),
    )
    .await?;

    let history_response = schema
        .execute(
            r#"
                query {
                    curvyCommitmentGasCostUpdates(first: 10) { gasFees { pendingNoteCommitment portalDeployment tokenId withdrawal } position { block eventItemIndex logIndex transactionHash transactionIndex } root }
                    curvyCommitmentGasFeeRootUpdates(first: 10) { position { block eventItemIndex logIndex transactionHash transactionIndex } root }
                    curvyCommittedNotes(first: 10) { batchIndex noteId position { block eventItemIndex logIndex transactionHash transactionIndex } }
                    curvyCommittedNullifiers(first: 10) { batchIndex nullifier position { block eventItemIndex logIndex transactionHash transactionIndex } }
                    curvyPendingNotes(first: 10) { amount ephemeralKey isPlaintext noteId position { block eventItemIndex logIndex transactionHash transactionIndex } tokenId viewTag }
                    curvyTokenRegistrations(first: 10) { position { block eventItemIndex logIndex transactionHash transactionIndex } tokenAddress tokenId }
                }
            "#,
        )
        .await
        .into_result()
        .map_err(|errors| anyhow::anyhow!("GraphQL query errors: {errors:?}"))?;
    let history_data = history_response.data.into_json()?;
    insta::assert_yaml_snapshot!(history_data);

    let cursor_page_response = schema
        .execute(format!(
            r#"
                query {{
                    curvyCommittedNotes(
                        after: {{ block: "{}", transactionIndex: "0", logIndex: "0", eventItemIndex: "0" }}
                        first: 1
                    ) {{
                        batchIndex
                        noteId
                        position {{ block eventItemIndex logIndex transactionHash transactionIndex }}
                    }}
                }}
            "#,
            latest_block + 1
        ))
        .await
        .into_result()
        .map_err(|errors| anyhow::anyhow!("GraphQL cursor query errors: {errors:?}"))?;
    insta::assert_yaml_snapshot!(
        "curvy_cursor_continues_within_log",
        cursor_page_response.data.into_json()?
    );

    for log in token_logs {
        handlers.collect_log_event(SerializableLog::from(log), true).await?;
    }
    handlers
        .collect_log_event(SerializableLog::from(gas_root_log.clone()), true)
        .await?;
    handlers
        .collect_log_event(SerializableLog::from(gas_cost_log.clone()), true)
        .await?;
    handlers
        .collect_log_event(SerializableLog::from(pending_log.clone()), true)
        .await?;
    handlers.collect_log_event(committed_notes_log, true).await?;
    handlers.collect_log_event(committed_nullifiers_log, true).await?;
    assert_eq!(
        curvy_token_registration::Entity::find()
            .count(db.conn(TargetDb::Index))
            .await?,
        u64::try_from(expected_token_registrations.len())?
    );
    assert_eq!(
        curvy_commitment_gas_fee_root::Entity::find()
            .count(db.conn(TargetDb::Index))
            .await?,
        1
    );
    assert_eq!(
        curvy_commitment_gas_cost::Entity::find()
            .count(db.conn(TargetDb::Index))
            .await?,
        u64::try_from(expected_gas_costs.len())?
    );
    assert_eq!(
        curvy_pending_note::Entity::find()
            .count(db.conn(TargetDb::Index))
            .await?,
        u64::try_from(expected_pending_notes.len())?
    );
    assert_eq!(
        curvy_committed_note::Entity::find()
            .count(db.conn(TargetDb::Index))
            .await?,
        u64::try_from(expected_committed_notes.len() + 1)?,
        "replaying a CommittedNotes log must not duplicate its rows"
    );
    assert_eq!(
        curvy_committed_nullifier::Entity::find()
            .count(db.conn(TargetDb::Index))
            .await?,
        u64::try_from(expected_committed_nullifiers.len())?,
        "replaying a CommittedNullifiers log must not duplicate its rows"
    );

    Ok(())
}
