//! Integration tests for blockchain log streaming functionality.
//!
//! This module contains tests that verify the proper operation of log streaming
//! from the blockchain, including filtering and event detection for channel operations.

mod common;

use std::time::Duration;

use alloy::{
    primitives::{Address as AlloyAddress, U256},
    rpc::{client::ClientBuilder, types::Filter},
    sol_types::SolEvent,
    transports::{http::ReqwestTransport, layers::RetryBackoffLayer},
};
use anyhow::Context;
use blokli_chain_rpc::{
    FilterSet, HoprIndexerRpcOperations,
    client::create_rpc_client_to_anvil,
    errors::RpcError,
    rpc::{RpcOperations, RpcOperationsConfig},
};
use blokli_chain_types::{AlloyAddressExt, ContractAddresses, ContractInstances, utils::create_anvil};
use common::{TEST_BLOCK_TIME, TEST_TX_POLLING_INTERVAL, wait_for_finality};
use futures::StreamExt;
use hopr_async_runtime::prelude::spawn;
use hopr_bindings::{
    hopr_channels_events::HoprChannelsEvents::{ChannelBalanceIncreased, ChannelOpened},
    hopr_token::HoprToken::{Approval, Transfer},
};
use hopr_crypto_types::keypairs::{ChainKeypair, Keypair};
use tokio::time::timeout;
use tracing::debug;

#[tokio::test]
async fn test_try_stream_logs_should_contain_all_logs_when_opening_channel() -> anyhow::Result<()> {
    let _ = env_logger::builder().is_test(true).try_init();

    let expected_block_time = TEST_BLOCK_TIME;

    let anvil = create_anvil(Some(expected_block_time));
    let chain_key_0 = ChainKeypair::from_secret(anvil.keys()[0].to_bytes().as_ref())?;
    let chain_key_1 = ChainKeypair::from_secret(anvil.keys()[1].to_bytes().as_ref())?;

    // Deploy contracts
    let contract_instances = {
        let client = create_rpc_client_to_anvil(&anvil, &chain_key_0);
        ContractInstances::deploy_for_testing(client, &chain_key_0).await?
    };

    let contract_addrs = ContractAddresses::from(&contract_instances);

    let filter_token_approval = Filter::new()
        .address(AlloyAddress::from_hopr_address(contract_addrs.token))
        .event_signature(Approval::SIGNATURE_HASH);
    let filter_token_transfer = Filter::new()
        .address(AlloyAddress::from_hopr_address(contract_addrs.token))
        .event_signature(Transfer::SIGNATURE_HASH);
    let filter_channels_opened = Filter::new()
        .address(AlloyAddress::from_hopr_address(contract_addrs.channels))
        .event_signature(ChannelOpened::SIGNATURE_HASH);
    let filter_channels_balance_increased = Filter::new()
        .address(AlloyAddress::from_hopr_address(contract_addrs.channels))
        .event_signature(ChannelBalanceIncreased::SIGNATURE_HASH);

    let log_filter = FilterSet {
        all: vec![
            filter_token_approval.clone(),
            filter_token_transfer.clone(),
            filter_channels_opened.clone(),
            filter_channels_balance_increased.clone(),
        ],
        token: vec![filter_token_approval, filter_token_transfer],
        no_token: vec![filter_channels_opened, filter_channels_balance_increased],
    };

    debug!("{:#?}", contract_addrs);
    debug!("{:#?}", log_filter);

    let tokens_minted_at =
        blokli_chain_types::utils::mint_tokens(contract_instances.token.clone(), U256::from(1000_u128))
            .await?
            .unwrap();
    debug!("tokens were minted at block {tokens_minted_at}");

    let transport_client = ReqwestTransport::new(anvil.endpoint_url());

    let rpc_client = ClientBuilder::default()
        .layer(RetryBackoffLayer::new(2, 100, 100))
        .transport(transport_client.clone(), transport_client.guess_local());

    let finality = 8_u32;
    let cfg = RpcOperationsConfig {
        tx_polling_interval: TEST_TX_POLLING_INTERVAL,
        contract_addrs,
        expected_block_time,
        finality,
        gas_oracle_url: None,
        ..RpcOperationsConfig::default()
    };

    // Wait until contracts deployments are final
    wait_for_finality(finality, expected_block_time).await;

    let rpc = RpcOperations::new(rpc_client, transport_client.client().clone(), cfg, None)?;

    // Spawn stream
    let count_filtered_topics = 2;
    let retrieved_logs = spawn(async move {
        Ok::<_, RpcError>(
            rpc.try_stream_logs(1, log_filter, false)?
                .skip_while(|b| futures::future::ready(b.len() != count_filtered_topics))
                .next()
                .await,
        )
    });

    // Spawn channel funding
    blokli_chain_types::utils::fund_channel(
        chain_key_1.public().to_address(),
        contract_instances.token,
        contract_instances.channels,
        U256::from(1_u128),
    )
    .await?;

    let retrieved_logs = timeout(Duration::from_secs(30), retrieved_logs) // Give up after 30 seconds
        .await
        .context("log streaming timed out")?
        .context("log stream task failed")?;

    let blocks = retrieved_logs
        .context("log stream task returned error")?
        .context("log stream yielded no blocks")?;
    let last_block_logs = blocks.logs.clone();

    let channel_open_filter = ChannelOpened::SIGNATURE_HASH;
    let channel_balance_filter = ChannelBalanceIncreased::SIGNATURE_HASH;

    debug!(
        "channel_open_filter: {:?} - {:?}",
        channel_open_filter,
        channel_open_filter.0.to_vec()
    );
    debug!(
        "channel_balance_filter: {:?} - {:?}",
        channel_balance_filter,
        channel_balance_filter.0.to_vec()
    );
    debug!("logs: {:#?}", last_block_logs);

    assert!(
        last_block_logs
            .iter()
            .any(|log| log.address == contract_addrs.channels && log.topics.contains(&channel_open_filter.into())),
        "must contain channel open"
    );
    assert!(
        last_block_logs
            .iter()
            .any(|log| log.address == contract_addrs.channels && log.topics.contains(&channel_balance_filter.into())),
        "must contain channel balance increase"
    );

    Ok(())
}

#[tokio::test]
async fn test_try_stream_logs_should_contain_only_channel_logs_when_filtered_on_funding_channel() -> anyhow::Result<()>
{
    let _ = env_logger::builder().is_test(true).try_init();

    let expected_block_time = TEST_BLOCK_TIME;

    let anvil = create_anvil(Some(expected_block_time));
    let chain_key_0 = ChainKeypair::from_secret(anvil.keys()[0].to_bytes().as_ref())?;
    let chain_key_1 = ChainKeypair::from_secret(anvil.keys()[1].to_bytes().as_ref())?;

    // Deploy contracts
    let contract_instances = {
        let client = create_rpc_client_to_anvil(&anvil, &chain_key_0);
        ContractInstances::deploy_for_testing(client, &chain_key_0).await?
    };

    let tokens_minted_at =
        blokli_chain_types::utils::mint_tokens(contract_instances.token.clone(), U256::from(1000_u128))
            .await?
            .unwrap();
    debug!("tokens were minted at block {tokens_minted_at}");

    let contract_addrs = ContractAddresses::from(&contract_instances);

    let finality = 2_u32;
    let cfg = RpcOperationsConfig {
        tx_polling_interval: TEST_TX_POLLING_INTERVAL,
        contract_addrs,
        expected_block_time,
        finality,
        gas_oracle_url: None,
        ..RpcOperationsConfig::default()
    };

    let transport_client = ReqwestTransport::new(anvil.endpoint_url());

    let rpc_client = ClientBuilder::default()
        .layer(RetryBackoffLayer::new(2, 100, 100))
        .transport(transport_client.clone(), transport_client.guess_local());

    // Wait until contracts deployments are final
    wait_for_finality(finality, expected_block_time).await;

    let rpc = RpcOperations::new(rpc_client, transport_client.client().clone(), cfg, None)?;

    let filter_channels_opened = Filter::new()
        .address(AlloyAddress::from_hopr_address(contract_addrs.channels))
        .event_signature(ChannelOpened::SIGNATURE_HASH);
    let filter_channels_balance_increased = Filter::new()
        .address(AlloyAddress::from_hopr_address(contract_addrs.channels))
        .event_signature(ChannelBalanceIncreased::SIGNATURE_HASH);

    let log_filter = FilterSet {
        all: vec![
            filter_channels_opened.clone(),
            filter_channels_balance_increased.clone(),
        ],
        token: vec![],
        no_token: vec![filter_channels_opened, filter_channels_balance_increased],
    };

    debug!("{:#?}", contract_addrs);
    debug!("{:#?}", log_filter);

    // Spawn stream
    let count_filtered_topics = 2;
    let retrieved_logs = spawn(async move {
        Ok::<_, RpcError>(
            rpc.try_stream_logs(1, log_filter, false)?
                .skip_while(|b| futures::future::ready(b.len() != count_filtered_topics))
                .next()
                .await,
        )
    });

    // Spawn channel funding
    blokli_chain_types::utils::fund_channel(
        chain_key_1.public().to_address(),
        contract_instances.token,
        contract_instances.channels,
        U256::from(1_u128),
    )
    .await?;

    let retrieved_logs = timeout(Duration::from_secs(30), retrieved_logs) // Give up after 30 seconds
        .await???;

    // The last block must contain all 2 events
    let last_block_logs = retrieved_logs.context("log stream yielded no blocks")?.logs;

    let channel_open_filter = ChannelOpened::SIGNATURE_HASH;
    let channel_balance_filter = ChannelBalanceIncreased::SIGNATURE_HASH;

    assert!(
        last_block_logs
            .iter()
            .any(|log| log.address == contract_addrs.channels && log.topics.contains(&channel_open_filter.into())),
        "must contain channel open"
    );
    assert!(
        last_block_logs
            .iter()
            .any(|log| log.address == contract_addrs.channels && log.topics.contains(&channel_balance_filter.into())),
        "must contain channel balance increase"
    );

    Ok(())
}
