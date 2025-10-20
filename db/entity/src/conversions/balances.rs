//! Balance conversion utilities for HOPR blokli

/// Convert a 12-byte balance representation to f64
///
/// This function converts the 12-byte big-endian balance stored in the database
/// to a floating-point representation. The balance is first converted to u128
/// by padding with 4 zero bytes, then cast to f64.
///
/// # Arguments
/// * `balance` - A slice containing the 12-byte balance
///
/// # Returns
/// * `f64` - The balance as a floating-point number, or 0.0 if the slice is not 12 bytes
pub fn balance_to_f64(balance: &[u8]) -> f64 {
    if balance.len() == 12 {
        let mut bytes = [0u8; 16];
        bytes[4..].copy_from_slice(balance);
        u128::from_be_bytes(bytes) as f64
    } else {
        0.0
    }
}

/// Convert an 8-byte value to u64
///
/// This function converts an 8-byte big-endian value to u64.
///
/// # Arguments
/// * `value` - A slice containing the 8-byte value
///
/// # Returns
/// * `u64` - The value as u64, or 0 if the slice is not 8 bytes
pub fn bytes_to_u64(value: &[u8]) -> u64 {
    if value.len() == 8 {
        let bytes: [u8; 8] = value.try_into().unwrap_or([0u8; 8]);
        u64::from_be_bytes(bytes)
    } else {
        0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_balance_to_f64_valid() {
        let balance = vec![0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x03, 0xE8];
        assert_eq!(balance_to_f64(&balance), 1000.0);
    }

    #[test]
    fn test_balance_to_f64_invalid_length() {
        let balance = vec![0x00, 0x00];
        assert_eq!(balance_to_f64(&balance), 0.0);
    }

    #[test]
    fn test_bytes_to_u64_valid() {
        let value = vec![0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x03, 0xE8];
        assert_eq!(bytes_to_u64(&value), 1000);
    }

    #[test]
    fn test_bytes_to_u64_invalid_length() {
        let value = vec![0x00, 0x00];
        assert_eq!(bytes_to_u64(&value), 0);
    }
}
