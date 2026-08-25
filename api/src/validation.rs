//! Input validation utilities for the GraphQL API

use std::str::FromStr;

use async_graphql::Error;
use hopr_types::{internal::prelude::ServiceType, primitive::traits::ToHex};

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

/// Parse a service type identifier from its GraphQL representation
///
/// Accepts the right-padded ASCII name the registry convention uses, such as `gvpn:exit`, and
/// falls back to the raw `0x`-prefixed 32-byte id for any type that does not follow it. The
/// contract does not enforce the convention, so both forms must be accepted.
///
/// # Arguments
/// * `service_type` - The service type identifier, as an ASCII name or as `0x`-prefixed hex
///
/// # Returns
/// * `Result<ServiceType, Error>` - The parsed identifier, or an error naming the rejected input
pub fn parse_service_type(service_type: &str) -> Result<ServiceType, Error> {
    let parsed = if service_type.starts_with("0x") || service_type.starts_with("0X") {
        ServiceType::from_hex(service_type)
    } else {
        ServiceType::from_str(service_type)
    };

    parsed.map_err(|e| Error::new(errors::messages::invalid_service_type(service_type, e)))
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

    #[test]
    fn test_parse_service_type_accepts_ascii_name() {
        assert_eq!(parse_service_type("gvpn:exit").unwrap(), ServiceType::GVPN_EXIT);
    }

    /// The right-padded hex of a name and the name itself must parse to the same identifier, so
    /// that a filter given either way selects the same entries.
    #[test]
    fn test_parse_service_type_accepts_hex_of_the_same_name() {
        let hex = "0x6776706e3a657869740000000000000000000000000000000000000000000000";
        assert_eq!(parse_service_type(hex).unwrap(), ServiceType::GVPN_EXIT);
    }

    /// The contract rejects the zero id, so a filter carrying it can never match anything.
    #[test]
    fn test_parse_service_type_rejects_zero() {
        let zero = "0x0000000000000000000000000000000000000000000000000000000000000000";
        assert!(parse_service_type(zero).is_err());
    }

    #[test]
    fn test_parse_service_type_rejects_empty_name() {
        assert!(parse_service_type("").is_err());
    }

    #[test]
    fn test_parse_service_type_rejects_name_above_32_bytes() {
        assert!(parse_service_type(&"a".repeat(33)).is_err());
    }

    /// Space is excluded along with the control characters: it is indistinguishable from the
    /// right padding to the eye.
    #[test]
    fn test_parse_service_type_rejects_non_graphic_name() {
        assert!(parse_service_type("gvpn exit").is_err());
        assert!(parse_service_type("gvpn:éxit").is_err());
    }

    #[test]
    fn test_parse_service_type_rejects_hex_of_the_wrong_length() {
        assert!(parse_service_type("0xdeadbeef").is_err());
    }
}
