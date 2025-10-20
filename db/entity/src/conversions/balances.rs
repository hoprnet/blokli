//! Balance conversion utilities for HOPR blokli

use hopr_primitive_types::prelude::{Balance, Currency, HoprBalance, IntoEndian, XDaiBalance};

/// Convert binary address (20 bytes) to hex string (without 0x prefix)
///
/// # Arguments
/// * `address` - A slice containing the 20-byte address
///
/// # Returns
/// * `String` - The address as a hexadecimal string (40 characters), or empty string if invalid
pub fn address_to_string(address: &[u8]) -> String {
    if address.len() == 20 {
        hex::encode(address)
    } else {
        String::new()
    }
}

/// Convert hex string address to binary (20 bytes)
///
/// # Arguments
/// * `address` - A hex string (with or without 0x prefix)
///
/// # Returns
/// * `Vec<u8>` - The address as 20 bytes, or empty vector if invalid
pub fn string_to_address(address: &str) -> Vec<u8> {
    let addr = address.strip_prefix("0x").unwrap_or(address);
    hex::decode(addr).unwrap_or_default()
}

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

/// Convert a 12-byte balance representation to String (human-readable format for HOPR)
///
/// This function converts the 12-byte big-endian balance stored in the database
/// to a human-readable string representation using the HoprBalance type.
/// The output format is like "10 wxHOPR" or "10.5 wxHOPR".
///
/// The 12-byte balance is padded to 32 bytes before conversion to Balance type.
///
/// # Arguments
/// * `balance` - A slice containing the 12-byte balance
///
/// # Returns
/// * `String` - The balance as a human-readable string (e.g., "10 wxHOPR"), or "0 wxHOPR" if the slice is not 12 bytes
pub fn balance_to_string(balance: &[u8]) -> String {
    hopr_balance_to_string(balance)
}

/// Convert a 12-byte HOPR balance to human-readable string
pub fn hopr_balance_to_string(balance: &[u8]) -> String {
    if balance.len() == 12 {
        // Pad 12 bytes to 32 bytes for Balance type
        let mut bytes = [0u8; 32];
        bytes[20..].copy_from_slice(balance);
        let hopr_balance = HoprBalance::from_be_bytes(&bytes);
        hopr_balance.to_string()
    } else {
        HoprBalance::zero().to_string()
    }
}

/// Convert a 12-byte native balance to human-readable string
pub fn native_balance_to_string(balance: &[u8]) -> String {
    if balance.len() == 12 {
        // Pad 12 bytes to 32 bytes for Balance type
        let mut bytes = [0u8; 32];
        bytes[20..].copy_from_slice(balance);
        let native_balance = XDaiBalance::from_be_bytes(&bytes);
        native_balance.to_string()
    } else {
        XDaiBalance::zero().to_string()
    }
}

/// Convert a 12-byte balance to a generic Balance type
///
/// This function converts the 12-byte big-endian balance to a Balance<C> type.
/// The 12-byte balance is padded to 32 bytes for proper conversion.
///
/// # Arguments
/// * `balance` - A slice containing the 12-byte balance
///
/// # Returns
/// * `Balance<C>` - The balance as a Balance type, or zero if the slice is not 12 bytes
pub fn balance_to_type<C: Currency>(balance: &[u8]) -> Balance<C> {
    if balance.len() == 12 {
        // Pad 12 bytes to 32 bytes for Balance type
        let mut bytes = [0u8; 32];
        bytes[20..].copy_from_slice(balance);
        Balance::<C>::from_be_bytes(&bytes)
    } else {
        Balance::<C>::zero()
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

    #[test]
    fn test_balance_to_string_valid() {
        let balance = vec![0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x03, 0xE8];
        assert_eq!(balance_to_string(&balance), "1000");
    }

    #[test]
    fn test_balance_to_string_large_value() {
        // Test with a large value: 1000000000000000000 (1 ETH in wei)
        let balance = vec![0x00, 0x00, 0x00, 0x00, 0x0D, 0xE0, 0xB6, 0xB3, 0xA7, 0x64, 0x00, 0x00];
        assert_eq!(balance_to_string(&balance), "1000000000000000000");
    }

    #[test]
    fn test_balance_to_string_invalid_length() {
        let balance = vec![0x00, 0x00];
        assert_eq!(balance_to_string(&balance), "0");
    }
}
