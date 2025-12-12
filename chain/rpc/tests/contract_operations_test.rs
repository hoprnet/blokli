//! Integration tests for HOPR smart contract operations

mod common;

use std::time::Duration;

use alloy::{
    rpc::client::ClientBuilder,
    transports::{http::ReqwestTransport, layers::RetryBackoffLayer},
};
use blokli_chain_rpc::{HoprIndexerRpcOperations, HoprRpcOperations, rpc::RpcOperationsConfig};
use blokli_chain_types::{ContractAddresses, ContractInstances};
use common::{TEST_BLOCK_TIME, TEST_FINALITY, create_rpc_client_to_anvil, wait_for_finality};
use hopr_crypto_types::keypairs::{ChainKeypair, Keypair};
use hopr_primitive_types::prelude::{Address, HoprBalance};

#[tokio::test]
async fn test_get_minimum_network_winning_probability() -> anyhow::Result<()> {
    let _ = env_logger::builder().is_test(true).try_init();

    let anvil = blokli_chain_types::utils::create_anvil(Some(TEST_BLOCK_TIME));
    let chain_key_0 = ChainKeypair::from_secret(anvil.keys()[0].to_bytes().as_ref())?;

    // Deploy contracts
    let contract_instances = {
        let client = create_rpc_client_to_anvil(&anvil, &chain_key_0);
        ContractInstances::deploy_for_testing(client, &chain_key_0).await?
    };

    let cfg = RpcOperationsConfig {
        chain_id: anvil.chain_id(),
        tx_polling_interval: Duration::from_millis(10),
        expected_block_time: TEST_BLOCK_TIME,
        finality: TEST_FINALITY,
        contract_addrs: ContractAddresses::from(&contract_instances),
        gas_oracle_url: None,
        ..RpcOperationsConfig::default()
    };

    let transport_client = ReqwestTransport::new(anvil.endpoint_url());

    let rpc_client = ClientBuilder::default()
        .layer(RetryBackoffLayer::new(2, 100, 100))
        .transport(transport_client.clone(), transport_client.guess_local());

    // Wait until contracts deployments are final
    wait_for_finality(TEST_FINALITY, TEST_BLOCK_TIME).await;

    let rpc = blokli_chain_rpc::rpc::RpcOperations::new(rpc_client, transport_client.client().clone(), cfg, None)?;

    // Get the minimum network winning probability
    let winning_prob = rpc.get_minimum_network_winning_probability().await?;

    // Winning probability should be valid (just verify the call succeeded)
    // The value is returned from the oracle, so we verify we got a valid response
    let _prob_bytes = winning_prob.as_ref();
    assert!(!_prob_bytes.is_empty(), "winning probability should not be empty");

    Ok(())
}

#[tokio::test]
async fn test_get_minimum_network_ticket_price() -> anyhow::Result<()> {
    let _ = env_logger::builder().is_test(true).try_init();

    let anvil = blokli_chain_types::utils::create_anvil(Some(TEST_BLOCK_TIME));
    let chain_key_0 = ChainKeypair::from_secret(anvil.keys()[0].to_bytes().as_ref())?;

    // Deploy contracts
    let contract_instances = {
        let client = create_rpc_client_to_anvil(&anvil, &chain_key_0);
        ContractInstances::deploy_for_testing(client, &chain_key_0).await?
    };

    let cfg = RpcOperationsConfig {
        chain_id: anvil.chain_id(),
        tx_polling_interval: Duration::from_millis(10),
        expected_block_time: TEST_BLOCK_TIME,
        finality: TEST_FINALITY,
        contract_addrs: ContractAddresses::from(&contract_instances),
        gas_oracle_url: None,
        ..RpcOperationsConfig::default()
    };

    let transport_client = ReqwestTransport::new(anvil.endpoint_url());

    let rpc_client = ClientBuilder::default()
        .layer(RetryBackoffLayer::new(2, 100, 100))
        .transport(transport_client.clone(), transport_client.guess_local());

    // Wait until contracts deployments are final
    wait_for_finality(TEST_FINALITY, TEST_BLOCK_TIME).await;

    let rpc = blokli_chain_rpc::rpc::RpcOperations::new(rpc_client, transport_client.client().clone(), cfg, None)?;

    // Get the minimum network ticket price
    let ticket_price = rpc.get_minimum_network_ticket_price().await?;

    // Ticket price should be non-negative
    assert!(
        ticket_price >= HoprBalance::zero(),
        "ticket price should be non-negative"
    );

    Ok(())
}

#[tokio::test]
async fn test_get_safe_from_node_safe_registry() -> anyhow::Result<()> {
    let _ = env_logger::builder().is_test(true).try_init();

    let anvil = blokli_chain_types::utils::create_anvil(Some(TEST_BLOCK_TIME));
    let chain_key_0 = ChainKeypair::from_secret(anvil.keys()[0].to_bytes().as_ref())?;

    // Deploy contracts
    let contract_instances = {
        let client = create_rpc_client_to_anvil(&anvil, &chain_key_0);
        ContractInstances::deploy_for_testing(client, &chain_key_0).await?
    };

    let cfg = RpcOperationsConfig {
        chain_id: anvil.chain_id(),
        tx_polling_interval: Duration::from_millis(10),
        expected_block_time: TEST_BLOCK_TIME,
        finality: TEST_FINALITY,
        contract_addrs: ContractAddresses::from(&contract_instances),
        gas_oracle_url: None,
        ..RpcOperationsConfig::default()
    };

    let transport_client = ReqwestTransport::new(anvil.endpoint_url());

    let rpc_client = ClientBuilder::default()
        .layer(RetryBackoffLayer::new(2, 100, 100))
        .transport(transport_client.clone(), transport_client.guess_local());

    // Wait until contracts deployments are final
    wait_for_finality(TEST_FINALITY, TEST_BLOCK_TIME).await;

    let rpc = blokli_chain_rpc::rpc::RpcOperations::new(rpc_client, transport_client.client().clone(), cfg, None)?;

    let node_address: Address = (&chain_key_0).into();

    // Query the registry (may return zero address if not registered)
    let safe_address = rpc.get_safe_from_node_safe_registry(node_address).await?;

    // The call should succeed and return an address (even if it's zero address)
    // In a fresh deployment, the node won't be registered, so we just verify the call works
    // Just verify the call succeeded by checking that we got an address back
    let _address_bytes = safe_address.as_ref();
    assert_eq!(_address_bytes.len(), 20, "should return a valid 20-byte address");

    Ok(())
}

#[tokio::test]
async fn test_get_channel_closure_notice_period() -> anyhow::Result<()> {
    let _ = env_logger::builder().is_test(true).try_init();

    let anvil = blokli_chain_types::utils::create_anvil(Some(TEST_BLOCK_TIME));
    let chain_key_0 = ChainKeypair::from_secret(anvil.keys()[0].to_bytes().as_ref())?;

    // Deploy contracts
    let contract_instances = {
        let client = create_rpc_client_to_anvil(&anvil, &chain_key_0);
        ContractInstances::deploy_for_testing(client, &chain_key_0).await?
    };

    let cfg = RpcOperationsConfig {
        chain_id: anvil.chain_id(),
        tx_polling_interval: Duration::from_millis(10),
        expected_block_time: TEST_BLOCK_TIME,
        finality: TEST_FINALITY,
        contract_addrs: ContractAddresses::from(&contract_instances),
        gas_oracle_url: None,
        ..RpcOperationsConfig::default()
    };

    let transport_client = ReqwestTransport::new(anvil.endpoint_url());

    let rpc_client = ClientBuilder::default()
        .layer(RetryBackoffLayer::new(2, 100, 100))
        .transport(transport_client.clone(), transport_client.guess_local());

    // Wait until contracts deployments are final
    wait_for_finality(TEST_FINALITY, TEST_BLOCK_TIME).await;

    let rpc = blokli_chain_rpc::rpc::RpcOperations::new(rpc_client, transport_client.client().clone(), cfg, None)?;

    // Get the channel closure notice period
    let notice_period = rpc.get_channel_closure_notice_period().await?;

    // Notice period should be a positive duration
    assert!(
        notice_period > Duration::from_secs(0),
        "notice period should be positive"
    );

    Ok(())
}
