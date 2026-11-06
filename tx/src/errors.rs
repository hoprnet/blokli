//! Error types for the transaction filter.

use thiserror::Error;

/// Result type for transaction filtering operations.
pub type Result<T, E = FilterError> = core::result::Result<T, E>;

/// Reasons a signed transaction can be rejected by [`TransactionFilter`].
///
/// # Example
///
/// ```
/// use blokli_tx::{FilterError, TransactionFilter};
///
/// // Every rejection carries the reason, so callers can map it onto their own error surface.
/// let error = TransactionFilter::default()
///     .filter_transaction(&[])
///     .expect_err("an empty payload is never authorized");
///
/// assert_eq!(error, FilterError::Empty);
/// assert_eq!(error.to_string(), "transaction data cannot be empty");
/// ```
///
/// [`TransactionFilter`]: crate::TransactionFilter
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum FilterError {
    /// The raw transaction byte slice was empty.
    #[error("transaction data cannot be empty")]
    Empty,

    /// The raw transaction bytes could not be decoded into a known envelope.
    #[error("failed to decode transaction: {0}")]
    Decode(String),

    /// Bytes were left over after decoding a single transaction envelope.
    #[error("{0} trailing bytes after the transaction envelope")]
    TrailingBytes(usize),

    /// The transaction is of a type that the filter does not support.
    #[error("unsupported transaction type: only legacy and EIP-1559 transactions are supported")]
    UnsupportedType,

    /// The transaction creates a contract (it has no recipient) and is always rejected.
    #[error("contract creation transactions are not allowed")]
    ContractCreation,

    /// The sender address could not be recovered from the transaction signature.
    #[error("failed to recover sender address: {0}")]
    SenderRecovery(String),

    /// The calldata is too short to contain a 4-byte function selector.
    #[error("calldata is too short to contain a 4-byte function selector")]
    MissingSelector,

    /// A Safe-module `execTransactionFromModule` call could not be decoded.
    #[error("failed to decode Safe module call: {0}")]
    ModuleUnwrap(String),

    /// A Safe-module call requested a `DelegateCall` to anything but the canonical `MultiSend`
    /// singleton, or a `MultiSend` batch nested a delegate call of its own.
    #[error("Safe module delegate calls are only allowed into the canonical MultiSend contract")]
    DelegateCallNotAllowed,

    /// A `MultiSend` batch could not be decoded into a sequence of plain calls.
    #[error("failed to decode Safe MultiSend batch: {0}")]
    MultiSendDecode(String),

    /// The effective destination contract is not present in the allow-set at all.
    #[error("unauthorized: contract {contract} is not allowed")]
    ContractNotAllowed {
        /// Hex-encoded effective destination contract address.
        contract: String,
    },

    /// The contract is in the allow-set, but not with this selector.
    #[error("unauthorized: selector {selector} is not allowed on contract {contract}")]
    Unauthorized {
        /// Hex-encoded effective destination contract address.
        contract: String,
        /// Hex-encoded effective 4-byte function selector.
        selector: String,
    },
}
