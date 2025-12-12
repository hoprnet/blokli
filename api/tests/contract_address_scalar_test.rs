use std::collections::HashMap;

use async_graphql::ScalarType;
use blokli_api_types::ContractAddressMap;
use blokli_chain_types::ContractAddresses;

#[test]
fn test_contract_address_map_serializes_as_json_string() {
    // Create a ContractAddressMap with test data
    let mut map = HashMap::new();
    map.insert("token".to_string(), "0xaabbccdd".to_string());
    map.insert("channels".to_string(), "0x11223344".to_string());
    map.insert("announcements".to_string(), "0x55667788".to_string());

    let contract_map = ContractAddressMap(map);

    // Serialize it
    let serialized = contract_map.to_value();

    // Verify it's a string
    assert!(
        matches!(serialized, async_graphql::Value::String(_)),
        "Should serialize to string, got: {:?}",
        serialized
    );

    // Extract and validate the JSON string
    if let async_graphql::Value::String(json_str) = serialized {
        // Verify it contains valid JSON
        let parsed: serde_json::Value =
            serde_json::from_str(&json_str).expect("contractAddresses should be valid JSON string");

        // Verify it's an object with expected keys
        assert!(parsed.is_object(), "Parsed value should be a JSON object");

        let obj = parsed.as_object().expect("Should be able to parse as object");

        assert_eq!(obj.get("token").and_then(|v| v.as_str()), Some("0xaabbccdd"));
        assert_eq!(obj.get("channels").and_then(|v| v.as_str()), Some("0x11223344"));
        assert_eq!(obj.get("announcements").and_then(|v| v.as_str()), Some("0x55667788"));
    } else {
        panic!("Expected string value");
    }
}

#[test]
fn test_contract_address_map_roundtrip_serialization() {
    // Create a test map
    let mut test_map = HashMap::new();
    test_map.insert("token".to_string(), "0xaabbccdd".to_string());
    test_map.insert("channels".to_string(), "0x11223344".to_string());

    let original = ContractAddressMap(test_map);

    // Serialize it
    let serialized = original.to_value();

    // Verify it's a string
    assert!(
        matches!(serialized, async_graphql::Value::String(_)),
        "Should serialize to string"
    );

    // Parse it back
    let parsed =
        <ContractAddressMap as async_graphql::ScalarType>::parse(serialized).expect("Should parse back successfully");

    // Verify they match
    assert_eq!(original.0, parsed.0, "Should match after roundtrip");
}

#[test]
fn test_contract_address_map_from_chain_types() {
    // Create a ContractAddresses instance
    let chain_addresses = ContractAddresses::default();

    // Convert to ContractAddressMap
    let map: ContractAddressMap = (&chain_addresses).into();

    // Serialize it
    let serialized = map.to_value();

    // Verify it's a string
    assert!(
        matches!(serialized, async_graphql::Value::String(_)),
        "Should serialize to string"
    );

    // Extract and validate the JSON string
    if let async_graphql::Value::String(json_str) = serialized {
        let parsed: serde_json::Value = serde_json::from_str(&json_str).expect("Should be valid JSON");

        let obj = parsed.as_object().expect("Should be an object");

        // Verify all required keys are present
        let expected_keys = vec![
            "token",
            "channels",
            "announcements",
            "module_implementation",
            "node_safe_migration",
            "node_safe_registry",
            "ticket_price_oracle",
            "winning_probability_oracle",
            "node_stake_v2_factory",
        ];

        for key in expected_keys {
            assert!(obj.contains_key(key), "contractAddresses should contain key: {}", key);
            let value = obj.get(key).and_then(|v| v.as_str());
            assert!(
                !value.unwrap_or("").is_empty(),
                "contractAddresses[{}] should not be empty",
                key
            );
        }
    }
}
