//! Transaction validator for validating raw transactions before submission
//!
//! This module provides basic validation for raw transactions. The validation is currently
//! stubbed and only checks for empty transactions. Future implementations can add more
//! sophisticated validation such as allowlist checking, gas limit validation, etc.

use thiserror::Error;

/// Errors that can occur during transaction validation
#[derive(Error, Debug, Clone, PartialEq)]
pub enum ValidationError {
    #[error("Transaction data cannot be empty")]
    EmptyTransaction,
    #[error("Invalid transaction format: {0}")]
    InvalidFormat(String),
    #[error("Contract not allowed: {0}")]
    ContractNotAllowed(String),
    #[error("Function not allowed: contract={0}, selector={1}")]
    FunctionNotAllowed(String, String),
}

/// Validator for raw transactions
///
/// Currently provides stub validation that only checks for empty transactions.
/// Future implementations can add:
/// - Allowlist validation for contracts
/// - Allowlist validation for function selectors
/// - Gas limit validation
/// - Nonce validation
/// - Signature validation
#[derive(Debug, Clone, Default)]
pub struct TransactionValidator {
    // TODO: Add configuration for allowlists
    // allowed_contracts: HashSet<Address>,
    // allowed_functions: HashMap<Address, HashSet<[u8; 4]>>,
}

impl TransactionValidator {
    /// Create a new transaction validator
    pub fn new() -> Self {
        Self::default()
    }

    /// Validate a raw transaction
    ///
    /// Currently only checks that the transaction is not empty.
    /// Future implementations will add more comprehensive validation.
    ///
    /// # Examples
    ///
    /// ```
    /// use blokli_chain_api::transaction_validator::{TransactionValidator, ValidationError};
    ///
    /// let validator = TransactionValidator::new();
    ///
    /// // Valid transaction with non-empty data
    /// let valid_tx = vec![0x01, 0x02, 0x03];
    /// assert!(validator.validate_raw_transaction(&valid_tx).is_ok());
    ///
    /// // Invalid empty transaction
    /// let empty_tx: &[u8] = &[];
    /// let result = validator.validate_raw_transaction(empty_tx);
    /// assert_eq!(result, Err(ValidationError::EmptyTransaction));
    /// ```
    ///
    /// # Errors
    /// Returns `ValidationError::EmptyTransaction` if the transaction data is empty
    pub fn validate_raw_transaction(&self, raw_tx: &[u8]) -> Result<(), ValidationError> {
        if raw_tx.is_empty() {
            return Err(ValidationError::EmptyTransaction);
        }

        // TODO: Add real validation logic
        // - Decode transaction to extract target contract and function selector
        // - Check against allowlist
        // - Validate gas parameters
        // - etc.

        Ok(())
    }

    /// Stub for future allowlist validation
    ///
    /// This method is a placeholder for future implementation of contract allowlist validation.
    #[allow(dead_code)]
    fn validate_contract_allowlist(&self, _contract_address: &str) -> Result<(), ValidationError> {
        // TODO: Implement contract allowlist validation
        Ok(())
    }

    /// Stub for future function allowlist validation
    ///
    /// This method is a placeholder for future implementation of function allowlist validation.
    #[allow(dead_code)]
    fn validate_function_allowlist(
        &self,
        _contract_address: &str,
        _function_selector: &[u8; 4],
    ) -> Result<(), ValidationError> {
        // TODO: Implement function allowlist validation
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_reject_empty_transaction() {
        let validator = TransactionValidator::new();
        let empty_tx: Vec<u8> = vec![];

        let result = validator.validate_raw_transaction(&empty_tx);
        assert!(matches!(result, Err(ValidationError::EmptyTransaction)));
    }

    #[test]
    fn test_accept_nonempty_transaction() {
        let validator = TransactionValidator::new();
        let tx = vec![0x01, 0x02, 0x03, 0x04];

        let result = validator.validate_raw_transaction(&tx);
        assert!(result.is_ok());
    }

    #[test]
    fn test_accept_minimal_transaction() {
        let validator = TransactionValidator::new();
        let tx = vec![0x00]; // Single byte

        let result = validator.validate_raw_transaction(&tx);
        assert!(result.is_ok());
    }

    #[test]
    fn test_accept_large_transaction() {
        let validator = TransactionValidator::new();
        let tx = vec![0xff; 1000]; // 1000 bytes

        let result = validator.validate_raw_transaction(&tx);
        assert!(result.is_ok());
    }

    #[test]
    fn test_validator_default() {
        let validator = TransactionValidator::default();
        let tx = vec![0x01];

        let result = validator.validate_raw_transaction(&tx);
        assert!(result.is_ok());
    }

    #[test]
    fn test_validator_is_cloneable() {
        let validator = TransactionValidator::new();
        let cloned = validator.clone();

        let tx = vec![0x01, 0x02];
        assert!(validator.validate_raw_transaction(&tx).is_ok());
        assert!(cloned.validate_raw_transaction(&tx).is_ok());
    }
}
