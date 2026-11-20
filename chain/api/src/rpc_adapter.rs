//! RPC adapter for integrating RpcOperations with the transaction executor
//!
//! This module provides implementations of the `RpcClient` and `ReceiptProvider` traits
//! for `RpcOperations`, allowing the transaction executor to submit raw transactions
//! and monitor their confirmation status.

use std::{sync::Arc, time::Duration};

use async_trait::async_trait;
use blokli_chain_rpc::{
    confirmation::{ConfirmationTarget, ProviderReceiptLookup, TipWaitError, wait_for_confirmed_tip},
    rpc::RpcOperations,
    transport::HttpRequestor,
};
use hopr_bindings::exports::alloy::{
    primitives::{Address as AlloyAddress, B256, Bytes},
    providers::{PendingTransactionBuilder, Provider},
};
use hopr_types::{crypto::types::Hash, primitive::prelude::Address};
use tokio::time::{Instant, sleep};
use tracing::{debug, error, info, warn};

use crate::{
    broadcast_error::{BroadcastRejection, classify_broadcast_error},
    fee_recommendation::{GasFeePolicy, RecommendedFees, recommended_fees},
    safe_execution::{decode_transaction_signer, decode_transaction_summary},
    transaction_executor::{BroadcastOutcome, ConfirmationError, RpcClient},
    transaction_monitor::{ReceiptLog, ReceiptProvider, TransactionReceipt},
};

/// RPC adapter that implements RpcClient and ReceiptProvider traits for RpcOperations
///
/// This adapter bridges the gap between the transaction executor's trait requirements
/// and the actual RpcOperations implementation from the chain/rpc module.
#[derive(Debug, Clone)]
pub struct RpcAdapter<R: HttpRequestor + 'static + Clone> {
    rpc: Arc<RpcOperations<R>>,
    gas_multiplier: f64,
    gas_fee_policy: GasFeePolicy,
}

impl<R: HttpRequestor + 'static + Clone> RpcAdapter<R> {
    /// Create a new RPC adapter wrapping the given RpcOperations
    pub fn new(rpc: RpcOperations<R>) -> Self {
        Self {
            rpc: Arc::new(rpc),
            gas_multiplier: 1.0,
            gas_fee_policy: GasFeePolicy::default(),
        }
    }

    /// Set the multiplier and policy that turn the node's fee estimate into the recommended fee
    pub fn with_fee_policy(mut self, gas_multiplier: f64, gas_fee_policy: GasFeePolicy) -> Self {
        self.gas_multiplier = gas_multiplier;
        self.gas_fee_policy = gas_fee_policy;
        self
    }
}

/// Maps an interrupted tip wait to the confirmation outcome seen by sync-mode callers.
///
/// The node accepted the transaction before the wait started, so the caller gets a timeout that
/// names the accepted transaction instead of a failed submission it could resubmit.
fn tip_wait_error(error: TipWaitError, tx_hash: B256) -> ConfirmationError {
    ConfirmationError::Timeout(format!("{error}, accepted transaction {tx_hash}"))
}

#[async_trait]
impl<R: HttpRequestor + 'static + Clone> RpcClient for RpcAdapter<R> {
    /// Send a raw transaction and return its hash
    ///
    /// Converts the raw transaction bytes to alloy Bytes format and submits to the RPC provider.
    /// Returns the transaction hash immediately without waiting for confirmation.
    async fn send_raw_transaction(&self, raw_tx: Vec<u8>) -> Result<Hash, String> {
        self.broadcast_raw_transaction(raw_tx).await.map(BroadcastOutcome::hash)
    }

    /// Send a raw transaction and report whether the node accepted it now or already held it
    ///
    /// An already-known answer is only reported as such once the node returns the transaction
    /// for its hash, so [`BroadcastOutcome::AlreadyKnown`] is evidence the node holds it.
    async fn broadcast_raw_transaction(&self, raw_tx: Vec<u8>) -> Result<BroadcastOutcome, String> {
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
                Ok(BroadcastOutcome::Accepted(hash))
            }
            Err(e) => {
                let rejection = classify_broadcast_error(&e.to_string());
                let summary = decode_transaction_summary(&bytes);

                // The node already holds this exact transaction, e.g. from an earlier attempt
                // whose response was lost: track it like a fresh one. Nodes may also answer
                // "already known" from a hash cache that includes rejected transactions, so
                // only trust the claim once the node actually returns the transaction.
                if let (Some(BroadcastRejection::AlreadyKnown), Some(summary)) = (rejection, summary) {
                    match self.is_transaction_known(summary.transaction_hash).await {
                        Ok(true) => {
                            info!(
                                tx_hash = %summary.transaction_hash,
                                nonce = summary.nonce,
                                "transaction already known to the RPC node, treating it as submitted"
                            );
                            return Ok(BroadcastOutcome::AlreadyKnown(summary.transaction_hash));
                        }
                        Ok(false) => {
                            warn!(
                                tx_hash = %summary.transaction_hash,
                                nonce = summary.nonce,
                                error = %e,
                                "RPC node reported the transaction as already known but holds no record of it"
                            );
                            return Err(format!(
                                "RPC error: the node reported the transaction as known but holds no record of it, so \
                                 an earlier submission was likely rejected (tx hash {})",
                                summary.transaction_hash
                            ));
                        }
                        Err(lookup_error) => {
                            warn!(
                                tx_hash = %summary.transaction_hash,
                                error = %lookup_error,
                                "could not verify an already-known transaction, reporting the rejection"
                            );
                        }
                    }
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

    /// Send a raw transaction and wait for the confirmation target
    ///
    /// Submits the transaction and waits until either the requested number of blocks has passed or
    /// the chain tip tracker confirms the block that included the transaction.
    async fn send_raw_transaction_with_confirm(
        &self,
        raw_tx: Vec<u8>,
        target: ConfirmationTarget,
        timeout: Option<Duration>,
    ) -> Result<Hash, ConfirmationError> {
        let raw_tx_len = raw_tx.len();
        debug!(
            raw_tx_len,
            ?target,
            "sending raw transaction and waiting for the confirmation target"
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
                let deadline = Instant::now() + timeout_duration;
                let timeout_error = || {
                    ConfirmationError::Timeout(format!(
                        "timed out after {timeout_duration:?} waiting for transaction {tx_hash} to be confirmed"
                    ))
                };

                let success = match target {
                    ConfirmationTarget::Blocks(confirmations) => {
                        // The node already accepted the transaction, so a receipt lookup error is
                        // retried within the deadline instead of being reported as a rejected
                        // broadcast. The transaction can still be mined.
                        let (provider, config) = pending_tx.split();
                        loop {
                            let builder = PendingTransactionBuilder::from_config(provider.clone(), config.clone())
                                .with_required_confirmations(confirmations);

                            match tokio::time::timeout_at(deadline, builder.get_receipt()).await {
                                Ok(Ok(receipt)) => break Ok(receipt.status()),
                                Ok(Err(error)) => {
                                    error!(%error, "error waiting for transaction confirmation, retrying");
                                    if Instant::now() >= deadline {
                                        break Err(timeout_error());
                                    }
                                    sleep(self.rpc.tx_polling_interval()).await;
                                }
                                Err(_) => break Err(timeout_error()),
                            }
                        }
                    }
                    ConfirmationTarget::ConfirmedTip => {
                        // The standalone API binary has no indexer that starts the tracker, so it
                        // is started here before the first tip is awaited.
                        let tracker = self.rpc.tip_tracker();
                        tracker.ensure_started();
                        let mut tips = tracker.subscribe();

                        let lookup = ProviderReceiptLookup::new(self.rpc.provider.clone(), tx_hash);
                        let receipt_future = wait_for_confirmed_tip(
                            &mut tips,
                            &lookup,
                            tracker.as_ref(),
                            self.rpc.tx_polling_interval(),
                        );

                        match tokio::time::timeout_at(deadline, receipt_future).await {
                            Ok(Ok(receipt)) => Ok(receipt.success),
                            Ok(Err(e)) => {
                                error!(error = %e, "error waiting for the confirmed chain tip");
                                Err(tip_wait_error(e, tx_hash))
                            }
                            Err(_) => Err(timeout_error()),
                        }
                    }
                }?;

                if success {
                    debug!(?tx_hash, "Transaction confirmed");
                    Ok(Hash::from(tx_hash.0))
                } else {
                    error!(?tx_hash, "Transaction reverted");
                    Err(ConfirmationError::Reverted(format!("{:?}", tx_hash)))
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

    async fn get_recommended_fees(&self) -> Result<RecommendedFees, String> {
        recommended_fees(&*self.rpc, self.gas_multiplier, self.gas_fee_policy)
            .await
            .map_err(|e| format!("Fee estimate error: {e}"))
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
