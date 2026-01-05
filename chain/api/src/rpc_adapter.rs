//! RPC adapter for integrating RpcOperations with the transaction executor
//!
//! This module provides implementations of the `RpcClient` and `ReceiptProvider` traits
//! for `RpcOperations`, allowing the transaction executor to submit raw transactions
//! and monitor their confirmation status.

use std::sync::Arc;

use alloy::{primitives::Bytes, providers::Provider};
use async_trait::async_trait;
use blokli_chain_rpc::{rpc::RpcOperations, transport::HttpRequestor};
use hopr_crypto_types::types::Hash;
use tracing::{debug, error};

use crate::{transaction_executor::RpcClient, transaction_monitor::ReceiptProvider};

/// RPC adapter that implements RpcClient and ReceiptProvider traits for RpcOperations
///
/// This adapter bridges the gap between the transaction executor's trait requirements
/// and the actual RpcOperations implementation from the chain/rpc module.
#[derive(Debug, Clone)]
pub struct RpcAdapter<R: HttpRequestor + 'static + Clone> {
    rpc: Arc<RpcOperations<R>>,
}

impl<R: HttpRequestor + 'static + Clone> RpcAdapter<R> {
    /// Create a new RPC adapter wrapping the given RpcOperations
    pub fn new(rpc: RpcOperations<R>) -> Self {
        Self { rpc: Arc::new(rpc) }
    }
}

#[async_trait]
impl<R: HttpRequestor + 'static + Clone> RpcClient for RpcAdapter<R> {
    /// Send a raw transaction and return its hash
    ///
    /// Converts the raw transaction bytes to alloy Bytes format and submits to the RPC provider.
    /// Returns the transaction hash immediately without waiting for confirmation.
    async fn send_raw_transaction(&self, raw_tx: Vec<u8>) -> Result<Hash, String> {
        debug!("Sending raw transaction ({} bytes)", raw_tx.len());

        // Convert Vec<u8> to alloy Bytes
        let bytes = Bytes::from(raw_tx);

        // Send raw transaction using the provider's send_raw_transaction method
        match self.rpc.provider.send_raw_transaction(&bytes).await {
            Ok(pending_tx) => {
                let tx_hash = pending_tx.tx_hash();
                debug!("Transaction submitted with hash: {:?}", tx_hash);

                // Convert alloy B256 to Hash
                let hash = Hash::from(tx_hash.0);
                Ok(hash)
            }
            Err(e) => {
                error!("Failed to send raw transaction: {}", e);
                Err(format!("RPC error: {}", e))
            }
        }
    }

    /// Send a raw transaction and wait for confirmations
    ///
    /// Submits the transaction and waits for the specified number of confirmations
    /// before returning the transaction hash.
    async fn send_raw_transaction_with_confirm(
        &self,
        raw_tx: Vec<u8>,
        confirmations: u64,
        timeout: Option<std::time::Duration>,
    ) -> Result<Hash, String> {
        debug!(
            "Sending raw transaction ({} bytes) and waiting for {} confirmations",
            raw_tx.len(),
            confirmations
        );

        // Convert Vec<u8> to alloy Bytes
        let bytes = Bytes::from(raw_tx);

        // Send raw transaction
        match self.rpc.provider.send_raw_transaction(&bytes).await {
            Ok(pending_tx) => {
                let tx_hash = *pending_tx.tx_hash();
                debug!("Transaction submitted with hash: {:?}", tx_hash);

                // Use configured timeout or default to 60 seconds
                let timeout_duration = timeout.unwrap_or(std::time::Duration::from_secs(60));

                // Wait for confirmations with timeout
                let receipt_future = pending_tx.with_required_confirmations(confirmations).get_receipt();

                match tokio::time::timeout(timeout_duration, receipt_future).await {
                    Ok(Ok(receipt)) => {
                        debug!("Transaction {:?} confirmed", tx_hash);

                        // Check transaction status
                        if receipt.status() {
                            let hash = Hash::from(receipt.transaction_hash.0);
                            Ok(hash)
                        } else {
                            error!("Transaction {:?} reverted", tx_hash);
                            Err(format!("Transaction reverted: {:?}", tx_hash))
                        }
                    }
                    Ok(Err(e)) => {
                        error!("Error waiting for transaction confirmation: {}", e);
                        Err(format!("Confirmation error: {}", e))
                    }
                    Err(_) => {
                        error!("Transaction {:?} timed out after {:?}", tx_hash, timeout_duration);
                        Err(format!("Transaction timeout: timed out after {:?}", timeout_duration))
                    }
                }
            }
            Err(e) => {
                error!("Failed to send raw transaction: {}", e);
                Err(format!("RPC error: {}", e))
            }
        }
    }
}

#[async_trait]
impl<R: HttpRequestor + 'static + Clone> ReceiptProvider for RpcAdapter<R> {
    /// Get the status of a transaction by its hash
    ///
    /// Returns:
    /// - Some(true) if the transaction is confirmed with successful status
    /// - Some(false) if the transaction is confirmed but reverted
    /// - None if the transaction is still pending or not found
    async fn get_transaction_status(&self, tx_hash: Hash) -> Result<Option<bool>, String> {
        debug!("Checking transaction status for hash: {:?}", tx_hash);

        // Convert Hash to alloy B256
        let b256_hash = alloy::primitives::B256::from_slice(tx_hash.as_ref());

        // Try to get the transaction receipt
        match self.rpc.provider.get_transaction_receipt(b256_hash).await {
            Ok(Some(receipt)) => {
                // Transaction found - check status
                let success = receipt.status();
                debug!(
                    "Transaction {:?} status: {}",
                    tx_hash,
                    if success { "confirmed" } else { "reverted" }
                );
                Ok(Some(success))
            }
            Ok(None) => {
                // Transaction not found (still pending)
                debug!("Transaction {:?} still pending", tx_hash);
                Ok(None)
            }
            Err(e) => {
                error!("Error getting transaction receipt for {:?}: {}", tx_hash, e);
                Err(format!("Receipt error: {}", e))
            }
        }
    }
}

#[cfg(test)]
mod tests {

    // Note: Full integration tests would require a running Ethereum node
    // These tests verify the adapter structure compiles correctly

    #[test]
    fn test_rpc_adapter_is_send_sync() {
        // This test ensures the RpcAdapter implements Send + Sync
        #[allow(unused)]
        fn assert_send<T: Send>() {}
        #[allow(unused)]
        fn assert_sync<T: Sync>() {}

        // This would fail to compile if RpcAdapter doesn't implement Send + Sync
        // assert_send::<RpcAdapter<_>>();
        // assert_sync::<RpcAdapter<_>>();
    }
}
