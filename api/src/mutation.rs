//! GraphQL mutation root and resolver implementations for transaction submission

use std::sync::Arc;

use async_graphql::{Context, ID, Object, Result, Union};
use blokli_api_types::{
    ContractNotAllowedError, FunctionNotAllowedError, InvalidTransactionIdError, RpcError, SendTransactionSuccess,
    TimeoutError, Transaction, TransactionInput,
};
use blokli_chain_api::{
    DefaultHttpRequestor,
    rpc_adapter::RpcAdapter,
    transaction_executor::{RawTransactionExecutor, TransactionExecutorError},
    transaction_store::{TransactionRecord, TransactionStore},
    transaction_validator::ValidationError,
};

use crate::errors;

/// Root mutation type providing transaction submission capabilities
pub struct MutationRoot;

/// Result type for fire-and-forget transaction submission
#[derive(Union)]
pub enum SendTransactionResult {
    Success(SendTransactionSuccess),
    ContractNotAllowed(ContractNotAllowedError),
    FunctionNotAllowed(FunctionNotAllowedError),
    RpcError(RpcError),
}

/// Result type for asynchronous transaction submission
#[derive(Union)]
pub enum SendTransactionAsyncResult {
    Transaction(Transaction),
    ContractNotAllowed(ContractNotAllowedError),
    FunctionNotAllowed(FunctionNotAllowedError),
    RpcError(RpcError),
}

/// Result type for synchronous transaction submission
#[derive(Union)]
pub enum SendTransactionSyncResult {
    Transaction(Transaction),
    ContractNotAllowed(ContractNotAllowedError),
    FunctionNotAllowed(FunctionNotAllowedError),
    RpcError(RpcError),
    Timeout(TimeoutError),
}

/// Result type for transaction query
#[derive(Union)]
pub enum TransactionResult {
    Transaction(Transaction),
    InvalidId(InvalidTransactionIdError),
}

#[Object]
impl MutationRoot {
    /// Submit a transaction with fire-and-forget mode
    ///
    /// Validates the pre-signed raw transaction data and submits it to the chain.
    /// Returns the transaction hash immediately after submission.
    /// Does not wait for confirmation and does not track transaction status.
    /// Use this mode for maximum performance when you don't need confirmation tracking.
    #[graphql(name = "sendTransaction")]
    async fn send_transaction(&self, ctx: &Context<'_>, input: TransactionInput) -> Result<SendTransactionResult> {
        let executor = ctx
            .data::<Arc<RawTransactionExecutor<RpcAdapter<DefaultHttpRequestor>>>>()
            .map_err(|_| async_graphql::Error::new("Transaction executor not available"))?;

        // Decode hex transaction data
        let raw_tx = hex_to_bytes(&input.raw_transaction)?;

        // Execute transaction in fire-and-forget mode
        match executor.send_raw_transaction(raw_tx).await {
            Ok(tx_hash) => Ok(SendTransactionResult::Success(SendTransactionSuccess {
                transaction_hash: tx_hash.into(),
            })),
            Err(e) => Ok(executor_error_to_send_result(e)),
        }
    }

    /// Submit a transaction asynchronously
    ///
    /// Validates the pre-signed raw transaction data and submits it to the chain immediately.
    /// Returns the transaction ID that can be used to query status later.
    /// Does not wait for on-chain confirmation. Background monitor tracks confirmation.
    #[graphql(name = "sendTransactionAsync")]
    async fn send_transaction_async(
        &self,
        ctx: &Context<'_>,
        input: TransactionInput,
    ) -> Result<SendTransactionAsyncResult> {
        let executor = ctx
            .data::<Arc<RawTransactionExecutor<RpcAdapter<DefaultHttpRequestor>>>>()
            .map_err(|_| async_graphql::Error::new("Transaction executor not available"))?;

        let store = ctx
            .data::<Arc<TransactionStore>>()
            .map_err(|_| async_graphql::Error::new("Transaction store not available"))?;

        // Decode hex transaction data
        let raw_tx = hex_to_bytes(&input.raw_transaction)?;

        // Execute transaction in async mode
        match executor.send_raw_transaction_async(raw_tx).await {
            Ok(uuid) => {
                // Retrieve the transaction record
                let record = store
                    .get(uuid)
                    .map_err(|e| async_graphql::Error::new(format!("Failed to retrieve transaction: {}", e)))?;

                Ok(SendTransactionAsyncResult::Transaction(record_to_graphql(record)))
            }
            Err(e) => Ok(executor_error_to_async_result(e)),
        }
    }

    /// Submit a transaction synchronously
    ///
    /// Validates the pre-signed raw transaction data, submits it to the chain, and waits for
    /// the specified number of confirmations (default: 8 blocks) before returning.
    /// Transaction is persisted to store and can be queried later.
    #[graphql(name = "sendTransactionSync")]
    async fn send_transaction_sync(
        &self,
        ctx: &Context<'_>,
        input: TransactionInput,
        confirmations: Option<i32>,
    ) -> Result<SendTransactionSyncResult> {
        let executor = ctx
            .data::<Arc<RawTransactionExecutor<RpcAdapter<DefaultHttpRequestor>>>>()
            .map_err(|_| async_graphql::Error::new("Transaction executor not available"))?;

        // Decode hex transaction data
        let raw_tx = hex_to_bytes(&input.raw_transaction)?;

        // Validate and convert confirmations value
        let confirmations = match confirmations {
            Some(c) if c < 0 => {
                return Err(async_graphql::Error::new(format!(
                    "Invalid confirmations value: {}. Must be non-negative (default: 8, max: 64)",
                    c
                )));
            }
            Some(c) => Some(u64::try_from(c).unwrap()), // Safe: already validated c >= 0
            None => None,
        };

        // Execute transaction in sync mode
        match executor.send_raw_transaction_sync(raw_tx, confirmations).await {
            Ok(record) => Ok(SendTransactionSyncResult::Transaction(record_to_graphql(record))),
            Err(e) => Ok(executor_error_to_sync_result(e)),
        }
    }
}

/// Helper function to convert hex string to bytes
fn hex_to_bytes(hex_str: &str) -> Result<Vec<u8>> {
    let hex_str = hex_str.to_lowercase();
    let hex_str = hex_str.strip_prefix("0x").unwrap_or(&hex_str);
    hex::decode(hex_str).map_err(|e| async_graphql::Error::new(format!("Invalid hex string: {}", e)))
}

/// Convert TransactionRecord to GraphQL Transaction
fn record_to_graphql(record: TransactionRecord) -> Transaction {
    Transaction {
        id: ID::from(record.id.to_string()),
        status: crate::conversions::store_status_to_graphql(record.status),
        submitted_at: record.submitted_at,
        transaction_hash: record.transaction_hash.into(),
    }
}

/// Convert TransactionExecutorError to SendTransactionResult
fn executor_error_to_send_result(error: TransactionExecutorError) -> SendTransactionResult {
    match error {
        TransactionExecutorError::ValidationFailed(ValidationError::ContractNotAllowed(address)) => {
            SendTransactionResult::ContractNotAllowed(errors::contract_not_allowed(address))
        }
        TransactionExecutorError::ValidationFailed(ValidationError::FunctionNotAllowed(address, selector)) => {
            SendTransactionResult::FunctionNotAllowed(errors::function_not_allowed(address, selector))
        }
        TransactionExecutorError::ValidationFailed(_) => {
            SendTransactionResult::RpcError(errors::rpc_validation_failed(&error))
        }
        TransactionExecutorError::RpcError(msg) => SendTransactionResult::RpcError(errors::rpc_error_with_message(msg)),
        _ => SendTransactionResult::RpcError(errors::rpc_internal_error(&error)),
    }
}

/// Convert TransactionExecutorError to SendTransactionAsyncResult
fn executor_error_to_async_result(error: TransactionExecutorError) -> SendTransactionAsyncResult {
    match error {
        TransactionExecutorError::ValidationFailed(ValidationError::ContractNotAllowed(address)) => {
            SendTransactionAsyncResult::ContractNotAllowed(errors::contract_not_allowed(address))
        }
        TransactionExecutorError::ValidationFailed(ValidationError::FunctionNotAllowed(address, selector)) => {
            SendTransactionAsyncResult::FunctionNotAllowed(errors::function_not_allowed(address, selector))
        }
        TransactionExecutorError::ValidationFailed(_) => {
            SendTransactionAsyncResult::RpcError(errors::rpc_validation_failed(&error))
        }
        TransactionExecutorError::RpcError(msg) => {
            SendTransactionAsyncResult::RpcError(errors::rpc_error_with_message(msg))
        }
        _ => SendTransactionAsyncResult::RpcError(errors::rpc_internal_error(&error)),
    }
}

/// Convert TransactionExecutorError to SendTransactionSyncResult
fn executor_error_to_sync_result(error: TransactionExecutorError) -> SendTransactionSyncResult {
    match error {
        TransactionExecutorError::ValidationFailed(ValidationError::ContractNotAllowed(address)) => {
            SendTransactionSyncResult::ContractNotAllowed(errors::contract_not_allowed(address))
        }
        TransactionExecutorError::ValidationFailed(ValidationError::FunctionNotAllowed(address, selector)) => {
            SendTransactionSyncResult::FunctionNotAllowed(errors::function_not_allowed(address, selector))
        }
        TransactionExecutorError::ValidationFailed(_) => {
            SendTransactionSyncResult::RpcError(errors::rpc_validation_failed(&error))
        }
        TransactionExecutorError::RpcError(msg) => {
            SendTransactionSyncResult::RpcError(errors::rpc_error_with_message(msg))
        }
        TransactionExecutorError::Timeout => SendTransactionSyncResult::Timeout(errors::timeout_error(
            "Transaction was not confirmed within the timeout window",
        )),
        _ => SendTransactionSyncResult::RpcError(errors::rpc_internal_error(&error)),
    }
}
