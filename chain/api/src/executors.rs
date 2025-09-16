use std::{marker::PhantomData, time::Duration};

use alloy::{providers::PendingTransaction, rpc::types::TransactionRequest};
use async_trait::async_trait;
use blokli_chain_actions::{action_queue::TransactionExecutor, payload::PayloadGenerator};
use blokli_chain_rpc::{HoprRpcOperations, errors::RpcError};
use futures::{FutureExt, future::Either, pin_mut};
use hopr_async_runtime::prelude::sleep;
use hopr_crypto_types::types::Hash;
use hopr_internal_types::prelude::*;
use hopr_primitive_types::prelude::*;
use serde::{Deserialize, Serialize};

/// Represents an abstract client that is capable of submitting
/// an Ethereum transaction-like object to the blockchain.
#[async_trait]
pub trait EthereumClient<T: Into<TransactionRequest>> {
    /// Sends transaction to the blockchain and returns its hash.
    ///
    /// Does not poll for transaction completion.
    async fn post_transaction(&self, tx: T) -> blokli_chain_rpc::errors::Result<Hash>;

    /// Sends transaction to the blockchain and awaits the required number
    /// of confirmations by polling the underlying provider periodically.
    ///
    /// Returns the TX hash.
    async fn post_transaction_and_await_confirmation(&self, tx: T) -> blokli_chain_rpc::errors::Result<Hash>;
}

#[derive(Clone, Debug, PartialEq, smart_default::SmartDefault, Serialize, Deserialize)]
pub struct RpcEthereumClientConfig {
    /// Maximum time to wait for the TX to get submitted.
    ///
    /// This should be strictly greater than any timeouts in the underlying `HoprRpcOperations`
    ///
    /// Defaults to 5 seconds.
    #[default(Duration::from_secs(5))]
    pub max_tx_submission_wait: Duration,
}

/// Instantiation of `EthereumClient` using `HoprRpcOperations`.
#[derive(Debug, Clone)]
pub struct RpcEthereumClient<Rpc: HoprRpcOperations> {
    rpc: Rpc,
    cfg: RpcEthereumClientConfig,
}

impl<Rpc: HoprRpcOperations> RpcEthereumClient<Rpc> {
    pub fn new(rpc: Rpc, cfg: RpcEthereumClientConfig) -> Self {
        Self { rpc, cfg }
    }

    /// Post a transaction with a specified timeout.
    ///
    /// If the transaction yields a result before the timeout, the result value is returned.
    /// Otherwise, an [RpcError::Timeout] is returned and the transaction sending is aborted.
    async fn post_tx_with_timeout(
        &self,
        tx: TransactionRequest,
    ) -> blokli_chain_rpc::errors::Result<PendingTransaction> {
        let submit_tx = self.rpc.send_transaction(tx).fuse();
        let timeout = sleep(self.cfg.max_tx_submission_wait).fuse();
        pin_mut!(submit_tx, timeout);

        match futures::future::select(submit_tx, timeout).await {
            Either::Left((res, _)) => res,
            Either::Right(_) => Err(RpcError::Timeout),
        }
    }

    /// Post a transaction with a specified timeout and await its confirmation.
    ///
    /// If the transaction yields a result before the timeout, the result value is returned.
    /// Otherwise, an [RpcError::Timeout] is returned and the transaction sending is aborted.
    async fn post_tx_with_timeout_and_confirm(&self, tx: TransactionRequest) -> blokli_chain_rpc::errors::Result<Hash> {
        let submit_tx = self.rpc.send_transaction_with_confirm(tx).fuse();
        let timeout = sleep(self.cfg.max_tx_submission_wait).fuse();
        pin_mut!(submit_tx, timeout);

        match futures::future::select(submit_tx, timeout).await {
            Either::Left((res, _)) => res,
            Either::Right(_) => Err(RpcError::Timeout),
        }
    }
}

#[async_trait]
impl<Rpc: HoprRpcOperations + Send + Sync> EthereumClient<TransactionRequest> for RpcEthereumClient<Rpc> {
    async fn post_transaction(&self, tx: TransactionRequest) -> blokli_chain_rpc::errors::Result<Hash> {
        self.post_tx_with_timeout(tx).await.map(|t| t.tx_hash().0.into())
    }

    /// Post a transaction and wait for its completion.
    ///
    /// The mechanism uses an internal timeout and retry mechanism (set to `3`)
    async fn post_transaction_and_await_confirmation(
        &self,
        tx: TransactionRequest,
    ) -> blokli_chain_rpc::errors::Result<Hash> {
        self.post_tx_with_timeout_and_confirm(tx).await
    }
}

/// Implementation of [`TransactionExecutor`] using the given [`EthereumClient`] and corresponding
/// [`PayloadGenerator`].
#[derive(Clone, Debug)]
pub struct EthereumTransactionExecutor<T, C, PGen>
where
    T: Into<TransactionRequest>,
    C: EthereumClient<T> + Clone,
    PGen: PayloadGenerator<T> + Clone,
{
    client: C,
    payload_generator: PGen,
    _data: PhantomData<T>,
}

impl<T, C, PGen> EthereumTransactionExecutor<T, C, PGen>
where
    T: Into<TransactionRequest>,
    C: EthereumClient<T> + Clone,
    PGen: PayloadGenerator<T> + Clone,
{
    pub fn new(client: C, payload_generator: PGen) -> Self {
        Self {
            client,
            payload_generator,
            _data: PhantomData,
        }
    }
}

#[async_trait]
impl<T, C, PGen> TransactionExecutor for EthereumTransactionExecutor<T, C, PGen>
where
    T: Into<TransactionRequest> + Sync + Send,
    C: EthereumClient<T> + Clone + Sync + Send,
    PGen: PayloadGenerator<T> + Clone + Sync + Send,
{
    async fn withdraw<Cr: Currency + Send>(
        &self,
        recipient: Address,
        amount: Balance<Cr>,
    ) -> blokli_chain_actions::errors::Result<Hash> {
        let payload = self.payload_generator.transfer(recipient, amount)?;

        // Withdraw transaction is out-of-band from Indexer, so its confirmation
        // is awaited via polling.
        Ok(self.client.post_transaction_and_await_confirmation(payload).await?)
    }

    async fn register_safe(&self, safe_address: Address) -> blokli_chain_actions::errors::Result<Hash> {
        let payload = self.payload_generator.register_safe_by_node(safe_address)?;
        Ok(self.client.post_transaction(payload).await?)
    }
}
