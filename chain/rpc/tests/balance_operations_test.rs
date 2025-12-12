//! Integration tests for balance and allowance query operations

mod common;

use std::time::Duration;

use alloy::{
    network::Ethereum,
    primitives::U256,
    providers::Provider,
    rpc::client::ClientBuilder,
    transports::{http::ReqwestTransport, layers::RetryBackoffLayer},
};
use blokli_chain_rpc::{HoprIndexerRpcOperations, rpc::RpcOperationsConfig};
use blokli_chain_types::{ContractAddresses, ContractInstances, utils::create_native_transfer};
use common::{
    TEST_BLOCK_TIME, TEST_FINALITY, create_rpc_client_to_anvil, create_test_rpc_operations, wait_for_finality,
    wait_until_tx,
};
use hex_literal::hex;
use hopr_crypto_types::keypairs::{ChainKeypair, Keypair};
use hopr_primitive_types::prelude::{Address, HoprBalance, XDaiBalance};
use lazy_static::lazy_static;

lazy_static! {
    static ref RANDY: Address = hex!("762614a5ed652457a2f1cdb8006380530c26ae6a").into();
}

#[tokio::test]
async fn test_get_balance_native() -> anyhow::Result<()> {
    let _ = env_logger::builder().is_test(true).try_init();

    let anvil = blokli_chain_types::utils::create_anvil(Some(TEST_BLOCK_TIME));
    let chain_key_0 = ChainKeypair::from_secret(anvil.keys()[0].to_bytes().as_ref())?;

    // Create wallet-enabled client for transaction sending
    let provider = create_rpc_client_to_anvil(&anvil, &chain_key_0);

    // Wait until contracts deployments are final
    wait_for_finality(TEST_FINALITY, TEST_BLOCK_TIME).await;

    // Also create RpcOperations for balance checking
    let transport_client = ReqwestTransport::new(anvil.endpoint_url());
    let rpc_client = ClientBuilder::default()
        .layer(RetryBackoffLayer::new(2, 100, 100))
        .transport(transport_client.clone(), transport_client.guess_local());

    let rpc = create_test_rpc_operations(rpc_client, transport_client.client().clone(), anvil.chain_id(), None)?;

    let balance_1: XDaiBalance = rpc.get_xdai_balance((&chain_key_0).into()).await?;
    assert!(balance_1.amount().gt(&0.into()), "balance must be greater than 0");

    // Send 1 ETH to some random address using wallet-enabled provider
    let tx = create_native_transfer::<Ethereum>(*RANDY, U256::from(1_u32));
    let pending_tx = provider.send_transaction(tx).await?;

    wait_until_tx(
        pending_tx
            .with_required_confirmations(TEST_FINALITY.into())
            .register()
            .await?,
        Duration::from_secs(8),
    )
    .await;

    let balance_2: XDaiBalance = rpc.get_xdai_balance((&chain_key_0).into()).await?;
    assert!(balance_2.lt(&balance_1), "balance must be diminished");

    Ok(())
}

#[tokio::test]
async fn test_get_balance_token() -> anyhow::Result<()> {
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

    let amount = 1024_u64;
    let _ = blokli_chain_types::utils::mint_tokens(contract_instances.token, U256::from(amount)).await;

    let transport_client = ReqwestTransport::new(anvil.endpoint_url());

    let rpc_client = ClientBuilder::default()
        .layer(RetryBackoffLayer::new(2, 100, 100))
        .transport(transport_client.clone(), transport_client.guess_local());

    // Wait until contracts deployments are final
    wait_for_finality(TEST_FINALITY, TEST_BLOCK_TIME).await;

    let rpc = blokli_chain_rpc::rpc::RpcOperations::new(rpc_client, transport_client.client().clone(), cfg, None)?;

    let balance: HoprBalance = rpc.get_hopr_balance((&chain_key_0).into()).await?;
    assert_eq!(amount, balance.amount().as_u64(), "invalid balance");

    Ok(())
}

#[tokio::test]
async fn test_get_hopr_allowance() -> anyhow::Result<()> {
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

    let owner: Address = (&chain_key_0).into();
    let spender = *RANDY;

    // Initially, allowance should be zero
    let allowance = rpc.get_hopr_allowance(owner, spender).await?;
    assert_eq!(allowance.amount().as_u64(), 0, "initial allowance should be zero");

    Ok(())
}
