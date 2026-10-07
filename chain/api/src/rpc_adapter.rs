//! RPC adapter for integrating RpcOperations with the transaction executor
//!
//! This module provides implementations of the `RpcClient` and `ReceiptProvider` traits
//! for `RpcOperations`, allowing the transaction executor to submit raw transactions
//! and monitor their confirmation status.

use std::{sync::Arc, time::Duration};

use async_trait::async_trait;
use blokli_chain_rpc::{rpc::RpcOperations, transport::HttpRequestor};
use hopr_bindings::exports::alloy::{
    primitives::{Address as AlloyAddress, B256, Bytes},
    providers::Provider,
};
use hopr_types::{crypto::types::Hash, primitive::prelude::Address};
use tracing::{debug, error, info};

use crate::{
    broadcast_error::{BroadcastRejection, classify_broadcast_error},
    safe_execution::{decode_transaction_signer, decode_transaction_summary},
    transaction_executor::{ConfirmationError, RpcClient},
    transaction_monitor::{ReceiptLog, ReceiptProvider, TransactionReceipt},
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
        let raw_tx_len = raw_tx.len();
        debug!(length = raw_tx_len, "sending raw transaction");

        // Convert Vec<u8> to alloy Bytes
        let bytes = Bytes::from(raw_tx);

        // Send raw transaction using the provider's send_raw_transaction method
        match self.rpc.provider.send_raw_transaction(&bytes).await {
            Ok(pending_tx) => {
                let tx_hash = pending_tx.tx_hash();
                debug!(?tx_hash, "transaction submitted");

                // Convert alloy B256 to Hash
                let hash = Hash::from(tx_hash.0);
                Ok(hash)
            }
            Err(e) => {
                let rejection = classify_broadcast_error(&e.to_string());
                let summary = decode_transaction_summary(&bytes);

                // The node already holds this exact transaction, e.g. from an earlier attempt
                // whose response was lost: it is in the mempool, so track it like a fresh one.
                if let (Some(BroadcastRejection::AlreadyKnown), Some(summary)) = (rejection, summary) {
                    info!(
                        tx_hash = %summary.transaction_hash,
                        nonce = summary.nonce,
                        "transaction already known to the RPC node, treating it as submitted"
                    );
                    return Ok(summary.transaction_hash);
                }

                error!(
                    raw_tx_len,
                    rejection = rejection.map_or("unclassified", BroadcastRejection::label),
                    signer = ?decode_transaction_signer(&bytes).map(Address::from),
                    tx_hash = ?summary.map(|tx| tx.transaction_hash),
                    nonce = ?summary.map(|tx| tx.nonce),
                    to = ?summary.and_then(|tx| tx.to),
                    max_fee_per_gas = ?summary.map(|tx| tx.max_fee_per_gas),
                    max_priority_fee_per_gas = ?summary.and_then(|tx| tx.max_priority_fee_per_gas),
                    error = %e,
                    "failed to send raw transaction"
                );
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
    ) -> Result<Hash, ConfirmationError> {
        let raw_tx_len = raw_tx.len();
        debug!(
            raw_tx_len,
            confirmations, "sending raw transaction and waiting for confirmations"
        );

        // Convert Vec<u8> to alloy Bytes
        let bytes = Bytes::from(raw_tx);

        // Send raw transaction
        match self.rpc.provider.send_raw_transaction(&bytes).await {
            Ok(pending_tx) => {
                let tx_hash = *pending_tx.tx_hash();
                debug!(?tx_hash, "transaction submitted");

                // Use configured timeout or default to 60 seconds
                let timeout_duration = timeout.unwrap_or(Duration::from_secs(60));

                // Wait for confirmations with timeout
                let receipt_future = pending_tx.with_required_confirmations(confirmations).get_receipt();

                match tokio::time::timeout(timeout_duration, receipt_future).await {
                    Ok(Ok(receipt)) => {
                        debug!(?tx_hash, "Transaction confirmed");

                        // Check transaction status
                        if receipt.status() {
                            let hash = Hash::from(receipt.transaction_hash.0);
                            Ok(hash)
                        } else {
                            error!(?tx_hash, "Transaction reverted");
                            Err(ConfirmationError::Reverted(format!("{:?}", tx_hash)))
                        }
                    }
                    Ok(Err(e)) => {
                        error!(error = %e, "error waiting for transaction confirmation");
                        Err(ConfirmationError::SubmissionFailed(format!(
                            "Confirmation error: {}",
                            e
                        )))
                    }
                    Err(_) => {
                        error!(?timeout_duration, ?tx_hash, "Transaction timed out");
                        Err(ConfirmationError::Timeout(format!(
                            "timed out after {:?}",
                            timeout_duration
                        )))
                    }
                }
            }
            Err(e) => {
                error!(raw_tx_len, error = %e, "failed to send raw transaction");
                Err(ConfirmationError::SubmissionFailed(format!("RPC error: {}", e)))
            }
        }
    }
}

#[async_trait]
impl<R: HttpRequestor + 'static + Clone> ReceiptProvider for RpcAdapter<R> {
    async fn get_transaction_receipt(&self, tx_hash: Hash) -> Result<Option<TransactionReceipt>, String> {
        debug!(?tx_hash, "fetching transaction receipt");
        let b256_hash = B256::from_slice(tx_hash.as_ref());
        match self.rpc.provider.get_transaction_receipt(b256_hash).await {
            Ok(Some(receipt)) => {
                let success = receipt.status();
                let logs = receipt
                    .inner
                    .logs()
                    .iter()
                    .map(|log| ReceiptLog {
                        address: log.address().into_array(),
                        topics: log.topics().iter().map(|topic| topic.0).collect(),
                        data: log.data().data.to_vec(),
                    })
                    .collect();
                Ok(Some(TransactionReceipt { success, logs }))
            }
            Ok(None) => Ok(None),
            Err(error) => Err(format!("Receipt error: {error}")),
        }
    }

    async fn get_transaction_receipt_logs(&self, tx_hash: Hash) -> Result<Option<Vec<ReceiptLog>>, String> {
        debug!(?tx_hash, "fetching receipt logs");

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
                debug!(?tx_hash, "no receipt found, transaction still pending");
                Ok(None)
            }
            Err(e) => {
                error!(?tx_hash, error = %e, "error getting receipt logs");
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
            // RPC supports tracing (verified at startup) but this specific call failed.
            // Surface it so callers count the failure instead of mistaking it for a
            // successful trace without a decodable reason; confirmation never waits on it.
            Err(e) => Err(format!("debug_traceTransaction failed: {e}")),
        }
    }

    async fn is_transaction_known(&self, tx_hash: Hash) -> Result<bool, String> {
        let b256_hash = B256::from_slice(tx_hash.as_ref());
        self.rpc
            .provider
            .get_transaction_by_hash(b256_hash)
            .await
            .map(|transaction| transaction.is_some())
            .map_err(|e| format!("Transaction lookup error: {e}"))
    }

    async fn get_mined_nonce(&self, address: [u8; 20]) -> Result<u64, String> {
        self.rpc
            .provider
            .get_transaction_count(AlloyAddress::from(address))
            .latest()
            .await
            .map_err(|e| format!("Nonce lookup error: {e}"))
    }

    async fn get_pending_nonce(&self, address: [u8; 20]) -> Result<u64, String> {
        self.rpc
            .provider
            .get_transaction_count(AlloyAddress::from(address))
            .pending()
            .await
            .map_err(|e| format!("Pending nonce lookup error: {e}"))
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
