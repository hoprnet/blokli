//! Checked numeric conversions for database boundaries.
//!
//! The database stores unsigned chain values (block numbers, log positions, block
//! ranges) in signed `i64`/`i32` columns. These helpers normalize every overflow or
//! sign mismatch into [`DbSqlError::InvalidData`] so callers surface a structured
//! error instead of silently truncating or wrapping values.
use crate::errors::{DbSqlError, Result};

/// Converts a `u64` into `i64`, returning [`DbSqlError::InvalidData`] on overflow
/// (i.e. when `value` exceeds `i64::MAX`).
pub(crate) fn u64_to_i64(value: u64, field_name: &str) -> Result<i64> {
    i64::try_from(value).map_err(|_| DbSqlError::InvalidData(format!("{field_name} {value} exceeds i64::MAX")))
}

/// Converts a signed `i64` column value back into `u64`, returning
/// [`DbSqlError::InvalidData`] when the stored value is negative.
pub(crate) fn i64_to_u64(value: i64, field_name: &str) -> Result<u64> {
    u64::try_from(value).map_err(|_| DbSqlError::InvalidData(format!("{field_name} {value} is negative")))
}

/// Converts a signed `i64` column value into `u32`, returning
/// [`DbSqlError::InvalidData`] when the stored value is negative or exceeds `u32::MAX`.
pub(crate) fn i64_to_u32(value: i64, field_name: &str) -> Result<u32> {
    u32::try_from(value)
        .map_err(|_| DbSqlError::InvalidData(format!("{field_name} {value} is negative or exceeds u32::MAX")))
}

/// Converts the `(block_number, tx_index, log_index)` log position triple into `i64`
/// values for the database. Any component that does not fit `i64` yields
/// [`DbSqlError::InvalidData`].
pub(crate) fn log_position_to_i64(block_number: u64, tx_index: u64, log_index: u64) -> Result<(i64, i64, i64)> {
    Ok((
        u64_to_i64(block_number, "block_number")?,
        u64_to_i64(tx_index, "tx_index")?,
        u64_to_i64(log_index, "log_index")?,
    ))
}

/// Converts an optional inclusive start block and optional offset into the `(i64,
/// Option<i64>)` range pair used by log queries.
///
/// `block_number` is the inclusive lower bound (defaults to `0` when `None`). When
/// `block_offset` is `Some(offset)`, the returned upper bound is the **exclusive**
/// end computed as `block_number + offset + 1`. Overflow of either the `u64` sum or
/// the subsequent `i64` conversion is reported via [`DbSqlError::InvalidData`].
pub(crate) fn block_range_to_i64(block_number: Option<u64>, block_offset: Option<u64>) -> Result<(i64, Option<i64>)> {
    let min_block_number = block_number.unwrap_or(0);
    let max_block_number = block_offset
        .map(|offset| {
            min_block_number
                .checked_add(offset)
                .and_then(|value| value.checked_add(1))
                .ok_or_else(|| {
                    DbSqlError::InvalidData(format!(
                        "block range starting at {min_block_number} with offset {offset} overflows u64"
                    ))
                })
        })
        .transpose()?;

    Ok((
        u64_to_i64(min_block_number, "block_number")?,
        max_block_number
            .map(|value| u64_to_i64(value, "block_range_end"))
            .transpose()?,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn u64_to_i64_rejects_values_over_i64_max() {
        assert_eq!(u64_to_i64(i64::MAX as u64, "field").unwrap(), i64::MAX);
        let err = u64_to_i64(i64::MAX as u64 + 1, "ticket_index").expect_err("value over i64::MAX should be rejected");
        assert!(matches!(err, DbSqlError::InvalidData(msg) if msg.contains("ticket_index")));
    }

    #[test]
    fn i64_to_u64_rejects_negative_values() {
        assert_eq!(i64_to_u64(0, "field").unwrap(), 0);
        let err = i64_to_u64(-1, "block_number").expect_err("negative value should be rejected");
        assert!(matches!(err, DbSqlError::InvalidData(msg) if msg.contains("negative")));
    }

    #[test]
    fn i64_to_u32_rejects_out_of_range_values() {
        assert_eq!(i64_to_u32(i64::from(u32::MAX), "field").unwrap(), u32::MAX);
        assert!(i64_to_u32(-1, "field").is_err());
        assert!(i64_to_u32(i64::from(u32::MAX) + 1, "field").is_err());
    }

    #[test]
    fn block_range_to_i64_computes_exclusive_upper_bound() {
        assert_eq!(block_range_to_i64(Some(10), Some(5)).unwrap(), (10, Some(16)));
        assert_eq!(block_range_to_i64(None, None).unwrap(), (0, None));
    }

    #[test]
    fn block_range_to_i64_rejects_u64_overflow() {
        assert!(block_range_to_i64(Some(u64::MAX), Some(1)).is_err());
    }
}
