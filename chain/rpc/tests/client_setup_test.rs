//! Integration tests for RPC client setup and initialization.
//!
//! This module contains tests that verify the proper initialization and configuration
//! of RPC clients, including contract deployment, error handling for malformed requests,
//! and snapshot-based testing capabilities.

mod common;

use std::time::Duration;

use blokli_chain_rpc::client::{DefaultRetryPolicy, SnapshotRequestor, SnapshotRequestorLayer};
use blokli_chain_types::{ContractAddresses, ContractInstances, utils::create_anvil};
use hopr_async_runtime::prelude::sleep;
use hopr_bindings::exports::alloy::{
    primitives::U64,
    providers::{Provider, ProviderBuilder},
    rpc::client::ClientBuilder,
    signers::local::PrivateKeySigner,
    transports::{http::ReqwestTransport, layers::RetryBackoffLayer},
};
use hopr_crypto_types::keypairs::{ChainKeypair, Keypair};
use hopr_primitive_types::prelude::Address;
use tempfile::NamedTempFile;

#[tokio::test]
async fn test_client_should_deploy_contracts_via_reqwest() -> anyhow::Result<()> {
    let anvil = create_anvil(None);
    let signer: PrivateKeySigner = anvil.keys()[0].clone().into();
    let signer_chain_key = ChainKeypair::from_secret(signer.to_bytes().as_ref())?;

    let rpc_client = ClientBuilder::default().http(anvil.endpoint_url());

    let provider = ProviderBuilder::new().wallet(signer).connect_client(rpc_client);

    let contracts = ContractInstances::deploy_for_testing(provider.clone(), &signer_chain_key)
        .await
        .expect("deploy failed");

    let contract_addrs = ContractAddresses::from(&contracts);

    assert_ne!(contract_addrs.token, Address::default());
    assert_ne!(contract_addrs.channels, Address::default());
    assert_ne!(contract_addrs.announcements, Address::default());
    assert_ne!(contract_addrs.node_safe_registry, Address::default());
    assert_ne!(contract_addrs.ticket_price_oracle, Address::default());

    Ok(())
}

#[tokio::test]
async fn test_client_should_fail_on_malformed_request() -> anyhow::Result<()> {
    let anvil = create_anvil(None);
    let signer: PrivateKeySigner = anvil.keys()[0].clone().into();

    let transport_client = ReqwestTransport::new(anvil.endpoint_url());

    let rpc_client = ClientBuilder::default().transport(transport_client.clone(), transport_client.guess_local());

    let provider = ProviderBuilder::new().wallet(signer).connect_client(rpc_client);

    let err = provider
        .raw_request::<(), U64>("eth_blockNumber_bla".into(), ())
        .await
        .expect_err("expected error");

    assert!(matches!(
        err,
        hopr_bindings::exports::alloy::transports::RpcError::ErrorResp(..)
    ));

    Ok(())
}

#[test_log::test(tokio::test)]
async fn test_client_from_file() -> anyhow::Result<()> {
    let block_time = Duration::from_millis(1100);
    let snapshot_file = NamedTempFile::new()?;

    let anvil = create_anvil(Some(block_time));

    {
        let mut last_number = 0;

        let transport_client = ReqwestTransport::new(anvil.endpoint_url());

        let rpc_client = ClientBuilder::default()
            .layer(RetryBackoffLayer::new_with_policy(
                2,
                100,
                100,
                DefaultRetryPolicy::default(),
            ))
            .layer(SnapshotRequestorLayer::new(snapshot_file.path().to_str().unwrap()))
            .transport(transport_client.clone(), transport_client.guess_local());

        let provider = ProviderBuilder::new().connect_client(rpc_client);

        for _ in 0..3 {
            sleep(block_time).await;

            let num = provider.get_block_number().await?;

            assert!(num > last_number, "next block number must be greater");
            last_number = num;
        }
    }

    {
        let transport_client = ReqwestTransport::new(anvil.endpoint_url());

        let snapshot_requestor = SnapshotRequestor::new(snapshot_file.path().to_str().unwrap())
            .load(true)
            .await;

        let rpc_client = ClientBuilder::default()
            .layer(RetryBackoffLayer::new_with_policy(
                2,
                100,
                100,
                DefaultRetryPolicy::default(),
            ))
            .layer(SnapshotRequestorLayer::from_requestor(snapshot_requestor))
            .transport(transport_client.clone(), transport_client.guess_local());

        let provider = ProviderBuilder::new().connect_client(rpc_client);

        let mut last_number = 0;
        for _ in 0..3 {
            sleep(block_time).await;

            let num = provider.get_block_number().await?;

            assert!(num > last_number, "next block number must be greater");
            last_number = num;
        }
    }

    Ok(())
}
