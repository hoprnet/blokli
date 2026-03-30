//! Revert reason decoding from debug trace output
//!
//! This module handles parsing `debug_traceTransaction` output (using `callTracer`)
//! and decoding Solidity revert reasons from the deepest failed internal call frame.

use tracing::debug;

/// Decode a Solidity revert reason from raw output bytes.
///
/// Delegates to alloy's ABI decoder for `Error(string)` and `Panic(uint256)`,
/// falling back to hex-encoded output for unrecognized or empty payloads.
pub fn decode_revert_reason(output: &[u8]) -> Option<String> {
    if output.is_empty() {
        return None;
    }

    alloy_sol_types::decode_revert_reason(output).or_else(|| Some(format!("0x{}", hex::encode(output))))
}

/// Extract the revert output bytes from the deepest failed frame in a `callTracer` trace.
///
/// Walks the trace tree depth-first through `calls` arrays, finding the deepest
/// frame with an `"error"` field and non-empty `"output"`.
///
/// Returns `None` if no reverted frame is found.
pub fn extract_revert_output_from_trace(trace: &serde_json::Value) -> Option<Vec<u8>> {
    let mut best: Option<(usize, Vec<u8>)> = None;
    find_deepest_revert(trace, 0, &mut best);
    best.map(|(_, bytes)| bytes)
}

/// Recursively walk the trace tree to find the deepest frame with an error and output.
fn find_deepest_revert(frame: &serde_json::Value, depth: usize, best: &mut Option<(usize, Vec<u8>)>) {
    // Check children first (depth-first)
    if let Some(calls) = frame.get("calls").and_then(|c| c.as_array()) {
        for child in calls {
            find_deepest_revert(child, depth + 1, best);
        }
    }

    // Check if this frame has an error with output
    let has_error = frame
        .get("error")
        .and_then(|e| e.as_str())
        .is_some_and(|s| !s.is_empty());

    if !has_error {
        return;
    }

    let output_hex = match frame.get("output").and_then(|o| o.as_str()) {
        Some(s) if !s.is_empty() && s != "0x" => s,
        _ => return,
    };

    let output_bytes = match hex_decode(output_hex) {
        Some(bytes) => bytes,
        None => {
            debug!("Failed to decode hex output from trace frame at depth {depth}: {output_hex}");
            return;
        }
    };

    // Keep the deepest match
    let dominated = best.as_ref().is_none_or(|(d, _)| depth >= *d);
    if dominated {
        *best = Some((depth, output_bytes));
    }
}

/// Decode a hex string (with or without `0x` prefix) to bytes.
fn hex_decode(s: &str) -> Option<Vec<u8>> {
    let s = s.strip_prefix("0x").unwrap_or(s);
    hex::decode(s).ok()
}

#[cfg(test)]
mod tests {
    use alloy_sol_types::{Panic, Revert, SolError};

    use super::*;

    #[test]
    fn test_decode_error_string_known_message() {
        let revert = Revert::from("insufficient funds");
        let encoded = revert.abi_encode();

        let result = decode_revert_reason(&encoded);
        assert_eq!(result, Some("revert: insufficient funds".to_string()));
    }

    #[test]
    fn test_decode_panic_codes() {
        let codes: Vec<u8> = vec![0x01, 0x11, 0x12, 0x32];
        let results: Vec<Option<String>> = codes
            .iter()
            .map(|&code| {
                let mut data = Vec::new();
                data.extend_from_slice(&Panic::SELECTOR);

                let mut code_bytes = [0u8; 32];
                code_bytes[31] = code;
                data.extend_from_slice(&code_bytes);

                decode_revert_reason(&data)
            })
            .collect();

        insta::assert_yaml_snapshot!(results);
    }

    #[test]
    fn test_decode_custom_error_returns_hex() {
        // Custom error with unknown selector
        let data = vec![0xDE, 0xAD, 0xBE, 0xEF, 0x01, 0x02, 0x03, 0x04];
        let result = decode_revert_reason(&data);
        assert_eq!(result, Some("0xdeadbeef01020304".to_string()));
    }

    #[test]
    fn test_decode_empty_bytes_returns_none() {
        let result = decode_revert_reason(&[]);
        assert!(result.is_none());
    }

    #[test]
    fn test_extract_revert_from_nested_trace() {
        // Simulate a callTracer output with nested calls where the deepest one has the revert
        let inner_output = format!("0x{}", hex::encode(Revert::SELECTOR));
        let trace = serde_json::json!({
            "type": "CALL",
            "from": "0x1111111111111111111111111111111111111111",
            "to": "0x2222222222222222222222222222222222222222",
            "error": "execution reverted",
            "output": "0x",
            "calls": [
                {
                    "type": "DELEGATECALL",
                    "from": "0x2222222222222222222222222222222222222222",
                    "to": "0x3333333333333333333333333333333333333333",
                    "error": "execution reverted",
                    "output": "0xaabbccdd",
                    "calls": [
                        {
                            "type": "CALL",
                            "from": "0x3333333333333333333333333333333333333333",
                            "to": "0x4444444444444444444444444444444444444444",
                            "error": "execution reverted",
                            "output": &inner_output
                        }
                    ]
                }
            ]
        });

        let output = extract_revert_output_from_trace(&trace);
        assert!(output.is_some(), "should find output from deepest reverted frame");

        let bytes = output.unwrap();
        assert_eq!(bytes, Revert::SELECTOR.to_vec());
    }

    #[test]
    fn test_extract_revert_no_error_returns_none() {
        let trace = serde_json::json!({
            "type": "CALL",
            "from": "0x1111111111111111111111111111111111111111",
            "to": "0x2222222222222222222222222222222222222222",
            "output": "0xdeadbeef",
            "calls": [
                {
                    "type": "CALL",
                    "from": "0x2222222222222222222222222222222222222222",
                    "to": "0x3333333333333333333333333333333333333333",
                    "output": "0x"
                }
            ]
        });

        let result = extract_revert_output_from_trace(&trace);
        assert!(result.is_none(), "should return None when no error frames exist");
    }

    #[test]
    fn test_extract_revert_picks_deepest_frame() {
        let trace = serde_json::json!({
            "type": "CALL",
            "error": "execution reverted",
            "output": "0xaaaa",
            "calls": [
                {
                    "type": "CALL",
                    "error": "execution reverted",
                    "output": "0xbbbb"
                }
            ]
        });

        let output = extract_revert_output_from_trace(&trace);
        let bytes = output.expect("should extract output");
        // Should pick the deeper child frame (0xbbbb), not the parent (0xaaaa)
        assert_eq!(bytes, vec![0xbb, 0xbb]);
    }

    #[test]
    fn test_hex_decode_with_prefix() {
        assert_eq!(hex_decode("0xaabb"), Some(vec![0xaa, 0xbb]));
    }

    #[test]
    fn test_hex_decode_without_prefix() {
        assert_eq!(hex_decode("aabb"), Some(vec![0xaa, 0xbb]));
    }

    #[test]
    fn test_hex_decode_odd_length_returns_none() {
        assert!(hex_decode("0xaab").is_none());
    }

    #[test]
    fn test_decode_short_bytes_returns_hex() {
        let data = vec![0xAB, 0xCD];
        let result = decode_revert_reason(&data);
        assert_eq!(result, Some("0xabcd".to_string()));
    }
}
