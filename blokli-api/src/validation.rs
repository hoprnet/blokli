//! Input validation utilities for the GraphQL API

use async_graphql::Error;

/// Validate an Ethereum address format
///
/// Ensures the address:
/// - Starts with "0x" prefix
/// - Has exactly 42 characters (0x + 40 hex digits)
/// - Contains only valid hexadecimal characters
///
/// # Arguments
/// * `address` - The address string to validate
///
/// # Returns
/// * `Result<(), Error>` - Ok if valid, Error with message if invalid
pub fn validate_eth_address(address: &str) -> Result<(), Error> {
    if address.is_empty() {
        return Err(Error::new("Address cannot be empty"));
    }

    if !address.starts_with("0x") {
        return Err(Error::new("Invalid address format: must start with '0x' prefix"));
    }

    if address.len() != 42 {
        return Err(Error::new(format!(
            "Invalid address length: expected 42 characters, got {}",
            address.len()
        )));
    }

    if !address[2..].chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(Error::new(
            "Invalid address format: contains non-hexadecimal characters",
        ));
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
    fn test_invalid_no_prefix() {
        let addr = "1234567890123456789012345678901234567890";
        assert!(validate_eth_address(addr).is_err());
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
}
