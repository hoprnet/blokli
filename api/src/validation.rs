//! Input validation utilities for the GraphQL API

use async_graphql::Error;

use crate::errors;

/// Validate an Ethereum address format
///
/// Ensures the address:
/// - Contains exactly 40 hex characters (20 bytes) after optional "0x" prefix
/// - Contains only valid hexadecimal characters
///
/// # Arguments
/// * `address` - The address string to validate (with or without 0x prefix)
///
/// # Returns
/// * `Result<(), Error>` - Ok if valid, Error with message if invalid
pub fn validate_eth_address(address: &str) -> Result<(), Error> {
    if address.is_empty() {
        return Err(Error::new(errors::messages::empty_address()));
    }

    let hex_part = address.strip_prefix("0x").unwrap_or(address);

    if hex_part.len() != 40 {
        return Err(Error::new(errors::messages::invalid_address_length(hex_part.len())));
    }

    if !hex_part.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(Error::new(errors::messages::invalid_address_characters()));
    }

    Ok(())
}

/// Validate an Ethereum address and convert to lowercase
///
/// This is useful for case-insensitive comparisons in the database.
///
/// # Arguments
/// * `address` - The address string to validate and normalize
///
/// # Returns
/// * `Result<String, Error>` - Normalized address if valid, Error if invalid
pub fn validate_and_normalize_address(address: &str) -> Result<String, Error> {
    validate_eth_address(address)?;
    Ok(address.to_lowercase())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_address() {
        let addr = "0x1234567890123456789012345678901234567890";
        assert!(validate_eth_address(addr).is_ok());
    }

    #[test]
    fn test_valid_address_mixed_case() {
        let addr = "0xAbCdEf1234567890123456789012345678901234";
        assert!(validate_eth_address(addr).is_ok());
    }

    #[test]
    fn test_valid_no_prefix() {
        let addr = "1234567890123456789012345678901234567890";
        assert!(validate_eth_address(addr).is_ok());
    }

    #[test]
    fn test_invalid_length_short() {
        let addr = "0x123456";
        assert!(validate_eth_address(addr).is_err());
    }

    #[test]
    fn test_invalid_length_long() {
        let addr = "0x12345678901234567890123456789012345678901234";
        assert!(validate_eth_address(addr).is_err());
    }

    #[test]
    fn test_invalid_characters() {
        let addr = "0xGHIJ567890123456789012345678901234567890";
        assert!(validate_eth_address(addr).is_err());
    }

    #[test]
    fn test_empty_address() {
        assert!(validate_eth_address("").is_err());
    }

    #[test]
    fn test_normalize_address() {
        let addr = "0xAbCdEf1234567890123456789012345678901234";
        let normalized = validate_and_normalize_address(addr).unwrap();
        assert_eq!(normalized, "0xabcdef1234567890123456789012345678901234");
    }

    #[test]
    fn test_validate_eth_address_without_prefix() {
        let addr = "1234567890123456789012345678901234567890";
        assert!(validate_eth_address(addr).is_ok());
    }
}
