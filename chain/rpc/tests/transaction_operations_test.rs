//! Integration tests for transaction operations including gas estimation and transaction sending

mod common;

use std::time::Duration;

use blokli_chain_rpc::HoprIndexerRpcOperations;
use blokli_chain_types::utils::create_native_transfer;
use common::{
    TEST_BLOCK_TIME, TEST_FINALITY, create_rpc_client_to_anvil, create_test_rpc_operations, wait_for_finality,
    wait_until_tx,
};
use hex_literal::hex;
use hopr_async_runtime::prelude::sleep;
use hopr_bindings::exports::alloy::{
    network::Ethereum,
    primitives::U256,
    providers::Provider,
    rpc::client::ClientBuilder,
    transports::{http::ReqwestTransport, layers::RetryBackoffLayer},
};
use hopr_crypto_types::keypairs::{ChainKeypair, Keypair};
use hopr_primitive_types::prelude::{Address, XDaiBalance};
use lazy_static::lazy_static;

lazy_static! {
    static ref RANDY: Address = hex!("762614a5ed652457a2f1cdb8006380530c26ae6a").into();
}

/// Tests that provider-native gas estimation methods work correctly without using custom gas oracle.
///
/// This is a **negative test** that verifies:
/// 1. Provider methods like `estimate_eip1559_fees()` and `get_gas_price()` make direct Ethereum RPC calls
/// 2. These methods do NOT trigger the custom gas oracle endpoint (mock expects 0 calls)
/// 3. The custom gas oracle (via GasOracleFiller) is only used when sending transactions through a provider that was
///    configured with ProviderBuilder and the GasOracleFiller
///
/// See `gas_oracle_test.rs` for positive tests where GasOracleFiller IS used during transaction sending.
#[tokio::test]
async fn test_should_estimate_tx() -> anyhow::Result<()> {
    let _ = env_logger::builder().is_test(true).try_init();

    let anvil = blokli_chain_types::utils::create_anvil(Some(TEST_BLOCK_TIME));
    let _chain_key_0 = ChainKeypair::from_secret(anvil.keys()[0].to_bytes().as_ref())?;

    // Set up gas oracle mock that expects to never be called
    // This verifies that provider gas estimation does not use the custom gas oracle
    let mut server = mockito::Server::new_async().await;
    let gas_oracle_mock = server
        .mock("GET", "/gas_oracle")
        .with_status(200)
        .with_body(r#"{"status":"1","message":"OK","result":{"LastBlock":"38791478","SafeGasPrice":"1.1","ProposeGasPrice":"1.1","FastGasPrice":"1.6","UsdPrice":"0.999985432689946"}}"#)
        .expect(0) // Expect 0 calls - this is intentional!
        .create_async()
        .await;

    let transport_client = ReqwestTransport::new(anvil.endpoint_url());
    let rpc_client = ClientBuilder::default()
        .layer(RetryBackoffLayer::new(2, 100, 100))
        .transport(transport_client.clone(), transport_client.guess_local());

    // Wait until contracts deployments are final
    wait_for_finality(TEST_FINALITY, TEST_BLOCK_TIME).await;

    // Create RpcOperations with gas oracle URL configured
    // However, the provider doesn't have GasOracleFiller, so it won't use the gas oracle
    let rpc = create_test_rpc_operations(
        rpc_client,
        transport_client.client().clone(),
        anvil.chain_id(),
        Some((server.url() + "/gas_oracle").parse()?),
    )?;

    // Call provider gas estimation methods - these use direct Ethereum RPC (eth_feeHistory, eth_gasPrice)
    let fees = rpc.provider.estimate_eip1559_fees().await?;

    assert!(
        fees.max_priority_fee_per_gas.ge(&0_u128),
        "estimated_max_priority_fee must be equal or greater than 0, 0.1 gwei"
    );

    let estimated_gas_price = rpc.provider.get_gas_price().await?;
    assert!(
        estimated_gas_price.ge(&100_000_000_u128),
        "estimated_max_fee must be greater than 0.1 gwei"
    );

    // Verify the gas oracle mock was never called (expect(0))
    // This confirms that provider gas estimation uses direct Ethereum RPC, not the custom gas oracle
    gas_oracle_mock.assert();

    Ok(())
}

#[tokio::test]
async fn test_should_send_tx() -> anyhow::Result<()> {
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

    // Test 1: Send ETH to some random address, do not wait for confirmation
    let tx_1 = create_native_transfer::<Ethereum>(*RANDY, U256::from(1000000_u32));
    let pending_tx = provider.send_transaction(tx_1).await?;

    wait_until_tx(
        pending_tx
            .with_required_confirmations(TEST_FINALITY.into())
            .register()
            .await?,
        Duration::from_secs(8),
    )
    .await;

    // Test 2: Send ETH to some random address, wait for confirmation
    let tx_2 = create_native_transfer::<Ethereum>(*RANDY, U256::from(1000000_u32));
    let _receipt = provider
        .send_transaction(tx_2)
        .await?
        .with_required_confirmations(TEST_FINALITY.into())
        .get_receipt()
        .await?;

    Ok(())
}

#[tokio::test]
async fn test_should_send_consecutive_txs() -> anyhow::Result<()> {
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

    let txs_count = 5_u64;
    let send_amount = 1000000_u64;

    // Send ETH to some random address
    futures::future::join_all((0..txs_count).map(|_| async {
        provider
            .send_transaction(create_native_transfer::<Ethereum>(*RANDY, U256::from(send_amount)))
            .await
            .expect("tx should be sent")
            .with_required_confirmations(TEST_FINALITY.into())
            .watch()
            .await
            .expect("tx should resolve")
    }))
    .await;

    sleep(TEST_BLOCK_TIME * (1 + TEST_FINALITY)).await;

    let balance_2: XDaiBalance = rpc.get_xdai_balance((&chain_key_0).into()).await?;

    assert!(
        balance_2.amount() <= balance_1.amount() - txs_count * send_amount,
        "balance must be less"
    );

    Ok(())
}
