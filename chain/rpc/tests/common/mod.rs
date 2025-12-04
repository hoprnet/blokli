//! Common test utilities for integration tests

use std::time::Duration;

use alloy::{
    node_bindings::AnvilInstance,
    providers::PendingTransaction,
    rpc::client::ClientBuilder,
    transports::{http::ReqwestTransport, layers::RetryBackoffLayer},
};
use blokli_chain_rpc::{
    rpc::{RpcOperations, RpcOperationsConfig},
    transport::HttpRequestor,
};
use tokio::time::sleep;

/// Standard block time for tests (1 second)
pub const TEST_BLOCK_TIME: Duration = Duration::from_secs(1);

/// Standard finality requirement for tests (2 blocks)
pub const TEST_FINALITY: u32 = 2;

/// Standard transaction polling interval for tests (10ms)
pub const TEST_TX_POLLING_INTERVAL: Duration = Duration::from_millis(10);

/// Wait for a transaction to be confirmed with a timeout.
///
/// This function will await the pending transaction with a deadline enforced by the timeout.
/// It will panic if the transaction is not confirmed within the timeout period.
///
/// # Arguments
///
/// * `pending` - The pending transaction to wait for
/// * `timeout` - Maximum time to wait for confirmation
///
/// # Panics
///
/// Panics if the transaction is not confirmed within the timeout period.
pub async fn wait_until_tx(pending: PendingTransaction, timeout: Duration) {
    let tx_hash = *pending.tx_hash();
    tokio::time::timeout(timeout, pending)
        .await
        .unwrap_or_else(|_| panic!("timeout awaiting tx hash {tx_hash} after {} seconds", timeout.as_secs()))
        .unwrap();
}

/// Wait for the finality period to ensure transactions are confirmed.
///
/// This function sleeps for `(1 + finality) * block_time` to ensure that transactions
/// are sufficiently confirmed before querying results.
///
/// # Arguments
///
/// * `finality` - Number of confirmation blocks required
/// * `block_time` - Expected time between blocks
pub async fn wait_for_finality(finality: u32, block_time: Duration) {
    sleep(block_time * (1 + finality)).await;
}

/// Create a standard RPC client for Anvil-based tests with retry layer.
///
/// This function creates an RPC client configured with:
/// - Retry policy: 2 retries with 100ms base and 100ms max backoff
/// - ReqwestTransport connected to the Anvil endpoint
///
/// # Arguments
///
/// * `anvil` - The Anvil instance to connect to
///
/// # Returns
///
/// Returns an RPC client configured with retry layer
pub fn create_test_rpc_client(anvil: &AnvilInstance) -> alloy::rpc::client::RpcClient {
    let transport_client = ReqwestTransport::new(anvil.endpoint_url());

    ClientBuilder::default()
        .layer(RetryBackoffLayer::new(2, 100, 100))
        .transport(transport_client.clone(), transport_client.guess_local())
}

/// Create standard RpcOperations for tests with common configuration.
///
/// This function creates an RpcOperations instance with test-friendly settings:
/// - Standard block time, finality, and tx polling interval
/// - Optional gas oracle URL
/// - Default configuration for other fields
///
/// # Arguments
///
/// * `rpc_client` - The RPC client to use
/// * `requestor` - The HTTP requestor for making requests
/// * `chain_id` - The blockchain chain ID
/// * `gas_oracle_url` - Optional gas oracle URL for EIP-1559 fee estimation
///
/// # Returns
///
/// Returns `Result<RpcOperations<R>>` with the configured RPC operations instance
pub fn create_test_rpc_operations<R: HttpRequestor + 'static + Clone>(
    rpc_client: alloy::rpc::client::RpcClient,
    requestor: R,
    chain_id: u64,
    gas_oracle_url: Option<url::Url>,
) -> anyhow::Result<RpcOperations<R>> {
    let cfg = RpcOperationsConfig {
        chain_id,
        tx_polling_interval: TEST_TX_POLLING_INTERVAL,
        expected_block_time: TEST_BLOCK_TIME,
        finality: TEST_FINALITY,
        gas_oracle_url,
        ..RpcOperationsConfig::default()
    };

    Ok(RpcOperations::new(rpc_client, requestor, cfg, None)?)
}

// Re-export for test convenience
pub use blokli_chain_rpc::client::create_rpc_client_to_anvil;
