//! Error types and utilities for the blokli API
//!
//! This module provides:
//! - Server-level errors (ApiError enum) for infrastructure failures
//! - GraphQL error codes and message templates for API responses
//! - Builder functions for creating GraphQL error types with consistent formatting

use blokli_api_types::{
    ContractNotAllowedError, FunctionNotAllowedError, InvalidAddressError, InvalidTransactionIdError,
    MissingFilterError, QueryFailedError, RpcError, TimeoutError,
};
use thiserror::Error;

// ============================================================================
// Server-Level Errors
// ============================================================================

/// API-related errors for server infrastructure
#[derive(Debug, Error)]
pub enum ApiError {
    /// Server binding error
    #[error("Failed to bind server: {0}")]
    BindError(#[from] std::io::Error),

    /// Database error
    #[error("Database error: {0}")]
    DatabaseError(#[from] sea_orm::DbErr),

    /// Configuration error
    #[error("Configuration error: {0}")]
    ConfigError(String),

    /// Internal server error
    #[error("Internal server error: {0}")]
    InternalError(#[from] anyhow::Error),

    /// Generic box error
    #[error("Error: {0}")]
    BoxError(#[from] Box<dyn std::error::Error>),
}

/// Result type alias for API operations
pub type ApiResult<T> = Result<T, ApiError>;

// ============================================================================
// GraphQL Error Codes
// ============================================================================

/// Error codes for GraphQL API errors
///
/// All GraphQL error responses use these standardized codes for consistent
/// client-side error handling.
pub mod codes {
    /// Context retrieval errors (database connection, config, etc.)
    pub const CONTEXT_ERROR: &str = "CONTEXT_ERROR";

    /// Database query execution failures
    pub const QUERY_FAILED: &str = "QUERY_FAILED";

    /// Resource not found errors
    pub const NOT_FOUND: &str = "NOT_FOUND";

    /// Invalid address format errors
    pub const INVALID_ADDRESS: &str = "INVALID_ADDRESS";

    /// Type conversion errors (e.g., i64 to i32)
    pub const CONVERSION_ERROR: &str = "CONVERSION_ERROR";

    /// Numeric overflow errors
    pub const OVERFLOW: &str = "OVERFLOW";

    /// Feature not yet implemented
    pub const NOT_IMPLEMENTED: &str = "NOT_IMPLEMENTED";

    /// Transaction validation failures
    pub const VALIDATION_FAILED: &str = "VALIDATION_FAILED";

    /// Blockchain RPC operation errors
    pub const RPC_ERROR: &str = "RPC_ERROR";

    /// Generic internal server errors
    pub const INTERNAL_ERROR: &str = "INTERNAL_ERROR";

    /// Contract not in allowlist
    pub const CONTRACT_NOT_ALLOWED: &str = "CONTRACT_NOT_ALLOWED";

    /// Function selector not allowed
    pub const FUNCTION_NOT_ALLOWED: &str = "FUNCTION_NOT_ALLOWED";

    /// Operation timeout
    pub const TIMEOUT: &str = "TIMEOUT";

    /// Invalid transaction ID format
    pub const INVALID_TRANSACTION_ID: &str = "INVALID_TRANSACTION_ID";

    /// Missing required filter parameter
    pub const MISSING_FILTER: &str = "MISSING_FILTER";

    /// Malformed data in database
    pub const INVALID_DB_DATA: &str = "INVALID_DB_DATA";

    /// Invalid filter combination
    pub const INVALID_FILTER: &str = "INVALID_FILTER";

    /// Channel not found
    pub const CHANNEL_NOT_FOUND: &str = "CHANNEL_NOT_FOUND";

    /// Invalid pagination parameters
    pub const INVALID_PAGINATION: &str = "INVALID_PAGINATION";
}

// ============================================================================
// GraphQL Error Message Templates
// ============================================================================

/// Message templates for common error scenarios
///
/// These functions provide consistent error message formatting across the API.
pub mod messages {
    /// Context retrieval error message
    pub fn context_error(context_type: &str, error: impl std::fmt::Display) -> String {
        format!("Failed to get {} from context: {}", context_type, error)
    }

    /// Database connection error message
    pub fn db_connection_error(error: impl std::fmt::Display) -> String {
        format!("Failed to get database connection: {}", error)
    }

    /// Database query error message
    pub fn query_error(operation: &str, error: impl std::fmt::Display) -> String {
        format!("Database query failed during {}: {}", operation, error)
    }

    /// Resource not found error message
    pub fn not_found(resource_type: &str, identifier: impl std::fmt::Display) -> String {
        format!("{} not found: {}", resource_type, identifier)
    }

    /// Address validation error messages
    pub fn empty_address() -> String {
        "Address cannot be empty".to_string()
    }

    pub fn invalid_address(address: &str, error: impl std::fmt::Display) -> String {
        format!("Invalid address '{}': {}", address, error)
    }

    pub fn invalid_address_length(actual_length: usize) -> String {
        format!(
            "Invalid address: must be 40 hex characters (20 bytes), got {} characters",
            actual_length
        )
    }

    pub fn invalid_address_characters() -> String {
        "Invalid address: contains non-hexadecimal characters".to_string()
    }

    /// Type conversion error message
    pub fn conversion_error(from_type: &str, to_type: &str, value: impl std::fmt::Display) -> String {
        format!(
            "Cannot convert {} to {}: value {} out of range",
            from_type, to_type, value
        )
    }

    /// Numeric overflow error message
    pub fn overflow_error(operation: &str, value: impl std::fmt::Display) -> String {
        format!("Numeric overflow in {}: {}", operation, value)
    }

    /// Not implemented feature message
    pub fn not_implemented(feature: &str) -> String {
        format!("{} is not yet implemented", feature)
    }

    /// Transaction validation error message
    pub fn validation_failed(reason: &str) -> String {
        format!("Transaction validation failed: {}", reason)
    }

    /// RPC error message
    pub fn rpc_error(operation: &str, error: impl std::fmt::Display) -> String {
        format!("RPC error during {}: {}", operation, error)
    }

    /// Internal error message
    pub fn internal_error(context: &str, error: impl std::fmt::Display) -> String {
        format!("Internal error in {}: {}", context, error)
    }

    /// Missing filter error message
    pub fn missing_filter(filter_name: &str, context: &str) -> String {
        format!("Missing required filter '{}' for {}", filter_name, context)
    }

    /// Invalid database data error message
    pub fn invalid_db_data(field: &str, reason: &str) -> String {
        format!("Invalid data in database field '{}': {}", field, reason)
    }

    /// Invalid filter combination message
    pub fn invalid_filter(reason: &str) -> String {
        format!("Invalid filter combination: {}", reason)
    }

    /// Channel not found message
    pub fn channel_not_found(channel_id: impl std::fmt::Display) -> String {
        format!("Channel not found: {}", channel_id)
    }

    /// Invalid pagination parameters message
    pub fn invalid_pagination(reason: &str) -> String {
        format!("Invalid pagination parameters: {}", reason)
    }

    /// Ticket parameters missing or incomplete message
    pub fn ticket_params_incomplete() -> String {
        "Ticket parameters are not yet available".to_string()
    }

    /// Network status unavailable message
    pub fn network_status_unavailable() -> String {
        "Network status is not yet available".to_string()
    }
}

// ============================================================================
// GraphQL Error Builder Functions
// ============================================================================

/// Creates a QueryFailedError for context retrieval failures
pub fn context_error(context_type: &str, error: impl std::fmt::Display) -> QueryFailedError {
    QueryFailedError {
        code: codes::CONTEXT_ERROR.to_string(),
        message: messages::context_error(context_type, error),
    }
}

/// Creates a QueryFailedError for database connection failures
pub fn db_connection_error(error: impl std::fmt::Display) -> QueryFailedError {
    QueryFailedError {
        code: codes::CONTEXT_ERROR.to_string(),
        message: messages::db_connection_error(error),
    }
}

/// Creates a QueryFailedError for database query failures
pub fn query_failed(operation: &str, error: impl std::fmt::Display) -> QueryFailedError {
    QueryFailedError {
        code: codes::QUERY_FAILED.to_string(),
        message: messages::query_error(operation, error),
    }
}

/// Creates a QueryFailedError for resource not found errors
pub fn not_found(resource_type: &str, identifier: impl std::fmt::Display) -> QueryFailedError {
    QueryFailedError {
        code: codes::NOT_FOUND.to_string(),
        message: messages::not_found(resource_type, identifier),
    }
}

/// Creates an InvalidAddressError for empty addresses
pub fn empty_address_error() -> InvalidAddressError {
    InvalidAddressError {
        code: codes::INVALID_ADDRESS.to_string(),
        message: messages::empty_address(),
        address: String::new(),
    }
}

/// Creates an InvalidAddressError for invalid address format
pub fn invalid_address_error(address: impl Into<String>, error: impl std::fmt::Display) -> InvalidAddressError {
    let address_str = address.into();
    InvalidAddressError {
        code: codes::INVALID_ADDRESS.to_string(),
        message: messages::invalid_address(&address_str, error),
        address: address_str,
    }
}

/// Creates an InvalidAddressError from a validation error message
///
/// Use this when you have a validation error message string directly
pub fn invalid_address_from_message(address: impl Into<String>, message: String) -> InvalidAddressError {
    InvalidAddressError {
        code: codes::INVALID_ADDRESS.to_string(),
        message,
        address: address.into(),
    }
}

/// Creates a QueryFailedError for invalid address validation
///
/// Use this when the result type expects QueryFailedError instead of InvalidAddressError
pub fn invalid_address_query_failed(message: String) -> QueryFailedError {
    QueryFailedError {
        code: codes::INVALID_ADDRESS.to_string(),
        message,
    }
}

/// Creates a QueryFailedError for type conversion failures
pub fn conversion_error(from_type: &str, to_type: &str, value: impl std::fmt::Display) -> QueryFailedError {
    QueryFailedError {
        code: codes::CONVERSION_ERROR.to_string(),
        message: messages::conversion_error(from_type, to_type, value),
    }
}

/// Creates a QueryFailedError for numeric overflow
pub fn overflow_error(operation: &str, value: impl std::fmt::Display) -> QueryFailedError {
    QueryFailedError {
        code: codes::OVERFLOW.to_string(),
        message: messages::overflow_error(operation, value),
    }
}

/// Creates a QueryFailedError for not implemented features
pub fn not_implemented(feature: &str) -> QueryFailedError {
    QueryFailedError {
        code: codes::NOT_IMPLEMENTED.to_string(),
        message: messages::not_implemented(feature),
    }
}

/// Creates a QueryFailedError for transaction validation failures
pub fn validation_failed(reason: &str) -> QueryFailedError {
    QueryFailedError {
        code: codes::VALIDATION_FAILED.to_string(),
        message: messages::validation_failed(reason),
    }
}

/// Creates an RpcError for blockchain RPC operation failures
pub fn rpc_error(operation: &str, error: impl std::fmt::Display) -> RpcError {
    RpcError {
        code: codes::RPC_ERROR.to_string(),
        message: messages::rpc_error(operation, error),
    }
}

/// Creates a QueryFailedError for blockchain RPC operation failures
///
/// Use this variant when the result type expects QueryFailedError instead of RpcError
pub fn rpc_query_failed(operation: &str, error: impl std::fmt::Display) -> QueryFailedError {
    QueryFailedError {
        code: codes::RPC_ERROR.to_string(),
        message: messages::rpc_error(operation, error),
    }
}

/// Creates a QueryFailedError for internal server errors
pub fn internal_error(context: &str, error: impl std::fmt::Display) -> QueryFailedError {
    QueryFailedError {
        code: codes::INTERNAL_ERROR.to_string(),
        message: messages::internal_error(context, error),
    }
}

/// Creates a MissingFilterError for missing required filters
pub fn missing_filter_error(filter_name: &str, context: &str) -> MissingFilterError {
    MissingFilterError {
        code: codes::MISSING_FILTER.to_string(),
        message: messages::missing_filter(filter_name, context),
    }
}

/// Creates a QueryFailedError for invalid database data
pub fn invalid_db_data(field: &str, reason: &str) -> QueryFailedError {
    QueryFailedError {
        code: codes::INVALID_DB_DATA.to_string(),
        message: messages::invalid_db_data(field, reason),
    }
}

/// Creates a QueryFailedError for invalid filter combinations
pub fn invalid_filter(reason: &str) -> QueryFailedError {
    QueryFailedError {
        code: codes::INVALID_FILTER.to_string(),
        message: messages::invalid_filter(reason),
    }
}

/// Creates a QueryFailedError for channel not found
pub fn channel_not_found(channel_id: impl std::fmt::Display) -> QueryFailedError {
    QueryFailedError {
        code: codes::CHANNEL_NOT_FOUND.to_string(),
        message: messages::channel_not_found(channel_id),
    }
}

/// Creates a QueryFailedError for invalid pagination parameters
pub fn invalid_pagination(reason: &str) -> QueryFailedError {
    QueryFailedError {
        code: codes::INVALID_PAGINATION.to_string(),
        message: messages::invalid_pagination(reason),
    }
}

/// Creates an InvalidTransactionIdError for invalid transaction ID format
pub fn invalid_transaction_id(transaction_id: impl Into<String>) -> InvalidTransactionIdError {
    let tx_id = transaction_id.into();
    InvalidTransactionIdError {
        code: codes::INVALID_TRANSACTION_ID.to_string(),
        message: format!("Invalid transaction ID format: {}", tx_id),
        transaction_id: tx_id,
    }
}

/// Creates a QueryFailedError for incomplete ticket parameters
pub fn ticket_params_incomplete() -> QueryFailedError {
    QueryFailedError {
        code: codes::NOT_FOUND.to_string(),
        message: messages::ticket_params_incomplete(),
    }
}

/// Creates a QueryFailedError for unavailable network status
pub fn network_status_unavailable() -> QueryFailedError {
    QueryFailedError {
        code: codes::NOT_FOUND.to_string(),
        message: messages::network_status_unavailable(),
    }
}

/// Converts an async_graphql::Error to a QueryFailedError
///
/// This is useful when wrapping errors from async-graphql operations
pub fn from_graphql_error(error: async_graphql::Error) -> QueryFailedError {
    QueryFailedError {
        code: codes::INTERNAL_ERROR.to_string(),
        message: error.message,
    }
}

/// Creates a ContractNotAllowedError for contract allowlist validation failures
pub fn contract_not_allowed(contract_address: impl Into<String>) -> ContractNotAllowedError {
    let address = contract_address.into();
    ContractNotAllowedError {
        code: codes::CONTRACT_NOT_ALLOWED.to_string(),
        message: format!("Contract not allowed: {}", address),
        contract_address: address,
    }
}

/// Creates a FunctionNotAllowedError for function selector validation failures
pub fn function_not_allowed(
    contract_address: impl Into<String>,
    function_selector: impl Into<String>,
) -> FunctionNotAllowedError {
    let address = contract_address.into();
    let selector = function_selector.into();
    FunctionNotAllowedError {
        code: codes::FUNCTION_NOT_ALLOWED.to_string(),
        message: format!("Function not allowed: contract={}, selector={}", address, selector),
        contract_address: address,
        function_selector: selector,
    }
}

/// Creates a TimeoutError for operation timeout failures
pub fn timeout_error(message: impl Into<String>) -> TimeoutError {
    TimeoutError {
        code: codes::TIMEOUT.to_string(),
        message: message.into(),
    }
}

/// Creates an RpcError for validation failures
pub fn rpc_validation_failed(error: impl std::fmt::Display) -> RpcError {
    RpcError {
        code: codes::VALIDATION_FAILED.to_string(),
        message: error.to_string(),
    }
}

/// Creates an RpcError for RPC operation failures with custom message
pub fn rpc_error_with_message(message: impl Into<String>) -> RpcError {
    RpcError {
        code: codes::RPC_ERROR.to_string(),
        message: message.into(),
    }
}

/// Creates an RpcError for internal errors
pub fn rpc_internal_error(error: impl std::fmt::Display) -> RpcError {
    RpcError {
        code: codes::INTERNAL_ERROR.to_string(),
        message: error.to_string(),
    }
}
