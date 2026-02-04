//! Revert reason decoding from debug trace output
//!
//! This module handles parsing `debug_traceTransaction` output (using `callTracer`)
//! and decoding Solidity revert reasons from the deepest failed internal call frame.

use tracing::debug;

/// Selector for `Error(string)` — first 4 bytes of keccak256("Error(string)")
const ERROR_SELECTOR: [u8; 4] = [0x08, 0xc3, 0x79, 0xa0];

/// Selector for `Panic(uint256)` — first 4 bytes of keccak256("Panic(uint256)")
const PANIC_SELECTOR: [u8; 4] = [0x4e, 0x48, 0x7b, 0x71];

/// Decode a Solidity revert reason from raw output bytes.
///
/// Supports:
/// - `Error(string)`: ABI-encoded revert string (e.g., `require(false, "message")`)
/// - `Panic(uint256)`: Panic codes (e.g., arithmetic overflow, division by zero)
/// - Custom/unknown errors: returns hex-encoded fallback
/// - Empty output: returns `None`
pub fn decode_revert_reason(output: &[u8]) -> Option<String> {
    if output.is_empty() {
        return None;
    }

    if output.len() < 4 {
        return Some(format!("0x{}", hex_encode(output)));
    }

    let selector: [u8; 4] = output[..4].try_into().ok()?;

    if selector == ERROR_SELECTOR {
        return decode_error_string(&output[4..]);
    }

    if selector == PANIC_SELECTOR {
        return decode_panic_code(&output[4..]);
    }

    // Unknown/custom error — return hex fallback
    Some(format!("0x{}", hex_encode(output)))
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

/// Decode an ABI-encoded `Error(string)` payload (after the 4-byte selector).
fn decode_error_string(data: &[u8]) -> Option<String> {
    // ABI encoding: offset (32 bytes) + length (32 bytes) + string data
    if data.len() < 64 {
        return Some(format!("0x{}{}", hex_encode(&ERROR_SELECTOR), hex_encode(data)));
    }

    // Read the string length from bytes 32..64
    let length_bytes: [u8; 32] = data[32..64].try_into().ok()?;
    let length = u256_to_usize(length_bytes)?;

    if data.len() < 64 + length {
        return Some(format!("0x{}{}", hex_encode(&ERROR_SELECTOR), hex_encode(data)));
    }

    let string_data = &data[64..64 + length];
    match std::str::from_utf8(string_data) {
        Ok(s) => Some(s.to_string()),
        Err(_) => Some(format!("0x{}{}", hex_encode(&ERROR_SELECTOR), hex_encode(data))),
    }
}

/// Decode an ABI-encoded `Panic(uint256)` payload (after the 4-byte selector).
fn decode_panic_code(data: &[u8]) -> Option<String> {
    if data.len() < 32 {
        return Some(format!("0x{}{}", hex_encode(&PANIC_SELECTOR), hex_encode(data)));
    }

    let code_bytes: [u8; 32] = data[..32].try_into().ok()?;
    let code = u256_to_usize(code_bytes).unwrap_or(usize::MAX);

    let description = match code {
        0x00 => "generic compiler panic",
        0x01 => "assertion failure",
        0x11 => "arithmetic overflow",
        0x12 => "division by zero",
        0x21 => "enum conversion overflow",
        0x22 => "storage encoding error",
        0x31 => "pop on empty array",
        0x32 => "array index out of bounds",
        0x41 => "excessive memory allocation",
        0x51 => "uninitialized function pointer",
        _ => "unknown panic code",
    };

    Some(format!("Panic(0x{code:02x}): {description}"))
}

/// Simple hex encoding (lowercase, no prefix).
fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Decode a hex string (with or without `0x` prefix) to bytes.
fn hex_decode(hex: &str) -> Option<Vec<u8>> {
    let hex = hex.strip_prefix("0x").unwrap_or(hex);
    if !hex.len().is_multiple_of(2) {
        return None;
    }

    (0..hex.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).ok())
        .collect()
}

/// Convert a big-endian 256-bit integer to usize, returning None on overflow.
fn u256_to_usize(bytes: [u8; 32]) -> Option<usize> {
    // Check that the high bytes are all zero (value fits in usize)
    let high_cutoff = 32 - std::mem::size_of::<usize>();
    if bytes[..high_cutoff].iter().any(|&b| b != 0) {
        return None;
    }

    let mut result: usize = 0;
    for &b in &bytes[high_cutoff..] {
        result = result.checked_shl(8)?.checked_add(b as usize)?;
    }
    Some(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_decode_error_string_known_message() {
        // Error(string) with message "insufficient funds"
        let message = "insufficient funds";
        let mut data = Vec::new();

        // Selector
        data.extend_from_slice(&ERROR_SELECTOR);

        // ABI encoding: offset = 32
        let mut offset = [0u8; 32];
        offset[31] = 0x20;
        data.extend_from_slice(&offset);

        // Length
        let mut length = [0u8; 32];
        length[31] = message.len() as u8;
        data.extend_from_slice(&length);

        // String data (padded to 32 bytes)
        let mut padded = vec![0u8; 32];
        padded[..message.len()].copy_from_slice(message.as_bytes());
        data.extend_from_slice(&padded);

        let result = decode_revert_reason(&data);
        assert_eq!(result, Some("insufficient funds".to_string()));
    }

    #[test]
    fn test_decode_panic_codes() {
        let test_cases = [
            (0x01u8, "Panic(0x01): assertion failure"),
            (0x11, "Panic(0x11): arithmetic overflow"),
            (0x12, "Panic(0x12): division by zero"),
            (0x32, "Panic(0x32): array index out of bounds"),
        ];

        for (code, expected) in test_cases {
            let mut data = Vec::new();
            data.extend_from_slice(&PANIC_SELECTOR);

            let mut code_bytes = [0u8; 32];
            code_bytes[31] = code;
            data.extend_from_slice(&code_bytes);

            let result = decode_revert_reason(&data);
            assert_eq!(result, Some(expected.to_string()), "Failed for panic code 0x{code:02x}");
        }
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
        let inner_output = format!("0x{}", hex_encode(&ERROR_SELECTOR));
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
        assert_eq!(bytes, ERROR_SELECTOR.to_vec());
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
