//! RPC adapter for integrating RpcOperations with the transaction executor
//!
//! This module provides implementations of the `RpcClient` and `ReceiptProvider` traits
//! for `RpcOperations`, allowing the transaction executor to submit raw transactions
//! and monitor their confirmation status.

use std::{sync::Arc, time::Duration};

use async_trait::async_trait;
use blokli_chain_rpc::{rpc::RpcOperations, transport::HttpRequestor};
use hopr_bindings::exports::alloy::{
    primitives::{B256, Bytes},
    providers::Provider,
};
use hopr_crypto_types::types::Hash;
use tracing::{debug, error, warn};

use crate::{
    transaction_executor::RpcClient,
    transaction_monitor::{ReceiptLog, ReceiptProvider},
};

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
        timeout: Option<Duration>,
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
                let timeout_duration = timeout.unwrap_or(Duration::from_secs(60));

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
        let b256_hash = B256::from_slice(tx_hash.as_ref());

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

    async fn get_transaction_receipt_logs(&self, tx_hash: Hash) -> Result<Option<Vec<ReceiptLog>>, String> {
        debug!("Fetching receipt logs for hash: {:?}", tx_hash);

        let b256_hash = B256::from_slice(tx_hash.as_ref());

        match self.rpc.provider.get_transaction_receipt(b256_hash).await {
            Ok(Some(receipt)) => {
                let logs = receipt
                    .inner
                    .logs()
                    .iter()
                    .map(|log| ReceiptLog {
                        address: log.address().into_array(),
                        topics: log.topics().iter().map(|t| t.0).collect(),
                        data: log.data().data.to_vec(),
                    })
                    .collect();
                Ok(Some(logs))
            }
            Ok(None) => {
                debug!("No receipt found for {:?} (transaction still pending)", tx_hash);
                Ok(None)
            }
            Err(e) => {
                error!("Error getting receipt logs for {:?}: {}", tx_hash, e);
                Err(format!("Receipt error: {}", e))
            }
        }
    }

    async fn get_revert_reason(&self, tx_hash: Hash) -> Result<Option<String>, String> {
        let b256_hash = B256::from_slice(tx_hash.as_ref());

        let params = serde_json::json!([
            format!("{b256_hash:#x}"),
            { "tracer": "callTracer", "tracerConfig": { "onlyTopCall": false } }
        ]);

        match self
            .rpc
            .provider
            .raw_request::<_, serde_json::Value>("debug_traceTransaction".into(), params)
            .await
        {
            Ok(trace) => {
                let output = crate::revert_decoder::extract_revert_output_from_trace(&trace);
                Ok(output.and_then(|b| crate::revert_decoder::decode_revert_reason(&b)))
            }
            Err(e) => {
                // RPC supports tracing (verified at startup) but this specific
                // call failed — log and return None rather than blocking confirmation.
                warn!("debug_traceTransaction failed for {b256_hash:#x}: {e}");
                Ok(None)
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
