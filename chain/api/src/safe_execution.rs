//! Safe contract execution detection and result extraction
//!
//! This module provides utilities for detecting Gnosis Safe contract execution
//! results from on-chain transaction receipts. It decodes the raw signed transaction
//! to extract the `to` address, and inspects receipt logs for
//! `ExecutionSuccess`/`ExecutionFailure` events.

use alloy_sol_types::{SolEvent, sol};
use async_trait::async_trait;
use blokli_db::BlokliDbAllOperations;
use hopr_bindings::exports::alloy::{
    consensus::{Transaction, TxEnvelope},
    eips::eip2718::Decodable2718,
};
use hopr_types::{crypto::types::Hash, primitive::prelude::Address};
use tracing::{error, warn};

use crate::{
    transaction_monitor::{ReceiptLog, SafeAddressChecker},
    transaction_store::SafeExecutionResult,
};

sol! {
    event ExecutionSuccess(bytes32 txHash, uint256 payment);
    event ExecutionFailure(bytes32 txHash, uint256 payment);
    event ExecutionFromModuleSuccess(address indexed module);
    event ExecutionFromModuleFailure(address indexed module);
}

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

        if topic0 == ExecutionSuccess::SIGNATURE_HASH.as_slice() {
            return Some(SafeExecutionResult {
                success: true,
                safe_tx_hash: extract_safe_tx_hash(log),
                revert_reason: None,
            });
        }

        if topic0 == ExecutionFailure::SIGNATURE_HASH.as_slice() {
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
        if topic0 == ExecutionFromModuleSuccess::SIGNATURE_HASH.as_slice() {
            return Some(SafeExecutionResult {
                success: true,
                safe_tx_hash: None,
                revert_reason: None,
            });
        }

        if topic0 == ExecutionFromModuleFailure::SIGNATURE_HASH.as_slice() {
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
        let hash_bytes: [u8; 32] = log.data[..32].try_into().expect("slice length verified as >= 32");
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
        match self.db.get_safe_contract_by_address(None, addr).await {
            Ok(Some(entry)) => {
                let safe_addr: [u8; 20] = entry.address.as_slice().try_into().ok()?;
                return Some(safe_addr);
            }
            Ok(None) => {}
            Err(e) => {
                error!(target = ?target, source = %e, "DB error in get_safe_contract_by_address");
            }
        }

        // Check if the target is a module address associated with a Safe
        match self.db.get_safe_contract_by_module_address(None, addr).await {
            Ok(Some(entry)) => {
                let safe_addr: [u8; 20] = entry.address.as_slice().try_into().ok()?;
                return Some(safe_addr);
            }
            Ok(None) => {}
            Err(e) => {
                error!(target = ?target, source = %e, "DB error in get_safe_contract_by_module_address");
            }
        }

        None
    }
}

#[cfg(test)]
mod tests {
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
            topics: vec![ExecutionSuccess::SIGNATURE_HASH.0],
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
            topics: vec![ExecutionSuccess::SIGNATURE_HASH.0],
            data,
        }];

        let result = inspect_safe_execution_logs(&safe_address, &logs);
        insta::assert_yaml_snapshot!(&result);
    }

    #[test]
    fn test_inspect_safe_execution_logs_failure_event() {
        let safe_address = [0xAA; 20];
        let data = vec![0u8; 64];

        let logs = vec![ReceiptLog {
            address: safe_address,
            topics: vec![ExecutionFailure::SIGNATURE_HASH.0],
            data,
        }];

        let result = inspect_safe_execution_logs(&safe_address, &logs);
        insta::assert_yaml_snapshot!(&result);
    }

    #[test]
    fn test_inspect_safe_execution_logs_indexed_tx_hash() {
        let safe_address = [0xAA; 20];
        let indexed_hash = [0x42u8; 32];

        let logs = vec![ReceiptLog {
            address: safe_address,
            topics: vec![ExecutionSuccess::SIGNATURE_HASH.0, indexed_hash],
            data: vec![0u8; 32], // payment only (no txHash in data)
        }];

        let result = inspect_safe_execution_logs(&safe_address, &logs);
        insta::assert_yaml_snapshot!(&result);
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
    fn test_inspect_safe_execution_logs_short_data_returns_none_hash() {
        let safe_address = [0xAA; 20];

        // Event with only the topic (no indexed txHash) and data shorter than 32 bytes
        let logs = vec![ReceiptLog {
            address: safe_address,
            topics: vec![ExecutionSuccess::SIGNATURE_HASH.0],
            data: vec![0u8; 10], // Too short to contain txHash
        }];

        let result = inspect_safe_execution_logs(&safe_address, &logs);
        insta::assert_yaml_snapshot!(&result);
    }

    #[test]
    fn test_inspect_safe_execution_logs_module_success_event() {
        let safe_address = [0xAA; 20];
        let module_address = [0xBB; 32]; // indexed module address in topics[1]

        let logs = vec![ReceiptLog {
            address: safe_address,
            topics: vec![ExecutionFromModuleSuccess::SIGNATURE_HASH.0, module_address],
            data: vec![],
        }];

        let result = inspect_safe_execution_logs(&safe_address, &logs);
        insta::assert_yaml_snapshot!(&result);
    }

    #[test]
    fn test_inspect_safe_execution_logs_module_failure_event() {
        let safe_address = [0xAA; 20];
        let module_address = [0xBB; 32]; // indexed module address in topics[1]

        let logs = vec![ReceiptLog {
            address: safe_address,
            topics: vec![ExecutionFromModuleFailure::SIGNATURE_HASH.0, module_address],
            data: vec![],
        }];

        let result = inspect_safe_execution_logs(&safe_address, &logs);
        insta::assert_yaml_snapshot!(&result);
    }

    #[test]
    fn test_inspect_safe_execution_logs_module_event_wrong_address() {
        let safe_address = [0xAA; 20];
        let other_address = [0xCC; 20];

        // Module success event from a different contract address should be ignored
        let logs = vec![ReceiptLog {
            address: other_address,
            topics: vec![ExecutionFromModuleSuccess::SIGNATURE_HASH.0],
            data: vec![],
        }];

        assert!(inspect_safe_execution_logs(&safe_address, &logs).is_none());
    }
}
