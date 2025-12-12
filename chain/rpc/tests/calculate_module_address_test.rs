//! Integration tests for calculate_module_address RPC operation
//!
//! These tests verify the `calculate_module_address` method works correctly
//! by interacting with a real HoprNodeStakeFactory contract deployed on Anvil.

mod common;

use std::time::Duration;

use alloy::{
    primitives::{Address as AlloyAddress, FixedBytes, U256},
    rpc::client::ClientBuilder,
    transports::{http::ReqwestTransport, layers::RetryBackoffLayer},
};
use blokli_chain_rpc::{HoprRpcOperations, rpc::RpcOperationsConfig};
use blokli_chain_types::{AlloyAddressExt, ContractAddresses, ContractInstances};
use common::{TEST_BLOCK_TIME, TEST_FINALITY, create_rpc_client_to_anvil, wait_for_finality};
use hopr_crypto_types::keypairs::{ChainKeypair, Keypair};
use hopr_primitive_types::prelude::Address;

/// Test basic functionality with valid inputs
#[tokio::test]
async fn test_calculate_module_address_basic() -> anyhow::Result<()> {
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

    // Use test addresses
    let owner: Address = (&chain_key_0).into();
    let safe_address: Address = (&ChainKeypair::from_secret(anvil.keys()[1].to_bytes().as_ref())?).into();
    let nonce = 0u64;

    // Calculate module address
    let module_address = rpc.calculate_module_address(owner, nonce, safe_address).await?;

    // Verify we got a valid address (non-zero, 20 bytes)
    assert_ne!(
        module_address,
        Address::default(),
        "Module address should not be zero address"
    );
    assert_eq!(module_address.as_ref().len(), 20, "Module address should be 20 bytes");

    Ok(())
}

/// Test that different nonces produce different addresses
#[tokio::test]
async fn test_calculate_module_address_with_different_nonces() -> anyhow::Result<()> {
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

    wait_for_finality(TEST_FINALITY, TEST_BLOCK_TIME).await;

    let rpc = blokli_chain_rpc::rpc::RpcOperations::new(rpc_client, transport_client.client().clone(), cfg, None)?;

    let owner: Address = (&chain_key_0).into();
    let safe_address: Address = (&ChainKeypair::from_secret(anvil.keys()[1].to_bytes().as_ref())?).into();

    // Calculate with different nonces
    let address_nonce_0 = rpc.calculate_module_address(owner, 0, safe_address).await?;
    let address_nonce_1 = rpc.calculate_module_address(owner, 1, safe_address).await?;
    let address_nonce_100 = rpc.calculate_module_address(owner, 100, safe_address).await?;

    // Verify all addresses are different
    assert_ne!(
        address_nonce_0, address_nonce_1,
        "Different nonces should produce different addresses"
    );
    assert_ne!(
        address_nonce_0, address_nonce_100,
        "Different nonces should produce different addresses"
    );
    assert_ne!(
        address_nonce_1, address_nonce_100,
        "Different nonces should produce different addresses"
    );

    Ok(())
}

/// Test that different owners produce different addresses
#[tokio::test]
async fn test_calculate_module_address_with_different_owners() -> anyhow::Result<()> {
    let _ = env_logger::builder().is_test(true).try_init();

    let anvil = blokli_chain_types::utils::create_anvil(Some(TEST_BLOCK_TIME));
    let chain_key_0 = ChainKeypair::from_secret(anvil.keys()[0].to_bytes().as_ref())?;

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

    wait_for_finality(TEST_FINALITY, TEST_BLOCK_TIME).await;

    let rpc = blokli_chain_rpc::rpc::RpcOperations::new(rpc_client, transport_client.client().clone(), cfg, None)?;

    // Use different owner addresses
    let owner_0: Address = (&ChainKeypair::from_secret(anvil.keys()[0].to_bytes().as_ref())?).into();
    let owner_1: Address = (&ChainKeypair::from_secret(anvil.keys()[1].to_bytes().as_ref())?).into();
    let owner_2: Address = (&ChainKeypair::from_secret(anvil.keys()[2].to_bytes().as_ref())?).into();

    let safe_address: Address = (&ChainKeypair::from_secret(anvil.keys()[3].to_bytes().as_ref())?).into();
    let nonce = 0u64;

    // Calculate with different owners
    let address_owner_0 = rpc.calculate_module_address(owner_0, nonce, safe_address).await?;
    let address_owner_1 = rpc.calculate_module_address(owner_1, nonce, safe_address).await?;
    let address_owner_2 = rpc.calculate_module_address(owner_2, nonce, safe_address).await?;

    // Verify all addresses are different
    assert_ne!(
        address_owner_0, address_owner_1,
        "Different owners should produce different addresses"
    );
    assert_ne!(
        address_owner_0, address_owner_2,
        "Different owners should produce different addresses"
    );
    assert_ne!(
        address_owner_1, address_owner_2,
        "Different owners should produce different addresses"
    );

    Ok(())
}

/// Test that different safe addresses produce different module addresses
#[tokio::test]
async fn test_calculate_module_address_with_different_safes() -> anyhow::Result<()> {
    let _ = env_logger::builder().is_test(true).try_init();

    let anvil = blokli_chain_types::utils::create_anvil(Some(TEST_BLOCK_TIME));
    let chain_key_0 = ChainKeypair::from_secret(anvil.keys()[0].to_bytes().as_ref())?;

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

    wait_for_finality(TEST_FINALITY, TEST_BLOCK_TIME).await;

    let rpc = blokli_chain_rpc::rpc::RpcOperations::new(rpc_client, transport_client.client().clone(), cfg, None)?;

    let owner: Address = (&chain_key_0).into();
    let nonce = 0u64;

    // Use different safe addresses
    let safe_0: Address = (&ChainKeypair::from_secret(anvil.keys()[1].to_bytes().as_ref())?).into();
    let safe_1: Address = (&ChainKeypair::from_secret(anvil.keys()[2].to_bytes().as_ref())?).into();
    let safe_2: Address = (&ChainKeypair::from_secret(anvil.keys()[3].to_bytes().as_ref())?).into();

    // Calculate with different safe addresses
    let address_safe_0 = rpc.calculate_module_address(owner, nonce, safe_0).await?;
    let address_safe_1 = rpc.calculate_module_address(owner, nonce, safe_1).await?;
    let address_safe_2 = rpc.calculate_module_address(owner, nonce, safe_2).await?;

    // Verify all addresses are different
    assert_ne!(
        address_safe_0, address_safe_1,
        "Different safe addresses should produce different module addresses"
    );
    assert_ne!(
        address_safe_0, address_safe_2,
        "Different safe addresses should produce different module addresses"
    );
    assert_ne!(
        address_safe_1, address_safe_2,
        "Different safe addresses should produce different module addresses"
    );

    Ok(())
}

/// Test that calculation is deterministic (same inputs always produce same output)
#[tokio::test]
async fn test_calculate_module_address_deterministic() -> anyhow::Result<()> {
    let _ = env_logger::builder().is_test(true).try_init();

    let anvil = blokli_chain_types::utils::create_anvil(Some(TEST_BLOCK_TIME));
    let chain_key_0 = ChainKeypair::from_secret(anvil.keys()[0].to_bytes().as_ref())?;

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

    wait_for_finality(TEST_FINALITY, TEST_BLOCK_TIME).await;

    let rpc = blokli_chain_rpc::rpc::RpcOperations::new(rpc_client, transport_client.client().clone(), cfg, None)?;

    let owner: Address = (&chain_key_0).into();
    let safe_address: Address = (&ChainKeypair::from_secret(anvil.keys()[1].to_bytes().as_ref())?).into();
    let nonce = 42u64;

    // Call function 3 times with identical parameters
    let address_1 = rpc.calculate_module_address(owner, nonce, safe_address).await?;
    let address_2 = rpc.calculate_module_address(owner, nonce, safe_address).await?;
    let address_3 = rpc.calculate_module_address(owner, nonce, safe_address).await?;

    // Verify all three calls return the exact same address
    assert_eq!(
        address_1, address_2,
        "Same inputs should produce same output (call 1 vs call 2)"
    );
    assert_eq!(
        address_1, address_3,
        "Same inputs should produce same output (call 1 vs call 3)"
    );
    assert_eq!(
        address_2, address_3,
        "Same inputs should produce same output (call 2 vs call 3)"
    );

    Ok(())
}

/// Test that RPC method matches direct contract call
#[tokio::test]
async fn test_calculate_module_address_matches_contract_expectation() -> anyhow::Result<()> {
    let _ = env_logger::builder().is_test(true).try_init();

    let anvil = blokli_chain_types::utils::create_anvil(Some(TEST_BLOCK_TIME));
    let chain_key_0 = ChainKeypair::from_secret(anvil.keys()[0].to_bytes().as_ref())?;

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

    wait_for_finality(TEST_FINALITY, TEST_BLOCK_TIME).await;

    // Store channels address before moving cfg
    let channels_address = cfg.contract_addrs.channels;

    let rpc = blokli_chain_rpc::rpc::RpcOperations::new(rpc_client, transport_client.client().clone(), cfg, None)?;

    let owner: Address = (&chain_key_0).into();
    let safe_address: Address = (&ChainKeypair::from_secret(anvil.keys()[1].to_bytes().as_ref())?).into();
    let nonce = 7u64;

    // Call via RPC operations method
    let address_via_rpc = rpc.calculate_module_address(owner, nonce, safe_address).await?;

    // Call directly via contract (replicates what calculate_module_address does internally)
    let channels_addr_bytes = channels_address.as_ref();
    let capability_permissions: [u8; 12] = [0x01, 0x01, 0x03, 0x03, 0x03, 0x03, 0x03, 0x03, 0x03, 0x03, 0x03, 0x03];
    let mut default_target = [0u8; 32];
    default_target[0..20].copy_from_slice(channels_addr_bytes);
    default_target[20..32].copy_from_slice(&capability_permissions);

    let address_via_contract = contract_instances
        .node_stake_v2_factory
        .predictModuleAddress_1(
            AlloyAddress::from_hopr_address(owner),
            U256::from(nonce),
            AlloyAddress::from_hopr_address(safe_address),
            FixedBytes::from(default_target),
        )
        .call()
        .await?
        .0;

    let address_via_contract_hopr = AlloyAddress::from(address_via_contract).to_hopr_address();

    // Verify both methods return the same address
    assert_eq!(
        address_via_rpc, address_via_contract_hopr,
        "RPC method should return same address as direct contract call"
    );

    Ok(())
}
