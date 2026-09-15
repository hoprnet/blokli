//! Checked numeric conversions for indexer chain-data boundaries.
//!
//! On-chain values arrive as `u64`/`U256`, but the database and downstream
//! position/account models use narrower signed types. These helpers normalize
//! every overflow or width mismatch into a [`CoreEthereumIndexerError::ProcessError`]
//! so a single malformed event is logged and skipped during block processing rather
//! than being silently truncated or wrapped.
use hopr_types::primitive::prelude::U256;

use crate::errors::{CoreEthereumIndexerError, Result};

/// Converts a `u64` chain value into `u32`, returning
/// [`CoreEthereumIndexerError::ProcessError`] when it exceeds `u32::MAX`.
pub(crate) fn u64_to_u32(value: u64, field_name: &str) -> Result<u32> {
    u32::try_from(value)
        .map_err(|_| CoreEthereumIndexerError::ProcessError(format!("{field_name} {value} does not fit into u32")))
}

/// Converts a `u64` chain value into `i64`, returning
/// [`CoreEthereumIndexerError::ProcessError`] when it exceeds `i64::MAX`.
pub(crate) fn u64_to_i64(value: u64, field_name: &str) -> Result<i64> {
    i64::try_from(value)
        .map_err(|_| CoreEthereumIndexerError::ProcessError(format!("{field_name} {value} does not fit into i64")))
}

/// Converts a `U256` chain value into `u64`, returning
/// [`CoreEthereumIndexerError::ProcessError`] when it does not fit into 64 bits.
pub(crate) fn u256_to_u64(value: U256, field_name: &str) -> Result<u64> {
    if value.bits() > 64 {
        return Err(CoreEthereumIndexerError::ProcessError(format!(
            "{field_name} {value} does not fit into u64"
        )));
    }

    Ok(value.low_u64())
}

/// Converts a `U256` chain value into `u32`, returning
/// [`CoreEthereumIndexerError::ProcessError`] when it does not fit into 32 bits.
pub(crate) fn u256_to_u32(value: U256, field_name: &str) -> Result<u32> {
    if value.bits() > 32 {
        return Err(CoreEthereumIndexerError::ProcessError(format!(
            "{field_name} {value} does not fit into u32"
        )));
    }

    Ok(value.low_u32())
}
