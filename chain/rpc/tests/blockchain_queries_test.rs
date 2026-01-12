//! Integration tests for blockchain query operations (block numbers, timestamps)

mod common;

use std::time::Duration;

use blokli_chain_rpc::{HoprIndexerRpcOperations, HoprRpcOperations, client::MetricsLayer, rpc::RpcOperationsConfig};
use common::{TEST_BLOCK_TIME, TEST_FINALITY, create_test_rpc_operations, wait_for_finality};
use hopr_async_runtime::prelude::sleep;
use hopr_bindings::exports::alloy::{
    providers::{Provider, ProviderBuilder},
    rpc::client::ClientBuilder,
    signers::local::PrivateKeySigner,
    transports::{http::ReqwestTransport, layers::RetryBackoffLayer},
};
use hopr_crypto_types::keypairs::{ChainKeypair, Keypair};

#[tokio::test]
async fn test_get_timestamp() -> anyhow::Result<()> {
    let _ = env_logger::builder().is_test(true).try_init();

    let anvil = blokli_chain_types::utils::create_anvil(Some(TEST_BLOCK_TIME));
    let _chain_key_0 = ChainKeypair::from_secret(anvil.keys()[0].to_bytes().as_ref())?;

    let transport_client = ReqwestTransport::new(anvil.endpoint_url());

    let rpc_client = ClientBuilder::default()
        .layer(RetryBackoffLayer::new(2, 100, 100))
        .transport(transport_client.clone(), transport_client.guess_local());

    // Wait until contracts deployments are final
    wait_for_finality(TEST_FINALITY, TEST_BLOCK_TIME).await;

    let rpc = create_test_rpc_operations(rpc_client, transport_client.client().clone(), anvil.chain_id(), None)?;

    // Get current block number
    let block_number = rpc.provider.get_block_number().await?;

    // Get timestamp for a block (accounting for finality)
    let timestamp = rpc.get_timestamp(block_number).await?;
    assert!(timestamp.is_some(), "timestamp should exist for valid block");

    // Verify timestamp is reasonable (Unix timestamp)
    let ts = timestamp.unwrap();
    assert!(ts > 1_000_000_000, "timestamp should be a valid Unix timestamp");

    Ok(())
}

#[tokio::test]
async fn test_client_should_get_block_number() -> anyhow::Result<()> {
    let block_time = Duration::from_millis(1100);

    let anvil = blokli_chain_types::utils::create_anvil(Some(block_time));
    let signer: PrivateKeySigner = anvil.keys()[0].clone().into();

    let transport_client = ReqwestTransport::new(anvil.endpoint_url());

    let rpc_client = ClientBuilder::default().transport(transport_client.clone(), transport_client.guess_local());

    let provider = ProviderBuilder::new().wallet(signer).connect_client(rpc_client);

    let mut last_number = 0;

    for _ in 0..3 {
        sleep(block_time).await;

        let num = provider.get_block_number().await?;

        assert!(num > last_number, "next block number must be greater");
        last_number = num;
    }

    Ok(())
}

#[tokio::test]
async fn test_client_should_get_block_number_with_metrics_without_retry() -> anyhow::Result<()> {
    let block_time = Duration::from_secs(1);

    let anvil = blokli_chain_types::utils::create_anvil(Some(block_time));
    let signer: PrivateKeySigner = anvil.keys()[0].clone().into();

    let transport_client = ReqwestTransport::new(anvil.endpoint_url());

    // additional retry layer
    let retry_layer = RetryBackoffLayer::new(2, 100, 100);

    let rpc_client = ClientBuilder::default()
        .layer(retry_layer)
        .layer(MetricsLayer)
        .transport(transport_client.clone(), transport_client.guess_local());

    let provider = ProviderBuilder::new().wallet(signer).connect_client(rpc_client);

    let mut last_number = 0;

    for _ in 0..3 {
        sleep(block_time).await;

        let num = provider.get_block_number().await?;

        assert!(num > last_number, "next block number must be greater");
        last_number = num;
    }

    Ok(())
}

#[tokio::test]
async fn test_should_get_block_number() -> anyhow::Result<()> {
    let expected_block_time = Duration::from_secs(1);
    let anvil = blokli_chain_types::utils::create_anvil(Some(expected_block_time));
    let _chain_key_0 = ChainKeypair::from_secret(anvil.keys()[0].to_bytes().as_ref())?;

    let transport_client = ReqwestTransport::new(anvil.endpoint_url());

    let rpc_client = ClientBuilder::default()
        .layer(RetryBackoffLayer::new(2, 100, 100))
        .transport(transport_client.clone(), transport_client.guess_local());

    let finality = 2_u32;
    let cfg = RpcOperationsConfig {
        finality,
        expected_block_time,
        gas_oracle_url: None,
        ..RpcOperationsConfig::default()
    };

    // Wait until contracts deployments are final
    wait_for_finality(finality, expected_block_time).await;

    let rpc = blokli_chain_rpc::rpc::RpcOperations::new(rpc_client, transport_client.client().clone(), cfg, None)?;

    let b1 = rpc.block_number().await?;

    sleep(expected_block_time * 2).await;

    let b2 = rpc.block_number().await?;

    assert!(b2 > b1, "block number should increase");

    Ok(())
}
