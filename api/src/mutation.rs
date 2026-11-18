//! GraphQL mutation root and resolver implementations for transaction submission

use std::sync::Arc;

use async_graphql::{Context, Object, Result, Union};
use blokli_api_types::{
    ContractNotAllowedError, DeduplicatedTransaction, FunctionNotAllowedError, HoprActionRejectedError,
    HoprActionThrottledError, InvalidTransactionIdError, OverloadedError, RpcError, SendTransactionSuccess,
    TimeoutError, Transaction, TransactionInput,
};
use blokli_chain_api::{
    DefaultHttpRequestor,
    rpc_adapter::RpcAdapter,
    transaction_executor::{RawTransactionExecutor, TransactionExecutorError},
    transaction_store::TransactionStore,
    transaction_validator::ValidationError,
};

use crate::{conversions::transaction_from_record, errors};

/// Root mutation type providing transaction submission capabilities
pub struct MutationRoot;

/// Result type for fire-and-forget transaction submission
#[derive(Union)]
pub enum SendTransactionResult {
    Success(SendTransactionSuccess),
    ContractNotAllowed(ContractNotAllowedError),
    FunctionNotAllowed(FunctionNotAllowedError),
    RpcError(RpcError),
    HoprActionRejected(HoprActionRejectedError),
    HoprActionThrottled(HoprActionThrottledError),
}

/// Result type for asynchronous transaction submission
#[derive(Union)]
pub enum SendTransactionAsyncResult {
    Transaction(Transaction),
    ContractNotAllowed(ContractNotAllowedError),
    FunctionNotAllowed(FunctionNotAllowedError),
    RpcError(RpcError),
    Overloaded(OverloadedError),
    HoprActionRejected(HoprActionRejectedError),
    HoprActionThrottled(HoprActionThrottledError),
    Deduplicated(DeduplicatedTransaction),
}

/// Result type for synchronous transaction submission
#[derive(Union)]
pub enum SendTransactionSyncResult {
    Transaction(Transaction),
    ContractNotAllowed(ContractNotAllowedError),
    FunctionNotAllowed(FunctionNotAllowedError),
    RpcError(RpcError),
    Timeout(TimeoutError),
    HoprActionRejected(HoprActionRejectedError),
    HoprActionThrottled(HoprActionThrottledError),
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
    ///
    /// A transaction with the same signer and nonce as a tracked pending one, but a different
    /// hash, is a fee-bumped replacement. It is broadcast and tracked as a transaction of its
    /// own, next to the original, which stays tracked. Both `maxFeePerGas` and
    /// `maxPriorityFeePerGas` must be at least 12.5% above the pending transaction's, or the
    /// recommended fees when those are higher. Refusals are `RpcError` results: code
    /// `REPLACEMENT_UNDERPRICED` with the minimum fees in the message, and
    /// `REPLACEMENT_LIMIT_REACHED` once the nonce was replaced the configured maximum number of
    /// times. Resubmitting the identical raw transaction returns the tracked one as before.
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
                    .get_public(uuid)
                    .map_err(|e| async_graphql::Error::new(format!("Failed to retrieve transaction: {}", e)))?;

                Ok(SendTransactionAsyncResult::Transaction(transaction_from_record(record)))
            }
            // An equivalent logical action is already tracked: hand back that transaction
            // rather than broadcasting a competing copy of the same intent.
            Err(TransactionExecutorError::DuplicateHoprAction { operation, existing }) => {
                match store.get_public(existing) {
                    Ok(record) => Ok(SendTransactionAsyncResult::Deduplicated(DeduplicatedTransaction {
                        transaction: transaction_from_record(record),
                        operation: operation.to_string(),
                    })),
                    // The tracked transaction concluded between the policy check and this
                    // lookup, so there is nothing to point at. Report capacity-neutral
                    // transience rather than inventing a terminal outcome.
                    Err(e) => Ok(SendTransactionAsyncResult::RpcError(errors::rpc_internal_error(e))),
                }
            }
            // The node still holds this exact transaction and it is already tracked: hand back
            // that transaction with its diagnosis instead of reporting a fresh submission.
            Err(TransactionExecutorError::AlreadyTracked { existing, diagnosis }) => match store.get_public(existing) {
                Ok(mut record) => {
                    // Resuming tracking cleared the stored diagnosis; report the one captured
                    // before that, so the caller still sees the fees and retry hint.
                    record.diagnosis = record.diagnosis.or(diagnosis.map(|diagnosis| *diagnosis));
                    Ok(SendTransactionAsyncResult::Deduplicated(DeduplicatedTransaction {
                        transaction: transaction_from_record(record),
                        operation: RAW_TRANSACTION_OPERATION.to_string(),
                    }))
                }
                Err(e) => Ok(SendTransactionAsyncResult::RpcError(errors::rpc_internal_error(e))),
            },
            Err(e) => Ok(executor_error_to_async_result(e)),
        }
    }

    /// Submit a transaction synchronously
    ///
    /// Validates the pre-signed raw transaction data, submits it to the chain, and waits for
    /// the specified number of confirmations (default: 3 blocks, max: 64) before returning.
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
        let confirmations = validate_sync_confirmations(confirmations)?;

        // Execute transaction in sync mode
        match executor.send_raw_transaction_sync(raw_tx, confirmations).await {
            Ok(record) => Ok(SendTransactionSyncResult::Transaction(transaction_from_record(record))),
            Err(e) => Ok(executor_error_to_sync_result(e)),
        }
    }
}

/// `operation` reported when a resubmitted raw transaction is deduplicated by its hash.
const RAW_TRANSACTION_OPERATION: &str = "raw_transaction";

/// Maximum number of confirmations `sendTransactionSync` will wait for.
const MAX_SYNC_CONFIRMATIONS: u64 = 64;

/// Validate the requested confirmations: non-negative and at most [`MAX_SYNC_CONFIRMATIONS`].
fn validate_sync_confirmations(confirmations: Option<i32>) -> Result<Option<u64>> {
    let Some(c) = confirmations else {
        return Ok(None);
    };
    match u64::try_from(c) {
        Ok(v) if v <= MAX_SYNC_CONFIRMATIONS => Ok(Some(v)),
        _ => Err(errors::invalid_confirmations(c, MAX_SYNC_CONFIRMATIONS)),
    }
}

/// Helper function to convert hex string to bytes
fn hex_to_bytes(hex_str: &str) -> Result<Vec<u8>> {
    let hex_str = hex_str.to_lowercase();
    let hex_str = hex_str.strip_prefix("0x").unwrap_or(&hex_str);
    hex::decode(hex_str).map_err(|e| async_graphql::Error::new(format!("Invalid hex string: {}", e)))
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
        TransactionExecutorError::HoprActionRejected { operation, reason } => {
            SendTransactionResult::HoprActionRejected(errors::hopr_action_rejected(
                operation,
                reason.code(),
                reason.message(),
            ))
        }
        TransactionExecutorError::HoprActionThrottled {
            operation,
            reason,
            retry_after,
        } => SendTransactionResult::HoprActionThrottled(errors::hopr_action_throttled(
            operation,
            reason.code(),
            reason.message(),
            retry_after,
        )),
        TransactionExecutorError::RpcError(msg) => SendTransactionResult::RpcError(errors::rpc_broadcast_error(msg)),
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
            SendTransactionAsyncResult::RpcError(errors::rpc_broadcast_error(msg))
        }
        TransactionExecutorError::HoprActionRejected { operation, reason } => {
            SendTransactionAsyncResult::HoprActionRejected(errors::hopr_action_rejected(
                operation,
                reason.code(),
                reason.message(),
            ))
        }
        TransactionExecutorError::HoprActionThrottled {
            operation,
            reason,
            retry_after,
        } => SendTransactionAsyncResult::HoprActionThrottled(errors::hopr_action_throttled(
            operation,
            reason.code(),
            reason.message(),
            retry_after,
        )),
        TransactionExecutorError::OverloadedError => {
            SendTransactionAsyncResult::Overloaded(errors::submission_capacity_exceeded())
        }
        // Replacement refusals reuse `RpcError`, so no client needs a new result type.
        TransactionExecutorError::ReplacementUnderpriced { .. } => {
            SendTransactionAsyncResult::RpcError(errors::replacement_underpriced(error.to_string()))
        }
        TransactionExecutorError::ReplacementLimitReached { .. } => {
            SendTransactionAsyncResult::RpcError(errors::replacement_limit_reached(error.to_string()))
        }
        TransactionExecutorError::ReplacementConflict => {
            SendTransactionAsyncResult::RpcError(errors::rpc_error_with_message(error.to_string()))
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
            SendTransactionSyncResult::RpcError(errors::rpc_broadcast_error(msg))
        }
        TransactionExecutorError::HoprActionRejected { operation, reason } => {
            SendTransactionSyncResult::HoprActionRejected(errors::hopr_action_rejected(
                operation,
                reason.code(),
                reason.message(),
            ))
        }
        TransactionExecutorError::HoprActionThrottled {
            operation,
            reason,
            retry_after,
        } => SendTransactionSyncResult::HoprActionThrottled(errors::hopr_action_throttled(
            operation,
            reason.code(),
            reason.message(),
            retry_after,
        )),
        TransactionExecutorError::Timeout(msg) => SendTransactionSyncResult::Timeout(errors::timeout_error(msg)),
        _ => SendTransactionSyncResult::RpcError(errors::rpc_internal_error(&error)),
    }
}

#[cfg(test)]
mod tests {
    use async_graphql::{EmptySubscription, Schema};
    use blokli_chain_api::transaction_store::FeePair;

    use super::*;
    use crate::query::QueryRoot;

    #[test]
    fn test_replacement_refusals_are_rpc_errors_with_the_fees_and_the_cap() {
        let underpriced = executor_error_to_async_result(TransactionExecutorError::ReplacementUnderpriced {
            minimum: FeePair {
                max_fee_per_gas: 1_125_000_000,
                max_priority_fee_per_gas: 250_000_000,
            },
        });
        let SendTransactionAsyncResult::RpcError(RpcError { code, message }) = underpriced else {
            panic!("a below-minimum replacement must be an RpcError");
        };
        assert_eq!(code, errors::codes::REPLACEMENT_UNDERPRICED);
        assert!(message.contains("1125000000"), "{message}");
        assert!(message.contains("250000000"), "{message}");

        let capped = executor_error_to_async_result(TransactionExecutorError::ReplacementLimitReached { max: 3 });
        let SendTransactionAsyncResult::RpcError(RpcError { code, message }) = capped else {
            panic!("an over-cap replacement must be an RpcError");
        };
        assert_eq!(code, errors::codes::REPLACEMENT_LIMIT_REACHED);
        assert!(message.contains('3'), "{message}");

        let conflict = executor_error_to_async_result(TransactionExecutorError::ReplacementConflict);
        assert!(matches!(conflict, SendTransactionAsyncResult::RpcError(_)));
    }

    #[test]
    fn test_result_unions_and_error_types_are_unchanged_for_old_clients() {
        let sdl = Schema::build(QueryRoot, MutationRoot, EmptySubscription).finish().sdl();

        for union in [
            "union SendTransactionAsyncResult = Transaction | ContractNotAllowedError | FunctionNotAllowedError | \
             RpcError | OverloadedError | HoprActionRejectedError | HoprActionThrottledError | DeduplicatedTransaction",
            "union SendTransactionSyncResult = Transaction | ContractNotAllowedError | FunctionNotAllowedError | \
             RpcError | TimeoutError | HoprActionRejectedError | HoprActionThrottledError",
            "union SendTransactionResult = SendTransactionSuccess | ContractNotAllowedError | FunctionNotAllowedError \
             | RpcError | HoprActionRejectedError | HoprActionThrottledError",
        ] {
            assert!(sdl.contains(union), "missing or changed: {union}");
        }

        // `RpcError` keeps exactly its two fields.
        let rpc_error = sdl
            .split("type RpcError {")
            .nth(1)
            .and_then(|rest| rest.split('}').next())
            .expect("RpcError is in the schema");
        let fields: Vec<&str> = rpc_error
            .lines()
            .map(str::trim)
            .filter(|line| line.contains(": "))
            .collect();
        assert_eq!(fields, ["code: String!", "message: String!"]);

        // No status was added to the transaction status enum, and no replacement-specific type.
        let status = sdl
            .split("enum TransactionStatus {")
            .nth(1)
            .and_then(|rest| rest.split('}').next())
            .expect("TransactionStatus is in the schema");
        let values: Vec<&str> = status
            .lines()
            .map(|line| line.trim().split(' ').next().unwrap_or_default())
            .filter(|word| !word.is_empty() && word.chars().all(|c| c.is_ascii_uppercase() || c == '_'))
            .collect();
        assert_eq!(
            values,
            [
                "PENDING",
                "SUBMITTED",
                "CONFIRMED",
                "REVERTED",
                "TIMEOUT",
                "VALIDATION_FAILED",
                "SUBMISSION_FAILED"
            ]
        );
        assert!(!sdl.contains("replacedBy") && !sdl.contains("ReplacementError"));
    }

    #[test]
    fn test_sync_confirmations_bounds() {
        assert_eq!(validate_sync_confirmations(None).unwrap(), None);
        assert_eq!(validate_sync_confirmations(Some(0)).unwrap(), Some(0));
        assert_eq!(validate_sync_confirmations(Some(64)).unwrap(), Some(64));

        let too_high = validate_sync_confirmations(Some(65)).unwrap_err();
        assert!(too_high.message.contains("64"));
        assert!(validate_sync_confirmations(Some(-1)).is_err());
        assert!(validate_sync_confirmations(Some(i32::MAX)).is_err());
    }
}
