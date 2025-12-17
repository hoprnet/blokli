//! Integration tests for contract verification system
//!
//! These tests verify the ContractVerifier functionality using real contract
//! deployments on Anvil local testnet.

mod common;

use std::sync::Arc;

use blokli_chain_rpc::{
    rpc::{RpcOperations, RpcOperationsConfig},
    verification::ContractVerifier,
};
use blokli_chain_types::{ContractAddresses, ContractInstances, utils::create_anvil};
use common::{
    TEST_BLOCK_TIME, TEST_FINALITY, TEST_TX_POLLING_INTERVAL, create_rpc_client_to_anvil, create_test_rpc_client,
    wait_for_finality,
};
use hopr_crypto_types::keypairs::{ChainKeypair, Keypair};
use hopr_primitive_types::primitives::Address;

/// Helper to create RpcOperations for testing
fn create_test_rpc_operations(
    rpc_client: alloy::rpc::client::RpcClient,
    chain_id: u64,
    contract_addrs: ContractAddresses,
) -> anyhow::Result<RpcOperations<blokli_chain_rpc::ReqwestClient>> {
    let requestor = blokli_chain_rpc::ReqwestClient::new();

    let rpc_cfg = RpcOperationsConfig {
        chain_id,
        contract_addrs,
        tx_polling_interval: TEST_TX_POLLING_INTERVAL,
        finality: TEST_FINALITY,
        max_block_range_fetch_size: 100,
        ..Default::default()
    };

    Ok(RpcOperations::new(rpc_client, requestor, rpc_cfg, None)?)
}

// Note: Exact bytecode verification of locally deployed contracts is not feasible
// because constructor arguments become immutable variables encoded in the bytecode,
// making each deployment unique. This test is commented out but kept for reference.
//
// In production, exact bytecode verification works because:
// 1. Contracts are deployed with known constructor arguments
// 2. DEPLOYED_BYTECODE in hopr-bindings matches the production deployment
//
// #[tokio::test]
// async fn test_verify_deployed_contracts_success() -> anyhow::Result<()> {
//     // This test cannot pass because locally deployed contracts have different
//     // constructor arguments than the DEPLOYED_BYTECODE constant expects
// }

#[tokio::test]
async fn test_verify_no_contract_at_address() -> anyhow::Result<()> {
    let _ = env_logger::builder().is_test(true).try_init();

    // Setup Anvil
    let anvil = create_anvil(Some(TEST_BLOCK_TIME));
    let chain_key_0 = ChainKeypair::from_secret(anvil.keys()[0].to_bytes().as_ref())?;

    // Deploy real contracts
    let client = create_rpc_client_to_anvil(&anvil, &chain_key_0);
    let contract_instances = ContractInstances::deploy_for_testing(client, &chain_key_0).await?;
    let mut contract_addrs = ContractAddresses::from(&contract_instances);

    // Replace token address with a random address (no code deployed)
    let random_address = Address::new(&[0xff; 20]);
    contract_addrs.token = random_address;

    // Wait for finality
    wait_for_finality(TEST_FINALITY, TEST_BLOCK_TIME).await;

    // Setup RpcOperations
    let rpc_client = create_test_rpc_client(&anvil);
    let rpc_ops = create_test_rpc_operations(rpc_client, anvil.chain_id(), contract_addrs)?;

    // Create verifier and attempt verification
    let verifier = ContractVerifier::new(Arc::new(rpc_ops));
    let result = verifier.verify_all_contracts(&contract_addrs).await;

    // Assertions
    assert!(result.is_err(), "Should fail when no contract at address");
    let error = result.unwrap_err();
    let error_msg = format!("{}", error);
    assert!(
        error_msg.contains("No contract code deployed"),
        "Error should indicate no code deployed: {}",
        error_msg
    );
    assert!(
        error_msg.contains("HoprToken"),
        "Error should mention HoprToken contract: {}",
        error_msg
    );

    Ok(())
}

#[tokio::test]
async fn test_verify_wrong_contract_deployed() -> anyhow::Result<()> {
    let _ = env_logger::builder().is_test(true).try_init();

    // Setup Anvil
    let anvil = create_anvil(Some(TEST_BLOCK_TIME));
    let chain_key_0 = ChainKeypair::from_secret(anvil.keys()[0].to_bytes().as_ref())?;

    // Deploy real contracts
    let client = create_rpc_client_to_anvil(&anvil, &chain_key_0);
    let contract_instances = ContractInstances::deploy_for_testing(client, &chain_key_0).await?;
    let mut contract_addrs = ContractAddresses::from(&contract_instances);

    // Swap addresses: put token address where channels should be
    contract_addrs.channels = contract_addrs.token;

    // Wait for finality
    wait_for_finality(TEST_FINALITY, TEST_BLOCK_TIME).await;

    // Setup RpcOperations
    let rpc_client = create_test_rpc_client(&anvil);
    let rpc_ops = create_test_rpc_operations(rpc_client, anvil.chain_id(), contract_addrs)?;

    // Create verifier and attempt verification
    let verifier = ContractVerifier::new(Arc::new(rpc_ops));
    let result = verifier.verify_all_contracts(&contract_addrs).await;

    // Assertions
    assert!(result.is_err(), "Should fail when wrong contract deployed");
    let error = result.unwrap_err();
    let error_msg = format!("{}", error);
    assert!(
        error_msg.contains("Bytecode mismatch"),
        "Error should indicate bytecode mismatch: {}",
        error_msg
    );
    assert!(
        error_msg.contains("HoprChannels"),
        "Error should mention HoprChannels contract: {}",
        error_msg
    );
    assert!(
        error_msg.contains("expected") && error_msg.contains("bytes"),
        "Error should show expected bytecode length: {}",
        error_msg
    );

    Ok(())
}

#[tokio::test]
async fn test_verify_stops_on_first_failure() -> anyhow::Result<()> {
    let _ = env_logger::builder().is_test(true).try_init();

    // Setup Anvil
    let anvil = create_anvil(Some(TEST_BLOCK_TIME));
    let chain_key_0 = ChainKeypair::from_secret(anvil.keys()[0].to_bytes().as_ref())?;

    // Deploy real contracts
    let client = create_rpc_client_to_anvil(&anvil, &chain_key_0);
    let contract_instances = ContractInstances::deploy_for_testing(client, &chain_key_0).await?;
    let mut contract_addrs = ContractAddresses::from(&contract_instances);

    // Replace the 1st contract address (token) with random address to avoid bytecode mismatch on deployed contracts
    contract_addrs.token = Address::new(&[0xaa; 20]);

    // Wait for finality
    wait_for_finality(TEST_FINALITY, TEST_BLOCK_TIME).await;

    // Setup RpcOperations
    let rpc_client = create_test_rpc_client(&anvil);
    let rpc_ops = create_test_rpc_operations(rpc_client, anvil.chain_id(), contract_addrs)?;

    // Create verifier and attempt verification
    let verifier = ContractVerifier::new(Arc::new(rpc_ops));
    let result = verifier.verify_all_contracts(&contract_addrs).await;

    // Assertions
    assert!(result.is_err(), "Should fail when 1st contract has no code");
    let error = result.unwrap_err();
    let error_msg = format!("{}", error);
    assert!(
        error_msg.contains("HoprToken"),
        "Error should be for 1st contract (HoprToken): {}",
        error_msg
    );
    assert!(
        error_msg.contains("No contract code deployed"),
        "Error should indicate no code deployed: {}",
        error_msg
    );

    Ok(())
}
