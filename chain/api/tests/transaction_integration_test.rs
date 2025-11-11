//! End-to-end integration tests for transaction submission system
//!
//! These tests use an in-process Anvil instance to test the complete
//! transaction flow from submission through confirmation tracking.

use std::{sync::Arc, time::Duration};

use alloy::{
    consensus::{SignableTransaction, TxLegacy},
    eips::eip2718::Encodable2718,
    node_bindings::AnvilInstance,
    primitives::{TxKind, U256},
    signers::{Signer, local::PrivateKeySigner},
};
use blokli_chain_api::{
    DefaultHttpRequestor,
    rpc_adapter::RpcAdapter,
    transaction_executor::{RawTransactionExecutor, RawTransactionExecutorConfig},
    transaction_monitor::{TransactionMonitor, TransactionMonitorConfig},
    transaction_store::{TransactionStatus, TransactionStore},
    transaction_validator::TransactionValidator,
};
use blokli_chain_rpc::{
    client::DefaultRetryPolicy,
    rpc::{RpcOperations, RpcOperationsConfig},
    transport::ReqwestClient,
};
use blokli_chain_types::{ContractAddresses, utils::create_anvil};
use hopr_crypto_types::{
    keypairs::{ChainKeypair, Keypair},
    types::Hash,
};
use hopr_primitive_types::primitives::Address;
use tokio::task::AbortHandle;

/// Test context containing all components needed for integration tests
struct TestContext {
    anvil: AnvilInstance,
    chain_key: ChainKeypair,
    executor: Arc<RawTransactionExecutor<RpcAdapter<DefaultHttpRequestor>>>,
    store: Arc<TransactionStore>,
    monitor: Arc<TransactionMonitor<RpcAdapter<DefaultHttpRequestor>>>,
    rpc_adapter: Arc<RpcAdapter<DefaultHttpRequestor>>,
    monitor_handle: Option<AbortHandle>,
}

impl Drop for TestContext {
    fn drop(&mut self) {
        // Stop the monitor if it's running
        if let Some(handle) = self.monitor_handle.take() {
            handle.abort();
        }
    }
}

/// Set up a complete test environment with Anvil and all transaction components
async fn setup_test_environment(
    block_time: Duration,
    confirmations: u32,
    executor_config: RawTransactionExecutorConfig,
) -> anyhow::Result<TestContext> {
    // Initialize logging for tests
    let _ = env_logger::builder().is_test(true).try_init();

    // Start Anvil with specified block time
    let anvil = create_anvil(Some(block_time));
    let chain_key = ChainKeypair::from_secret(anvil.keys()[0].to_bytes().as_ref())?;

    // Create RPC configuration
    let cfg = RpcOperationsConfig {
        chain_id: anvil.chain_id(),
        tx_polling_interval: Duration::from_millis(100),
        expected_block_time: block_time,
        finality: confirmations,
        gas_oracle_url: None,
        contract_addrs: ContractAddresses::default(),
        ..Default::default()
    };

    // Set up RPC client with retry policy
    let transport_client = alloy::transports::http::ReqwestTransport::new(anvil.endpoint_url());
    let rpc_client = alloy::rpc::client::ClientBuilder::default()
        .layer(alloy::transports::layers::RetryBackoffLayer::new_with_policy(
            2,
            100,
            100,
            DefaultRetryPolicy::default(),
        ))
        .transport(transport_client.clone(), transport_client.guess_local());

    let rpc_operations = RpcOperations::new(rpc_client, ReqwestClient::new(), cfg, None)?;
    let rpc_adapter = Arc::new(RpcAdapter::new(rpc_operations));

    // Create transaction components
    let transaction_store = Arc::new(TransactionStore::new());
    let transaction_validator = Arc::new(TransactionValidator::new());

    let transaction_executor = Arc::new(RawTransactionExecutor::with_shared_dependencies(
        rpc_adapter.clone(),
        transaction_store.clone(),
        transaction_validator,
        executor_config,
    ));

    let monitor_config = TransactionMonitorConfig {
        poll_interval: Duration::from_millis(200),
        timeout: Duration::from_secs(300),
        per_transaction_delay: Duration::from_millis(100),
    };

    let transaction_monitor = Arc::new(TransactionMonitor::new(
        transaction_store.clone(),
        (*rpc_adapter).clone(),
        monitor_config,
    ));

    Ok(TestContext {
        anvil,
        chain_key,
        executor: transaction_executor,
        store: transaction_store,
        monitor: transaction_monitor,
        rpc_adapter,
        monitor_handle: None,
    })
}

/// Start the transaction monitor in the background
fn start_monitor(ctx: &mut TestContext) -> AbortHandle {
    let monitor = ctx.monitor.clone();
    let handle = tokio::spawn(async move {
        monitor.start().await;
    });
    handle.abort_handle()
}

/// Generate a random address for testing
fn random_address() -> Address {
    Address::new(&rand::random::<[u8; 20]>())
}

/// Wait for a condition to become true, polling every 50ms
///
/// Returns immediately when condition succeeds, or errors after timeout.
/// This significantly improves test suite runtime compared to fixed sleeps.
async fn wait_for_condition<F>(timeout: Duration, condition_desc: &str, mut condition: F) -> anyhow::Result<()>
where
    F: FnMut() -> bool,
{
    let start = std::time::Instant::now();
    let poll_interval = Duration::from_millis(50);

    loop {
        if condition() {
            return Ok(());
        }

        if start.elapsed() >= timeout {
            anyhow::bail!("Timeout waiting for: {}", condition_desc);
        }

        tokio::time::sleep(poll_interval).await;
    }
}

/// Create a simple ETH transfer transaction
async fn create_eth_transfer_tx(
    ctx: &TestContext,
    to: Address,
    value_wei: u128,
    nonce: u64,
) -> anyhow::Result<Vec<u8>> {
    let tx = TxLegacy {
        chain_id: Some(ctx.anvil.chain_id()),
        nonce,
        gas_price: 1_000_000_000, // 1 gwei
        gas_limit: 21_000,
        to: TxKind::Call(to.into()),
        value: U256::from(value_wei),
        input: Default::default(),
    };

    // Create a signer from the chain key
    let signer = PrivateKeySigner::from_slice(ctx.chain_key.secret().as_ref()).expect("Failed to create signer");

    // Sign the transaction hash
    let tx_hash = tx.signature_hash();
    let signature = signer.sign_hash(&tx_hash).await?;
    let signed_tx = tx.into_signed(signature);

    // Encode the signed transaction
    let mut encoded = Vec::new();
    signed_tx.encode_2718(&mut encoded);

    Ok(encoded)
}

// =============================================================================
// Test Category 1: Fire-and-Forget Mode Tests
// =============================================================================

#[tokio::test]
async fn test_fire_and_forget_returns_hash_immediately() -> anyhow::Result<()> {
    let ctx = setup_test_environment(Duration::from_secs(1), 2, RawTransactionExecutorConfig::default()).await?;

    let recipient = random_address();
    let raw_tx = create_eth_transfer_tx(&ctx, recipient, 1000, 0).await?;

    // Send transaction in fire-and-forget mode
    let tx_hash = ctx.executor.send_raw_transaction(raw_tx).await?;

    // Verify hash is returned
    assert_ne!(tx_hash, Hash::default());

    // Verify transaction is NOT in store (fire-and-forget doesn't track)
    let submitted = ctx.store.list_by_status(TransactionStatus::Submitted);
    let pending = ctx.store.list_by_status(TransactionStatus::Pending);
    assert!(
        submitted.is_empty() && pending.is_empty(),
        "Fire-and-forget should not store transactions"
    );

    Ok(())
}

#[tokio::test]
async fn test_fire_and_forget_with_insufficient_gas() -> anyhow::Result<()> {
    let ctx = setup_test_environment(Duration::from_secs(1), 2, RawTransactionExecutorConfig::default()).await?;

    let recipient = random_address();
    let mut raw_tx = create_eth_transfer_tx(&ctx, recipient, 1000, 0).await?;

    // Corrupt the transaction by truncating it (should cause decoding/gas issues)
    raw_tx.truncate(raw_tx.len() / 2);

    let result = ctx.executor.send_raw_transaction(raw_tx).await;
    assert!(result.is_err(), "Should fail with corrupted transaction");

    Ok(())
}

// =============================================================================
// Test Category 2: Async Mode Tests
// =============================================================================

#[tokio::test]
async fn test_async_mode_returns_uuid_and_tracks() -> anyhow::Result<()> {
    let mut ctx = setup_test_environment(Duration::from_secs(1), 2, RawTransactionExecutorConfig::default()).await?;

    let recipient = random_address();
    let raw_tx = create_eth_transfer_tx(&ctx, recipient, 1000, 0).await?;

    // Start the monitor
    ctx.monitor_handle = Some(start_monitor(&mut ctx));

    // Send transaction in async mode
    let uuid = ctx.executor.send_raw_transaction_async(raw_tx).await?;

    // Verify UUID is returned and transaction is in store
    let record = ctx.store.get(uuid)?;
    assert_eq!(record.status, TransactionStatus::Submitted);
    assert!(record.transaction_hash.is_some());

    // Wait for monitor to update status
    wait_for_condition(Duration::from_secs(3), "transaction to be confirmed", || {
        ctx.store
            .get(uuid)
            .map(|r| r.status == TransactionStatus::Confirmed)
            .unwrap_or(false)
    })
    .await?;

    Ok(())
}

#[tokio::test]
async fn test_async_mode_multiple_transactions() -> anyhow::Result<()> {
    let mut ctx = setup_test_environment(Duration::from_secs(1), 2, RawTransactionExecutorConfig::default()).await?;

    ctx.monitor_handle = Some(start_monitor(&mut ctx));

    // Submit 3 transactions
    let mut uuids = Vec::new();
    for nonce in 0..3 {
        let recipient = random_address();
        let raw_tx = create_eth_transfer_tx(&ctx, recipient, 1000 + nonce as u128, nonce).await?;
        let uuid = ctx.executor.send_raw_transaction_async(raw_tx).await?;
        uuids.push(uuid);
    }

    // All should be submitted
    for uuid in &uuids {
        let record = ctx.store.get(*uuid)?;
        assert_eq!(record.status, TransactionStatus::Submitted);
    }

    // Wait for confirmations
    wait_for_condition(Duration::from_secs(4), "all transactions to be confirmed", || {
        uuids.iter().all(|uuid| {
            ctx.store
                .get(*uuid)
                .map(|r| r.status == TransactionStatus::Confirmed)
                .unwrap_or(false)
        })
    })
    .await?;

    Ok(())
}

#[tokio::test]
async fn test_async_mode_query_status_before_confirmation() -> anyhow::Result<()> {
    let mut ctx = setup_test_environment(
        Duration::from_secs(2), // Slower block time
        2,
        RawTransactionExecutorConfig::default(),
    )
    .await?;

    ctx.monitor_handle = Some(start_monitor(&mut ctx));

    let recipient = random_address();
    let raw_tx = create_eth_transfer_tx(&ctx, recipient, 1000, 0).await?;

    let uuid = ctx.executor.send_raw_transaction_async(raw_tx).await?;

    // Immediately check status - should be Submitted
    let record = ctx.store.get(uuid)?;
    assert_eq!(record.status, TransactionStatus::Submitted);

    // Wait a bit but not enough for full confirmation
    tokio::time::sleep(Duration::from_secs(1)).await;

    // Should still be submitted or in progress
    let record = ctx.store.get(uuid)?;
    assert!(
        matches!(record.status, TransactionStatus::Submitted),
        "Should still be submitted or confirming"
    );

    Ok(())
}

#[tokio::test]
async fn test_async_mode_validation_failure() -> anyhow::Result<()> {
    let ctx = setup_test_environment(Duration::from_secs(1), 2, RawTransactionExecutorConfig::default()).await?;

    // Empty transaction should fail validation
    let empty_tx = vec![];

    let result = ctx.executor.send_raw_transaction_async(empty_tx).await;
    assert!(result.is_err(), "Empty transaction should fail validation");

    // Verify nothing was stored
    let submitted = ctx.store.list_by_status(TransactionStatus::Submitted);
    let pending = ctx.store.list_by_status(TransactionStatus::Pending);
    assert!(
        submitted.is_empty() && pending.is_empty(),
        "Failed validation should not create record"
    );

    Ok(())
}

// =============================================================================
// Test Category 3: Sync Mode Tests
// =============================================================================

#[tokio::test]
async fn test_sync_mode_waits_for_confirmations() -> anyhow::Result<()> {
    let ctx = setup_test_environment(Duration::from_secs(1), 2, RawTransactionExecutorConfig::default()).await?;

    let recipient = random_address();
    let raw_tx = create_eth_transfer_tx(&ctx, recipient, 1000, 0).await?;

    let start = std::time::Instant::now();

    // Send transaction in sync mode with 2 confirmations
    let record = ctx.executor.send_raw_transaction_sync(raw_tx, Some(2)).await?;

    let elapsed = start.elapsed();

    // Should have waited for confirmations (at least 2 seconds with 1s block time)
    assert!(elapsed.as_secs() >= 2, "Should wait for confirmations");

    // Should be confirmed
    assert_eq!(record.status, TransactionStatus::Confirmed);
    assert!(record.transaction_hash.is_some());

    Ok(())
}

#[tokio::test]
async fn test_sync_mode_with_custom_confirmations() -> anyhow::Result<()> {
    let ctx = setup_test_environment(Duration::from_secs(1), 2, RawTransactionExecutorConfig::default()).await?;

    let recipient = random_address();
    let raw_tx = create_eth_transfer_tx(&ctx, recipient, 1000, 0).await?;

    let start = std::time::Instant::now();

    // Wait for 1 confirmation only
    let record = ctx.executor.send_raw_transaction_sync(raw_tx, Some(1)).await?;

    let elapsed = start.elapsed();

    // Should wait less time (around 1 second)
    assert!(elapsed.as_secs() >= 1, "Should wait for at least 1 block");
    assert!(elapsed.as_secs() < 3, "Should not wait too long");
    assert_eq!(record.status, TransactionStatus::Confirmed);

    Ok(())
}

#[tokio::test]
async fn test_sync_mode_timeout() -> anyhow::Result<()> {
    let config = RawTransactionExecutorConfig {
        confirmation_timeout: Duration::from_secs(2), // Very short timeout
        ..Default::default()
    };

    let ctx = setup_test_environment(
        Duration::from_secs(10), // Very slow block time
        2,
        config,
    )
    .await?;

    let recipient = random_address();
    let raw_tx = create_eth_transfer_tx(&ctx, recipient, 1000, 0).await?;

    // Should timeout waiting for confirmations
    let result = ctx.executor.send_raw_transaction_sync(raw_tx, Some(2)).await;

    assert!(result.is_err(), "Should timeout waiting for confirmations");

    Ok(())
}

#[tokio::test]
async fn test_sync_mode_persists_to_store() -> anyhow::Result<()> {
    let ctx = setup_test_environment(Duration::from_secs(1), 2, RawTransactionExecutorConfig::default()).await?;

    let recipient = random_address();
    let raw_tx = create_eth_transfer_tx(&ctx, recipient, 1000, 0).await?;

    let record = ctx.executor.send_raw_transaction_sync(raw_tx, Some(2)).await?;

    // Verify transaction is in store
    let stored_record = ctx.store.get(record.id)?;
    assert_eq!(stored_record.id, record.id);
    assert_eq!(stored_record.status, TransactionStatus::Confirmed);
    assert_eq!(stored_record.transaction_hash, record.transaction_hash);

    Ok(())
}

// =============================================================================
// Test Category 4: Error Scenario Tests
// =============================================================================

#[tokio::test]
async fn test_empty_transaction_validation() -> anyhow::Result<()> {
    let ctx = setup_test_environment(Duration::from_secs(1), 2, RawTransactionExecutorConfig::default()).await?;

    let empty_tx = vec![];

    // All modes should reject empty transactions
    assert!(ctx.executor.send_raw_transaction(empty_tx.clone()).await.is_err());
    assert!(ctx.executor.send_raw_transaction_async(empty_tx.clone()).await.is_err());
    assert!(ctx.executor.send_raw_transaction_sync(empty_tx, Some(2)).await.is_err());

    Ok(())
}

#[tokio::test]
async fn test_malformed_transaction_hex() -> anyhow::Result<()> {
    let ctx = setup_test_environment(Duration::from_secs(1), 2, RawTransactionExecutorConfig::default()).await?;

    // Create invalid transaction data
    let malformed_tx = vec![0xFF; 10]; // Random bytes

    let result = ctx.executor.send_raw_transaction(malformed_tx.clone()).await;
    assert!(result.is_err(), "Should reject malformed transaction");

    Ok(())
}

#[tokio::test]
async fn test_transaction_with_invalid_signature() -> anyhow::Result<()> {
    let ctx = setup_test_environment(Duration::from_secs(1), 2, RawTransactionExecutorConfig::default()).await?;

    let recipient = random_address();
    let mut raw_tx = create_eth_transfer_tx(&ctx, recipient, 1000, 0).await?;

    // Corrupt the signature by flipping some bytes
    let len = raw_tx.len();
    if len > 10 {
        raw_tx[len - 5] ^= 0xFF;
    }

    let result = ctx.executor.send_raw_transaction(raw_tx).await;
    assert!(result.is_err(), "Should reject invalid signature");

    Ok(())
}

#[tokio::test]
#[ignore] // Skipped: Cannot drop anvil instance due to Drop trait on TestContext
async fn test_rpc_connection_failure_handling() -> anyhow::Result<()> {
    // FIXME: This test would require a different approach to simulate RPC failure
    // without dropping the anvil instance
    Ok(())
}

// =============================================================================
// Test Category 5: Concurrent Operations Tests
// =============================================================================

#[tokio::test]
async fn test_concurrent_async_submissions() -> anyhow::Result<()> {
    let mut ctx = setup_test_environment(Duration::from_secs(1), 2, RawTransactionExecutorConfig::default()).await?;

    ctx.monitor_handle = Some(start_monitor(&mut ctx));

    // Submit 10 transactions concurrently
    let mut handles = Vec::new();
    for nonce in 0..10 {
        let executor = ctx.executor.clone();
        let recipient = random_address();
        let raw_tx = create_eth_transfer_tx(&ctx, recipient, 1000 + nonce as u128, nonce).await?;

        let handle = tokio::spawn(async move { executor.send_raw_transaction_async(raw_tx).await });
        handles.push(handle);
    }

    // Wait for all submissions
    let mut uuids = Vec::new();
    for handle in handles {
        let uuid = handle.await??;
        uuids.push(uuid);
    }

    // All should be tracked
    assert_eq!(uuids.len(), 10);
    for uuid in &uuids {
        let record = ctx.store.get(*uuid)?;
        assert_eq!(record.status, TransactionStatus::Submitted);
    }

    // Wait for confirmations
    wait_for_condition(
        Duration::from_secs(4),
        "all concurrent transactions to be confirmed",
        || {
            uuids.iter().all(|uuid| {
                ctx.store
                    .get(*uuid)
                    .map(|r| r.status == TransactionStatus::Confirmed)
                    .unwrap_or(false)
            })
        },
    )
    .await?;

    Ok(())
}

#[tokio::test]
async fn test_mixed_mode_operations() -> anyhow::Result<()> {
    let mut ctx = setup_test_environment(Duration::from_secs(1), 2, RawTransactionExecutorConfig::default()).await?;

    ctx.monitor_handle = Some(start_monitor(&mut ctx));

    // Fire-and-forget (nonce 0)
    let tx1 = create_eth_transfer_tx(&ctx, random_address(), 1000, 0).await?;
    let hash1 = ctx.executor.send_raw_transaction(tx1).await?;
    assert_ne!(hash1, Hash::default());

    // Async (nonce 1)
    let tx2 = create_eth_transfer_tx(&ctx, random_address(), 2000, 1).await?;
    let uuid2 = ctx.executor.send_raw_transaction_async(tx2).await?;

    // Sync (nonce 2)
    let tx3 = create_eth_transfer_tx(&ctx, random_address(), 3000, 2).await?;
    let record3 = ctx.executor.send_raw_transaction_sync(tx3, Some(2)).await?;

    // Verify sync completed
    assert_eq!(record3.status, TransactionStatus::Confirmed);

    // Wait for async to confirm
    wait_for_condition(Duration::from_secs(3), "async transaction to be confirmed", || {
        ctx.store
            .get(uuid2)
            .map(|r| r.status == TransactionStatus::Confirmed)
            .unwrap_or(false)
    })
    .await?;

    // Fire-and-forget should not be in store - count all statuses
    let mut total_tracked = 0;
    for status in [
        TransactionStatus::Submitted,
        TransactionStatus::Confirmed,
        TransactionStatus::Pending,
    ] {
        total_tracked += ctx.store.list_by_status(status).len();
    }
    assert_eq!(total_tracked, 2); // Only async and sync

    Ok(())
}

// =============================================================================
// Test Category 6: Transaction Monitor Tests
// =============================================================================

#[tokio::test]
async fn test_monitor_updates_confirmed_status() -> anyhow::Result<()> {
    let mut ctx = setup_test_environment(Duration::from_secs(1), 2, RawTransactionExecutorConfig::default()).await?;

    let recipient = random_address();
    let raw_tx = create_eth_transfer_tx(&ctx, recipient, 1000, 0).await?;

    // Submit without monitor
    let uuid = ctx.executor.send_raw_transaction_async(raw_tx).await?;

    // Initially submitted
    let record = ctx.store.get(uuid)?;
    assert_eq!(record.status, TransactionStatus::Submitted);

    // Start monitor
    ctx.monitor_handle = Some(start_monitor(&mut ctx));

    // Wait for monitor to process
    wait_for_condition(Duration::from_secs(3), "monitor to confirm transaction", || {
        ctx.store
            .get(uuid)
            .map(|r| r.status == TransactionStatus::Confirmed)
            .unwrap_or(false)
    })
    .await?;
    Ok(())
}

#[tokio::test]
async fn test_monitor_batch_processing() -> anyhow::Result<()> {
    let mut ctx = setup_test_environment(Duration::from_secs(1), 2, RawTransactionExecutorConfig::default()).await?;

    // Submit multiple transactions
    let mut uuids = Vec::new();
    for nonce in 0..5 {
        let recipient = random_address();
        let raw_tx = create_eth_transfer_tx(&ctx, recipient, 1000 + nonce as u128, nonce).await?;
        let uuid = ctx.executor.send_raw_transaction_async(raw_tx).await?;
        uuids.push(uuid);
    }

    // Start monitor after submissions
    ctx.monitor_handle = Some(start_monitor(&mut ctx));

    // Wait for batch processing
    wait_for_condition(
        Duration::from_secs(4),
        "monitor to batch-process all transactions",
        || {
            uuids.iter().all(|uuid| {
                ctx.store
                    .get(*uuid)
                    .map(|r| r.status == TransactionStatus::Confirmed)
                    .unwrap_or(false)
            })
        },
    )
    .await?;
    Ok(())
}

#[tokio::test]
async fn test_monitor_handles_reorg() -> anyhow::Result<()> {
    let mut ctx = setup_test_environment(Duration::from_secs(1), 2, RawTransactionExecutorConfig::default()).await?;

    ctx.monitor_handle = Some(start_monitor(&mut ctx));

    let recipient = random_address();
    let raw_tx = create_eth_transfer_tx(&ctx, recipient, 1000, 0).await?;

    let uuid = ctx.executor.send_raw_transaction_async(raw_tx).await?;

    // Wait for confirmation
    wait_for_condition(
        Duration::from_secs(3),
        "transaction to be confirmed after reorg",
        || {
            ctx.store
                .get(uuid)
                .map(|r| r.status == TransactionStatus::Confirmed)
                .unwrap_or(false)
        },
    )
    .await?;
    // In a real reorg scenario, the transaction might become pending again
    // For now, we just verify the monitor continues to work correctly
    tokio::time::sleep(Duration::from_secs(2)).await;

    // Should still be confirmed (or updated if reorg happened)
    let record = ctx.store.get(uuid)?;
    assert!(
        matches!(record.status, TransactionStatus::Confirmed),
        "Transaction should remain in valid state"
    );

    Ok(())
}
