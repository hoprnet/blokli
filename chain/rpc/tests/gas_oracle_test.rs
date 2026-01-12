//! Integration tests for gas oracle functionality.
//!
//! This module contains tests that verify the proper operation of the gas oracle
//! integration, including EIP-1559 and legacy transaction gas price estimation.

mod common;

use blokli_chain_rpc::client::{
    EIP1559_FEE_ESTIMATION_DEFAULT_MAX_FEE_GNOSIS, EIP1559_FEE_ESTIMATION_DEFAULT_PRIORITY_FEE_GNOSIS, GasOracleFiller,
};
use blokli_chain_types::utils::create_anvil;
use hopr_bindings::exports::alloy::{
    network::TransactionBuilder,
    primitives::{U256, address},
    providers::{
        Provider, ProviderBuilder,
        fillers::{BlobGasFiller, CachedNonceManager, ChainIdFiller, GasFiller, NonceFiller},
    },
    rpc::{client::ClientBuilder, types::TransactionRequest},
    signers::local::PrivateKeySigner,
    transports::{http::ReqwestTransport, layers::RetryBackoffLayer},
};

/// Tests that GasOracleFiller handles EIP-1559 transactions using fallback fees.
///
/// **Note**: GasOracleFiller does NOT query the gas oracle for EIP-1559 transactions.
/// Due to Foundry/Anvil limitations with EIP-1559 fee estimation, it returns hardcoded
/// fallback values instead. This test verifies that behavior by expecting 0 oracle calls.
///
/// See: https://github.com/foundry-rs/foundry/issues/5709
#[tokio::test]
async fn test_client_should_use_fallback_for_eip1559_tx() -> anyhow::Result<()> {
    let _ = env_logger::builder().is_test(true).try_init();

    let mut server = mockito::Server::new_async().await;

    // Expect 0 calls - GasOracleFiller uses fallback fees for EIP-1559, doesn't query oracle
    let m = server
        .mock("GET", "/gasapi.ashx?apikey=key&method=gasoracle")
        .with_status(http::StatusCode::ACCEPTED.as_u16().into())
        .with_body(r#"{"status":"1","message":"OK","result":{"LastBlock":"39864926","SafeGasPrice":"1.1","ProposeGasPrice":"1.1","FastGasPrice":"1.6","UsdPrice":"0.999968207972734"}}"#)
        .expect(0)
        .create();

    let anvil = create_anvil(None);
    let signer: PrivateKeySigner = anvil.keys()[0].clone().into();

    let transport_client = ReqwestTransport::new(anvil.endpoint_url());

    let rpc_client = ClientBuilder::default()
        .layer(RetryBackoffLayer::new(2, 100, 100))
        .transport(transport_client.clone(), transport_client.guess_local());

    let provider = ProviderBuilder::new()
        .disable_recommended_fillers()
        .wallet(signer)
        .filler(ChainIdFiller::default())
        .filler(NonceFiller::new(CachedNonceManager::default()))
        .filler(GasOracleFiller::new(
            transport_client.client().clone(),
            Some((server.url() + "/gasapi.ashx?apikey=key&method=gasoracle").parse()?),
            EIP1559_FEE_ESTIMATION_DEFAULT_MAX_FEE_GNOSIS,
            EIP1559_FEE_ESTIMATION_DEFAULT_PRIORITY_FEE_GNOSIS,
        ))
        .filler(GasFiller)
        .connect_client(rpc_client);

    let tx = TransactionRequest::default()
        .to(address!("d8dA6BF26964aF9D7eEd9e03E53415D37aA96045"))
        .value(U256::from(100))
        .transaction_type(2);

    let receipt = provider.send_transaction(tx).await?.get_receipt().await?;

    m.assert();
    assert_eq!(receipt.gas_used, 21000);
    Ok(())
}

#[tokio::test]
async fn test_client_should_call_on_gas_oracle_for_legacy_tx() -> anyhow::Result<()> {
    let _ = env_logger::builder().is_test(true).try_init();

    let mut server = mockito::Server::new_async().await;

    let m = server
        .mock("GET", "/gasapi.ashx?apikey=key&method=gasoracle")
        .with_status(http::StatusCode::ACCEPTED.as_u16().into())
        .with_body(r#"{"status":"1","message":"OK","result":{"LastBlock":"39864926","SafeGasPrice":"1.1","ProposeGasPrice":"3.5","FastGasPrice":"1.6","UsdPrice":"0.999968207972734"}}"#)
        .expect(1)
        .create();

    let anvil = create_anvil(None);
    let signer: PrivateKeySigner = anvil.keys()[0].clone().into();

    let transport_client = ReqwestTransport::new(anvil.endpoint_url());

    let rpc_client = ClientBuilder::default()
        .layer(RetryBackoffLayer::new(2, 100, 100))
        .transport(transport_client.clone(), transport_client.guess_local());

    let provider = ProviderBuilder::new()
        .disable_recommended_fillers()
        .wallet(signer)
        .filler(ChainIdFiller::default())
        .filler(NonceFiller::new(CachedNonceManager::default()))
        .filler(GasOracleFiller::new(
            transport_client.client().clone(),
            Some((server.url() + "/gasapi.ashx?apikey=key&method=gasoracle").parse()?),
            EIP1559_FEE_ESTIMATION_DEFAULT_MAX_FEE_GNOSIS,
            EIP1559_FEE_ESTIMATION_DEFAULT_PRIORITY_FEE_GNOSIS,
        ))
        .filler(GasFiller)
        .filler(BlobGasFiller::default())
        .connect_client(rpc_client);

    // ChainIdFiller is required for proper legacy transaction handling with gas_price
    let tx = TransactionRequest::default()
        .with_to(address!("d8dA6BF26964aF9D7eEd9e03E53415D37aA96045"))
        .with_value(U256::from(100))
        .with_gas_price(1000000000);

    let receipt = provider.send_transaction(tx).await?.get_receipt().await?;

    m.assert();
    assert_eq!(receipt.gas_used, 21000);
    Ok(())
}
