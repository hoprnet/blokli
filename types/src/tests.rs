#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use async_graphql::{ScalarType, Value};

    use crate::ContractAddressMap;

    #[test]
    fn test_contract_address_map_serializes_to_json_string() {
        // Create a ContractAddressMap with test data
        let mut map = HashMap::new();
        map.insert("token".to_string(), "0x123abc".to_string());
        map.insert("channels".to_string(), "0x456def".to_string());
        map.insert("announcements".to_string(), "0x789ghi".to_string());

        let contract_map = ContractAddressMap(map);

        // Convert to GraphQL value
        let value = contract_map.to_value();

        // Verify it's a string
        assert!(
            matches!(value, Value::String(_)),
            "ContractAddressMap should serialize to a string, got {:?}",
            value
        );

        // Extract the string and verify it's valid JSON
        if let Value::String(json_str) = value {
            let parsed: serde_json::Value = serde_json::from_str(&json_str).expect("Should be valid JSON string");

            // Verify structure
            assert!(parsed.is_object(), "Stringified value should parse to JSON object");

            let obj = parsed.as_object().unwrap();
            assert_eq!(obj.get("token").and_then(|v| v.as_str()), Some("0x123abc"));
            assert_eq!(obj.get("channels").and_then(|v| v.as_str()), Some("0x456def"));
            assert_eq!(obj.get("announcements").and_then(|v| v.as_str()), Some("0x789ghi"));
        } else {
            panic!("Expected string value");
        }
    }

    #[test]
    fn test_contract_address_map_parses_from_json_string() {
        // Create a JSON string input
        let json_input = r#"{"token":"0xaaa","channels":"0xbbb","node_safe_registry":"0xccc"}"#;
        let value = Value::String(json_input.to_string());

        // Parse it
        let result = <ContractAddressMap as async_graphql::ScalarType>::parse(value);

        assert!(result.is_ok(), "Should successfully parse JSON string input");

        if let Ok(contract_map) = result {
            assert_eq!(contract_map.0.get("token").map(|s| s.as_str()), Some("0xaaa"));
            assert_eq!(contract_map.0.get("channels").map(|s| s.as_str()), Some("0xbbb"));
            assert_eq!(
                contract_map.0.get("node_safe_registry").map(|s| s.as_str()),
                Some("0xccc")
            );
        }
    }

    #[test]
    fn test_contract_address_map_rejects_non_string_input() {
        // Try to parse with a JSON object (not a string)
        let mut obj_map = serde_json::Map::new();
        obj_map.insert("token".to_string(), serde_json::json!("0x123"));

        let value = Value::Object(
            obj_map
                .into_iter()
                .map(|(k, v)| {
                    let graphql_value = match v {
                        serde_json::Value::String(s) => Value::String(s),
                        _ => Value::String(String::new()),
                    };
                    (async_graphql::Name::new(k), graphql_value)
                })
                .collect(),
        );

        let result = <ContractAddressMap as async_graphql::ScalarType>::parse(value);
        assert!(result.is_err(), "Should reject non-string input (object)");
    }

    #[test]
    fn test_contract_address_map_roundtrip() {
        // Create original map
        let mut original_map = HashMap::new();
        original_map.insert("token".to_string(), "0x1111".to_string());
        original_map.insert("channels".to_string(), "0x2222".to_string());
        original_map.insert("announcements".to_string(), "0x3333".to_string());

        let original = ContractAddressMap(original_map.clone());

        // Serialize to value
        let serialized = original.to_value();

        // Parse it back
        let parsed = <ContractAddressMap as async_graphql::ScalarType>::parse(serialized);

        assert!(parsed.is_ok(), "Should successfully roundtrip");

        if let Ok(restored) = parsed {
            assert_eq!(original.0, restored.0, "Maps should be equal after roundtrip");
        }
    }

    #[test]
    fn test_contract_address_map_with_empty_map() {
        let empty_map = ContractAddressMap(HashMap::new());

        let value = empty_map.to_value();

        assert!(
            matches!(value, Value::String(_)),
            "Empty map should serialize to string"
        );

        if let Value::String(json_str) = value {
            let parsed: serde_json::Value = serde_json::from_str(&json_str).expect("Should be valid JSON");
            assert!(parsed.is_object());
            assert_eq!(parsed.as_object().unwrap().len(), 0);
        }
    }

    #[test]
    fn test_contract_address_map_parses_invalid_json_string() {
        let invalid_json = Value::String("not valid json".to_string());

        let result = <ContractAddressMap as async_graphql::ScalarType>::parse(invalid_json);
        assert!(result.is_err(), "Should reject invalid JSON string");
    }
}
