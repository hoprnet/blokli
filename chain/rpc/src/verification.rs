use std::{sync::Arc, time::Duration};

use alloy::{primitives::Bytes, providers::Provider};
use blokli_chain_types::{AlloyAddressExt, ContractAddresses};
use hopr_async_runtime::prelude::sleep;
use hopr_bindings::{
    hopr_announcements::HoprAnnouncements, hopr_channels::HoprChannels,
    hopr_node_management_module::HoprNodeManagementModule, hopr_node_safe_migration::HoprNodeSafeMigration,
    hopr_node_safe_registry::HoprNodeSafeRegistry, hopr_node_stake_factory::HoprNodeStakeFactory,
    hopr_ticket_price_oracle::HoprTicketPriceOracle, hopr_token::HoprToken,
    hopr_winning_probability_oracle::HoprWinningProbabilityOracle,
};
use hopr_primitive_types::primitives::Address;
use thiserror::Error;

use crate::{errors::RpcError, rpc::RpcOperations, transport::HttpRequestor};

const MAX_RETRY_ATTEMPTS: u32 = 5;
const BASE_DELAY_MS: u64 = 1000; // 1 second

/// Result of verifying a single contract
#[derive(Debug, Clone)]
pub struct VerificationResult {
    pub contract_name: String,
    pub address: Address,
    pub is_valid: bool,
    pub expected_length: usize,
    pub actual_length: usize,
}

/// Errors that can occur during contract verification
#[derive(Error, Debug)]
pub enum VerificationError {
    #[error("No contract code deployed at address {address} for contract '{contract}'")]
    NoCodeDeployed { contract: String, address: Address },

    #[error(
        "Bytecode mismatch for contract '{contract}' at address {address}: expected {expected_length} bytes, got \
         {actual_length} bytes"
    )]
    BytecodeMismatch {
        contract: String,
        address: Address,
        expected_length: usize,
        actual_length: usize,
    },

    #[error("RPC timeout for contract '{contract}' after {attempts} attempts: {last_error}")]
    RpcTimeout {
        contract: String,
        attempts: u32,
        last_error: String,
    },

    #[error("RPC error: {0}")]
    RpcError(#[from] RpcError),
}

/// Contract verifier that compares deployed bytecode against hopr-bindings
#[derive(Debug, Clone)]
pub struct ContractVerifier<R: HttpRequestor + Clone + 'static> {
    rpc_operations: Arc<RpcOperations<R>>,
}

impl<R: HttpRequestor + Clone + 'static> ContractVerifier<R> {
    /// Create a new contract verifier
    pub fn new(rpc_operations: Arc<RpcOperations<R>>) -> Self {
        Self { rpc_operations }
    }

    /// Verify all contracts in the provided addresses
    pub async fn verify_all_contracts(
        &self,
        addrs: &ContractAddresses,
    ) -> std::result::Result<Vec<VerificationResult>, VerificationError> {
        tracing::info!("Starting contract verification for 9 contracts");

        let verifications = vec![
            ("HoprToken", addrs.token, &HoprToken::DEPLOYED_BYTECODE),
            ("HoprChannels", addrs.channels, &HoprChannels::DEPLOYED_BYTECODE),
            (
                "HoprAnnouncements",
                addrs.announcements,
                &HoprAnnouncements::DEPLOYED_BYTECODE,
            ),
            (
                "HoprNodeManagementModule",
                addrs.module_implementation,
                &HoprNodeManagementModule::DEPLOYED_BYTECODE,
            ),
            (
                "HoprNodeSafeMigration",
                addrs.node_safe_migration,
                &HoprNodeSafeMigration::DEPLOYED_BYTECODE,
            ),
            (
                "HoprNodeSafeRegistry",
                addrs.node_safe_registry,
                &HoprNodeSafeRegistry::DEPLOYED_BYTECODE,
            ),
            (
                "HoprTicketPriceOracle",
                addrs.ticket_price_oracle,
                &HoprTicketPriceOracle::DEPLOYED_BYTECODE,
            ),
            (
                "HoprWinningProbabilityOracle",
                addrs.winning_probability_oracle,
                &HoprWinningProbabilityOracle::DEPLOYED_BYTECODE,
            ),
            (
                "HoprNodeStakeFactory",
                addrs.node_stake_factory,
                &HoprNodeStakeFactory::DEPLOYED_BYTECODE,
            ),
        ];

        let mut results = Vec::new();
        for (idx, (name, address, expected_bytecode)) in verifications.iter().enumerate() {
            tracing::info!("Verifying {} at {} [{}/9]", name, address, idx + 1);

            let result = self.verify_single_contract(name, *address, expected_bytecode).await?;

            tracing::info!("✓ {} verified (bytecode: {} bytes)", name, result.expected_length);

            results.push(result);
        }

        tracing::info!("✓ All 9 contracts verified successfully");
        Ok(results)
    }

    /// Verify a single contract by comparing deployed bytecode with expected bytecode
    async fn verify_single_contract(
        &self,
        name: &str,
        addr: Address,
        expected_bytecode: &Bytes,
    ) -> std::result::Result<VerificationResult, VerificationError> {
        let deployed_bytecode = self.fetch_bytecode_with_retry(addr, MAX_RETRY_ATTEMPTS).await?;

        // Check if any code is deployed at the address
        if deployed_bytecode.is_empty() {
            return Err(VerificationError::NoCodeDeployed {
                contract: name.to_string(),
                address: addr,
            });
        }

        // Direct bytecode comparison
        if deployed_bytecode.as_ref() != expected_bytecode.as_ref() {
            return Err(VerificationError::BytecodeMismatch {
                contract: name.to_string(),
                address: addr,
                expected_length: expected_bytecode.len(),
                actual_length: deployed_bytecode.len(),
            });
        }

        Ok(VerificationResult {
            contract_name: name.to_string(),
            address: addr,
            is_valid: true,
            expected_length: expected_bytecode.len(),
            actual_length: deployed_bytecode.len(),
        })
    }

    /// Fetch bytecode with exponential backoff retry logic
    async fn fetch_bytecode_with_retry(
        &self,
        address: Address,
        max_attempts: u32,
    ) -> std::result::Result<Bytes, RpcError> {
        let alloy_address = alloy::primitives::Address::from_hopr_address(address);
        let mut last_error = None;

        for attempt in 1..=max_attempts {
            match self.rpc_operations.provider.get_code_at(alloy_address).await {
                Ok(code) => return Ok(code),
                Err(e) => {
                    last_error = Some(e);

                    // Only retry on transient errors
                    if attempt < max_attempts {
                        let delay = Self::calculate_delay(attempt);
                        tracing::warn!(
                            "RPC error fetching bytecode (attempt {}/{}), retrying in {:?}",
                            attempt,
                            max_attempts,
                            delay
                        );
                        sleep(delay).await;
                    }
                }
            }
        }

        // All retries exhausted
        Err(RpcError::AlloyRpcError(last_error.unwrap()))
    }

    /// Calculate exponential backoff delay with simple jitter
    fn calculate_delay(attempt: u32) -> Duration {
        let base = BASE_DELAY_MS * 2u64.pow(attempt - 1);
        // Simple jitter: alternate between 80% and 120% of base delay
        let jitter_factor = if attempt % 2 == 0 { 1.2 } else { 0.8 };
        let delay_ms = (base as f64 * jitter_factor) as u64;
        Duration::from_millis(delay_ms)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_calculate_delay() {
        // Test that delays increase exponentially
        let delay1 = ContractVerifier::<crate::client::DefaultHttpRequestor>::calculate_delay(1);
        let delay2 = ContractVerifier::<crate::client::DefaultHttpRequestor>::calculate_delay(2);
        let delay3 = ContractVerifier::<crate::client::DefaultHttpRequestor>::calculate_delay(3);

        // Base delays should be approximately 1s, 2s, 4s (with jitter)
        assert!(delay1.as_millis() >= 800 && delay1.as_millis() <= 1200);
        assert!(delay2.as_millis() >= 1600 && delay2.as_millis() <= 2400);
        assert!(delay3.as_millis() >= 3200 && delay3.as_millis() <= 4800);
    }

    #[test]
    fn test_calculate_delay_all_attempts() {
        // Test all 5 retry attempts with jitter pattern
        // Jitter alternates: attempt 1 (0.8x), attempt 2 (1.2x), attempt 3 (0.8x), etc.
        let delay1 = ContractVerifier::<crate::client::DefaultHttpRequestor>::calculate_delay(1);
        let delay2 = ContractVerifier::<crate::client::DefaultHttpRequestor>::calculate_delay(2);
        let delay3 = ContractVerifier::<crate::client::DefaultHttpRequestor>::calculate_delay(3);
        let delay4 = ContractVerifier::<crate::client::DefaultHttpRequestor>::calculate_delay(4);
        let delay5 = ContractVerifier::<crate::client::DefaultHttpRequestor>::calculate_delay(5);

        // Verify exponential backoff with alternating jitter
        // Base: 1000, 2000, 4000, 8000, 16000
        // With jitter (0.8x or 1.2x): 800, 2400, 3200, 9600, 12800
        assert_eq!(delay1.as_millis(), 800, "Attempt 1: 1000ms * 0.8");
        assert_eq!(delay2.as_millis(), 2400, "Attempt 2: 2000ms * 1.2");
        assert_eq!(delay3.as_millis(), 3200, "Attempt 3: 4000ms * 0.8");
        assert_eq!(delay4.as_millis(), 9600, "Attempt 4: 8000ms * 1.2");
        assert_eq!(delay5.as_millis(), 12800, "Attempt 5: 16000ms * 0.8");
    }

    #[test]
    fn test_verification_error_no_code_deployed_display() {
        let error = VerificationError::NoCodeDeployed {
            contract: "HoprToken".to_string(),
            address: Address::default(),
        };

        let error_msg = format!("{}", error);
        assert!(
            error_msg.contains("No contract code deployed"),
            "Error should contain 'No contract code deployed'"
        );
        assert!(error_msg.contains("HoprToken"), "Error should contain contract name");
        assert!(
            error_msg.contains(&Address::default().to_string()),
            "Error should contain address"
        );
    }

    #[test]
    fn test_verification_error_bytecode_mismatch_display() {
        let error = VerificationError::BytecodeMismatch {
            contract: "HoprChannels".to_string(),
            address: Address::default(),
            expected_length: 12345,
            actual_length: 54321,
        };

        let error_msg = format!("{}", error);
        assert!(
            error_msg.contains("Bytecode mismatch"),
            "Error should contain 'Bytecode mismatch'"
        );
        assert!(error_msg.contains("HoprChannels"), "Error should contain contract name");
        assert!(error_msg.contains("12345"), "Error should contain expected length");
        assert!(error_msg.contains("54321"), "Error should contain actual length");
    }

    #[test]
    fn test_verification_error_rpc_timeout_display() {
        let error = VerificationError::RpcTimeout {
            contract: "HoprAnnouncements".to_string(),
            attempts: 5,
            last_error: "connection timeout".to_string(),
        };

        let error_msg = format!("{}", error);
        assert!(error_msg.contains("RPC timeout"), "Error should contain 'RPC timeout'");
        assert!(
            error_msg.contains("HoprAnnouncements"),
            "Error should contain contract name"
        );
        assert!(error_msg.contains("5"), "Error should contain attempt count");
        assert!(
            error_msg.contains("connection timeout"),
            "Error should contain last error"
        );
    }

    #[test]
    fn test_verification_result_structure() {
        let result = VerificationResult {
            contract_name: "HoprToken".to_string(),
            address: Address::default(),
            is_valid: true,
            expected_length: 10000,
            actual_length: 10000,
        };

        assert_eq!(result.contract_name, "HoprToken");
        assert!(result.is_valid);
        assert_eq!(result.expected_length, result.actual_length);
    }
}
