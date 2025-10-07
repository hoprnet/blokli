//! Error types for the blokli API

use thiserror::Error;

/// API-related errors
#[derive(Debug, Error)]
pub enum ApiError {
    /// Server binding error
    #[error("Failed to bind server: {0}")]
    BindError(#[from] std::io::Error),

    /// Configuration error
    #[error("Configuration error: {0}")]
    ConfigError(String),

    /// Internal server error
    #[error("Internal server error: {0}")]
    InternalError(#[from] anyhow::Error),
}

/// Result type alias for API operations
pub type ApiResult<T> = Result<T, ApiError>;
