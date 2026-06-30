//! Error types for the transaction filter.

use thiserror::Error;

/// Result type for transaction filtering operations.
pub type Result<T, E = FilterError> = core::result::Result<T, E>;

/// Reasons a signed transaction can be rejected by [`TransactionFilter`].
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

    /// A Safe-module call requested a `DelegateCall`, which is never allowed.
    #[error("Safe module delegate calls are not allowed")]
    DelegateCallNotAllowed,

    /// The `(contract, selector)` pair is not present in the allow-set.
    #[error("unauthorized: selector {selector} is not allowed on contract {contract}")]
    Unauthorized {
        /// Hex-encoded effective destination contract address.
        contract: String,
        /// Hex-encoded effective 4-byte function selector.
        selector: String,
    },
}
