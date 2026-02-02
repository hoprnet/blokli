//! Safe contract execution detection and result extraction
//!
//! This module provides utilities for detecting Gnosis Safe contract execution
//! results from on-chain transaction receipts. It decodes the raw signed transaction
//! to extract the `to` address, and inspects receipt logs for
//! `ExecutionSuccess`/`ExecutionFailure` events.

use async_trait::async_trait;
use blokli_db::BlokliDbAllOperations;
use hopr_bindings::exports::alloy::{
    consensus::{Transaction, TxEnvelope},
    eips::eip2718::Decodable2718,
};
use hopr_crypto_types::types::Hash;
use hopr_primitive_types::prelude::Address;
use tracing::warn;

use crate::{
    transaction_monitor::{ReceiptLog, SafeAddressChecker},
    transaction_store::SafeExecutionResult,
};

/// Gnosis Safe event topic: keccak256("ExecutionSuccess(bytes32,uint256)")
pub(crate) const EXECUTION_SUCCESS_TOPIC: [u8; 32] = [
    0x44, 0x2e, 0x71, 0x5f, 0x62, 0x63, 0x46, 0xe8, 0xc5, 0x43, 0x81, 0x00, 0x2d, 0xa6, 0x14, 0xf6, 0x2b, 0xee, 0x8d,
    0x27, 0x38, 0x65, 0x35, 0xb2, 0x52, 0x1e, 0xc8, 0x54, 0x08, 0x98, 0x55, 0x6e,
];

/// Gnosis Safe event topic: keccak256("ExecutionFailure(bytes32,uint256)")
const EXECUTION_FAILURE_TOPIC: [u8; 32] = [
    0x23, 0x42, 0x8b, 0x18, 0xac, 0xfb, 0x3e, 0xa6, 0x4b, 0x08, 0xdc, 0x0c, 0x1d, 0x29, 0x6e, 0xa9, 0xc0, 0x97, 0x02,
    0xc0, 0x90, 0x83, 0xca, 0x52, 0x72, 0xe6, 0x4d, 0x11, 0x5b, 0x68, 0x7d, 0x23,
];

/// Gnosis Safe event topic: keccak256("ExecutionFromModuleSuccess(address)")
///
/// Emitted when a transaction is executed via a Safe module (e.g., `execTransactionFromModule`).
/// The event signature is `event ExecutionFromModuleSuccess(address indexed module)`.
/// Unlike direct execution events, there is no `txHash` parameter.
pub(crate) const EXECUTION_FROM_MODULE_SUCCESS_TOPIC: [u8; 32] = [
    0x68, 0x95, 0xc1, 0x36, 0x64, 0xaa, 0x4f, 0x67, 0x28, 0x8b, 0x25, 0xd7, 0xa2, 0x1d, 0x7a, 0xaa, 0x34, 0x91, 0x6e,
    0x35, 0x5f, 0xb9, 0xb6, 0xfa, 0xe0, 0xa1, 0x39, 0xa9, 0x08, 0x5b, 0xec, 0xb8,
];

/// Gnosis Safe event topic: keccak256("ExecutionFromModuleFailure(address)")
///
/// Emitted when a module-initiated transaction fails inside the Safe.
/// The event signature is `event ExecutionFromModuleFailure(address indexed module)`.
/// Unlike direct execution events, there is no `txHash` parameter.
pub(crate) const EXECUTION_FROM_MODULE_FAILURE_TOPIC: [u8; 32] = [
    0xac, 0xd2, 0xc8, 0x70, 0x28, 0x04, 0x12, 0x8f, 0xdb, 0x0d, 0xb2, 0xbb, 0x49, 0xf6, 0xd1, 0x27, 0xdd, 0x01, 0x81,
    0xc1, 0x3f, 0xd4, 0x5d, 0xbf, 0xe1, 0x6d, 0xe0, 0x93, 0x0e, 0x2b, 0xd3, 0x75,
];

/// Extract the `to` address from a raw signed transaction.
///
/// Supports legacy, EIP-2930, EIP-1559, and EIP-4844 transaction types.
/// Returns `None` for contract creation transactions or if decoding fails.
pub fn decode_transaction_to_address(raw_tx: &[u8]) -> Option<[u8; 20]> {
    let envelope = TxEnvelope::decode_2718(&mut &raw_tx[..]).ok()?;
    let to_addr = envelope.to()?;
    Some(to_addr.into_array())
}

/// Check receipt logs for Safe `ExecutionSuccess`/`ExecutionFailure` events.
///
/// Inspects the given logs for Gnosis Safe execution events emitted by the
/// specified Safe address. Returns `None` if no Safe execution event is found.
pub fn inspect_safe_execution_logs(safe_address: &[u8; 20], logs: &[ReceiptLog]) -> Option<SafeExecutionResult> {
    for log in logs {
        // Only consider logs from the Safe contract address
        if log.address != *safe_address {
            continue;
        }

        let Some(topic0) = log.topics.first() else {
            continue;
        };

        if *topic0 == EXECUTION_SUCCESS_TOPIC {
            return Some(SafeExecutionResult {
                success: true,
                safe_tx_hash: extract_safe_tx_hash(log),
                revert_reason: None,
            });
        }

        if *topic0 == EXECUTION_FAILURE_TOPIC {
            return Some(SafeExecutionResult {
                success: false,
                safe_tx_hash: extract_safe_tx_hash(log),
                // Revert reason is not available from Safe events directly.
                // The Safe contract catches the revert internally and only
                // emits the txHash and payment in the event data.
                revert_reason: None,
            });
        }

        // Module execution events: ExecutionFromModuleSuccess/ExecutionFromModuleFailure
        // These have signature `event ExecutionFromModule{Success,Failure}(address indexed module)`
        // and do not contain a Safe txHash parameter.
        if *topic0 == EXECUTION_FROM_MODULE_SUCCESS_TOPIC {
            return Some(SafeExecutionResult {
                success: true,
                safe_tx_hash: None,
                revert_reason: None,
            });
        }

        if *topic0 == EXECUTION_FROM_MODULE_FAILURE_TOPIC {
            return Some(SafeExecutionResult {
                success: false,
                safe_tx_hash: None,
                revert_reason: None,
            });
        }
    }

    None
}

/// Extract the Safe transaction hash from an execution event log.
///
/// Both `ExecutionSuccess` and `ExecutionFailure` events encode txHash as either:
/// - `topics[1]` (if the parameter is indexed)
/// - First 32 bytes of `data` (if not indexed, as in Safe v1.3.0)
///
/// Returns `None` if the log data is too short to contain a transaction hash.
fn extract_safe_tx_hash(log: &ReceiptLog) -> Option<Hash> {
    // Check if txHash is in topics[1] (indexed parameter)
    if log.topics.len() > 1 {
        return Some(Hash::from(log.topics[1]));
    }

    // Fall back to first 32 bytes of data (non-indexed parameter)
    if log.data.len() >= 32 {
        let mut hash_bytes = [0u8; 32];
        hash_bytes.copy_from_slice(&log.data[..32]);
        return Some(Hash::from(hash_bytes));
    }

    warn!(
        "Safe execution event data is too short ({} bytes), expected at least 32",
        log.data.len()
    );
    None
}

/// Database-backed Safe address checker.
///
/// Checks if a given address corresponds to a known Safe contract
/// by querying the database.
pub struct DbSafeAddressChecker<T> {
    db: T,
}

impl<T> std::fmt::Debug for DbSafeAddressChecker<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DbSafeAddressChecker").finish_non_exhaustive()
    }
}

impl<T> DbSafeAddressChecker<T> {
    pub fn new(db: T) -> Self {
        Self { db }
    }
}

#[async_trait]
impl<T: BlokliDbAllOperations + Send + Sync> SafeAddressChecker for DbSafeAddressChecker<T> {
    async fn find_safe_for_target(&self, target: &[u8; 20]) -> Option<[u8; 20]> {
        let addr = Address::try_from(target.as_slice()).ok()?;

        // Check if the target is a known Safe contract address (returns itself)
        if let Ok(Some(entry)) = self.db.get_safe_contract_by_address(None, addr).await {
            let mut safe_addr = [0u8; 20];
            safe_addr.copy_from_slice(&entry.address);
            return Some(safe_addr);
        }

        // Check if the target is a module address associated with a Safe
        if let Ok(Some(entry)) = self.db.get_safe_contract_by_module_address(None, addr).await {
            let mut safe_addr = [0u8; 20];
            safe_addr.copy_from_slice(&entry.address);
            return Some(safe_addr);
        }

        None
    }
}

#[cfg(test)]
mod tests {
    use hopr_bindings::exports::alloy::primitives::keccak256;

    use super::*;

    #[test]
    fn test_decode_transaction_to_address_invalid_data() {
        assert!(decode_transaction_to_address(&[]).is_none());
        assert!(decode_transaction_to_address(&[0xff]).is_none());
        assert!(decode_transaction_to_address(&[0x01, 0x02, 0x03]).is_none());
    }

    #[test]
    fn test_inspect_safe_execution_logs_no_safe_events() {
        let safe_address = [0xAA; 20];
        let logs = vec![ReceiptLog {
            address: [0xBB; 20], // Different address
            topics: vec![EXECUTION_SUCCESS_TOPIC],
            data: vec![0u8; 64],
        }];

        assert!(inspect_safe_execution_logs(&safe_address, &logs).is_none());
    }

    #[test]
    fn test_inspect_safe_execution_logs_success_event() {
        let safe_address = [0xAA; 20];
        let mut data = vec![0u8; 64];
        // Set txHash in first 32 bytes of data
        data[0] = 0x42;
        data[31] = 0xFF;

        let logs = vec![ReceiptLog {
            address: safe_address,
            topics: vec![EXECUTION_SUCCESS_TOPIC],
            data,
        }];

        let result = inspect_safe_execution_logs(&safe_address, &logs);
        assert!(result.is_some());

        let result = result.unwrap();
        assert!(result.success);
        assert!(result.revert_reason.is_none());

        let safe_tx_hash = result.safe_tx_hash.expect("safe_tx_hash should be present");
        let hash_bytes: &[u8] = safe_tx_hash.as_ref();
        assert_eq!(hash_bytes[0], 0x42);
        assert_eq!(hash_bytes[31], 0xFF);
    }

    #[test]
    fn test_inspect_safe_execution_logs_failure_event() {
        let safe_address = [0xAA; 20];
        let data = vec![0u8; 64];

        let logs = vec![ReceiptLog {
            address: safe_address,
            topics: vec![EXECUTION_FAILURE_TOPIC],
            data,
        }];

        let result = inspect_safe_execution_logs(&safe_address, &logs);
        assert!(result.is_some());

        let result = result.unwrap();
        assert!(!result.success);
        assert!(result.revert_reason.is_none());
    }

    #[test]
    fn test_inspect_safe_execution_logs_indexed_tx_hash() {
        let safe_address = [0xAA; 20];
        let indexed_hash = [0x42u8; 32];

        let logs = vec![ReceiptLog {
            address: safe_address,
            topics: vec![EXECUTION_SUCCESS_TOPIC, indexed_hash],
            data: vec![0u8; 32], // payment only (no txHash in data)
        }];

        let result = inspect_safe_execution_logs(&safe_address, &logs).unwrap();
        assert!(result.success);

        let safe_tx_hash = result.safe_tx_hash.expect("safe_tx_hash should be present");
        let hash_bytes: &[u8] = safe_tx_hash.as_ref();
        assert_eq!(hash_bytes, &indexed_hash);
    }

    #[test]
    fn test_inspect_safe_execution_logs_empty_logs() {
        let safe_address = [0xAA; 20];
        assert!(inspect_safe_execution_logs(&safe_address, &[]).is_none());
    }

    #[test]
    fn test_inspect_safe_execution_logs_unrelated_event() {
        let safe_address = [0xAA; 20];
        let logs = vec![ReceiptLog {
            address: safe_address,
            topics: vec![[0xFF; 32]], // Unknown event topic
            data: vec![0u8; 64],
        }];

        assert!(inspect_safe_execution_logs(&safe_address, &logs).is_none());
    }

    #[test]
    fn test_execution_success_topic_hash() {
        let computed = keccak256("ExecutionSuccess(bytes32,uint256)");
        assert_eq!(computed.0, EXECUTION_SUCCESS_TOPIC);
    }

    #[test]
    fn test_execution_failure_topic_hash() {
        let computed = keccak256("ExecutionFailure(bytes32,uint256)");
        assert_eq!(computed.0, EXECUTION_FAILURE_TOPIC);
    }

    #[test]
    fn test_inspect_safe_execution_logs_short_data_returns_none_hash() {
        let safe_address = [0xAA; 20];

        // Event with only the topic (no indexed txHash) and data shorter than 32 bytes
        let logs = vec![ReceiptLog {
            address: safe_address,
            topics: vec![EXECUTION_SUCCESS_TOPIC],
            data: vec![0u8; 10], // Too short to contain txHash
        }];

        let result = inspect_safe_execution_logs(&safe_address, &logs).unwrap();
        assert!(result.success);
        assert!(
            result.safe_tx_hash.is_none(),
            "safe_tx_hash should be None for short data"
        );
    }

    #[test]
    fn test_execution_from_module_success_topic_hash() {
        let computed = keccak256("ExecutionFromModuleSuccess(address)");
        assert_eq!(computed.0, EXECUTION_FROM_MODULE_SUCCESS_TOPIC);
    }

    #[test]
    fn test_execution_from_module_failure_topic_hash() {
        let computed = keccak256("ExecutionFromModuleFailure(address)");
        assert_eq!(computed.0, EXECUTION_FROM_MODULE_FAILURE_TOPIC);
    }

    #[test]
    fn test_inspect_safe_execution_logs_module_success_event() {
        let safe_address = [0xAA; 20];
        let module_address = [0xBB; 32]; // indexed module address in topics[1]

        let logs = vec![ReceiptLog {
            address: safe_address,
            topics: vec![EXECUTION_FROM_MODULE_SUCCESS_TOPIC, module_address],
            data: vec![],
        }];

        let result = inspect_safe_execution_logs(&safe_address, &logs);
        assert!(result.is_some());

        let result = result.unwrap();
        assert!(result.success);
        assert!(
            result.safe_tx_hash.is_none(),
            "module execution events have no safe_tx_hash"
        );
        assert!(result.revert_reason.is_none());
    }

    #[test]
    fn test_inspect_safe_execution_logs_module_failure_event() {
        let safe_address = [0xAA; 20];
        let module_address = [0xBB; 32]; // indexed module address in topics[1]

        let logs = vec![ReceiptLog {
            address: safe_address,
            topics: vec![EXECUTION_FROM_MODULE_FAILURE_TOPIC, module_address],
            data: vec![],
        }];

        let result = inspect_safe_execution_logs(&safe_address, &logs);
        assert!(result.is_some());

        let result = result.unwrap();
        assert!(!result.success);
        assert!(
            result.safe_tx_hash.is_none(),
            "module execution events have no safe_tx_hash"
        );
        assert!(result.revert_reason.is_none());
    }

    #[test]
    fn test_inspect_safe_execution_logs_module_event_wrong_address() {
        let safe_address = [0xAA; 20];
        let other_address = [0xCC; 20];

        // Module success event from a different contract address should be ignored
        let logs = vec![ReceiptLog {
            address: other_address,
            topics: vec![EXECUTION_FROM_MODULE_SUCCESS_TOPIC],
            data: vec![],
        }];

        assert!(inspect_safe_execution_logs(&safe_address, &logs).is_none());
    }
}
