//! Policy controlling which raw transactions the executor will submit.
//!
//! The policy is the integration point for the stand-alone [`blokli_tx`] filtering crate.
//! [`TransactionPolicy::AllowAll`] accepts any non-empty transaction (used by the standalone API
//! stubs and tests). [`TransactionPolicy::Whitelist`] enforces a `(contract, selector)` allow-set;
//! in production it is built by [`network_transaction_filter`] from the network's contract addresses,
//! so the set of relayable HOPR operations is a property of the network, not operator configuration.

use blokli_chain_types::ContractAddresses;
use blokli_tx::{FilterError, TransactionFilter};
use hopr_bindings::{
    exports::alloy::sol_types::SolCall,
    hopr_channels::HoprChannels::{
        closeIncomingChannelCall, closeIncomingChannelSafeCall, finalizeOutgoingChannelClosureCall,
        finalizeOutgoingChannelClosureSafeCall, fundChannelCall, fundChannelSafeCall,
        initiateOutgoingChannelClosureCall, initiateOutgoingChannelClosureSafeCall, redeemTicketCall,
        redeemTicketSafeCall,
    },
    hopr_node_safe_registry::HoprNodeSafeRegistry::{deregisterNodeBySafeCall, registerSafeByNodeCall},
    hopr_token::HoprToken::{approveCall, sendCall, transferCall},
};

/// Build the network transaction allow-set from its contract addresses.
///
/// Maps each HOPR contract to the function selectors bloklid relays for it. Channel operations are
/// included in both their direct and Safe-module (`*Safe`) variants, since the filter unwraps
/// `execTransactionFromModule` and matches the inner call. Token `approve`/`transfer`/`send` and the
/// safe-registry register/deregister operations cover the remaining relayable calls.
pub fn network_transaction_filter(contracts: &ContractAddresses) -> TransactionFilter {
    let token = contracts.token;
    let channels = contracts.channels;
    let registry = contracts.node_safe_registry;

    TransactionFilter::from_pairs([
        (token, approveCall::SELECTOR),
        (token, transferCall::SELECTOR),
        (token, sendCall::SELECTOR),
        (channels, fundChannelCall::SELECTOR),
        (channels, fundChannelSafeCall::SELECTOR),
        (channels, closeIncomingChannelCall::SELECTOR),
        (channels, closeIncomingChannelSafeCall::SELECTOR),
        (channels, initiateOutgoingChannelClosureCall::SELECTOR),
        (channels, initiateOutgoingChannelClosureSafeCall::SELECTOR),
        (channels, finalizeOutgoingChannelClosureCall::SELECTOR),
        (channels, finalizeOutgoingChannelClosureSafeCall::SELECTOR),
        (channels, redeemTicketCall::SELECTOR),
        (channels, redeemTicketSafeCall::SELECTOR),
        (registry, registerSafeByNodeCall::SELECTOR),
        (registry, deregisterNodeBySafeCall::SELECTOR),
    ])
}

/// Decides whether a raw signed transaction may be submitted to the chain.
#[derive(Debug, Clone, Default)]
pub enum TransactionPolicy {
    /// Accept any non-empty transaction; no allow-set enforcement.
    #[default]
    AllowAll,
    /// Enforce a `blokli-tx` `(contract, selector)` allow-set.
    Whitelist(TransactionFilter),
}

impl TransactionPolicy {
    /// Check a raw signed transaction against the policy.
    ///
    /// An empty payload is rejected in all modes. Under [`TransactionPolicy::Whitelist`] the
    /// transaction is decoded and matched against the allow-set via
    /// [`TransactionFilter::filter_transaction`].
    ///
    /// # Errors
    /// Returns a [`FilterError`] when the payload is empty or, under a whitelist policy, when the
    /// transaction cannot be decoded or is not authorized.
    pub fn check(&self, raw_tx: &[u8]) -> Result<(), FilterError> {
        if raw_tx.is_empty() {
            return Err(FilterError::Empty);
        }

        match self {
            TransactionPolicy::AllowAll => Ok(()),
            TransactionPolicy::Whitelist(filter) => filter.filter_transaction(raw_tx).map(|_| ()),
        }
    }
}

#[cfg(test)]
mod tests {
    use blokli_chain_types::ContractAddresses;
    use hopr_bindings::{
        exports::alloy::{
            consensus::{SignableTransaction, TxEip1559},
            eips::eip2718::Encodable2718,
            primitives::{Address as AlloyAddress, Bytes, TxKind, U256},
            signers::{SignerSync, local::PrivateKeySigner},
            sol_types::SolCall,
        },
        hopr_channels::HoprChannels::fundChannelSafeCall,
        hopr_token::HoprToken::approveCall,
    };
    use hopr_types::primitive::prelude::Address;

    use super::*;

    const KEY: &str = "ac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80";
    const TOKEN: [u8; 20] = [0x11; 20];
    const CHANNELS: [u8; 20] = [0x22; 20];
    const REGISTRY: [u8; 20] = [0x33; 20];

    fn signed_tx(to: [u8; 20], selector: [u8; 4]) -> Vec<u8> {
        let signer: PrivateKeySigner = KEY.parse().unwrap();
        let mut input = selector.to_vec();
        input.extend_from_slice(&[0u8; 32]);
        let tx = TxEip1559 {
            chain_id: 1,
            nonce: 0,
            gas_limit: 21_000,
            max_fee_per_gas: 1_000_000_000,
            max_priority_fee_per_gas: 1_000_000_000,
            to: TxKind::Call(AlloyAddress::from(to)),
            value: U256::ZERO,
            access_list: Default::default(),
            input: Bytes::from(input),
        };
        let signature = signer.sign_hash_sync(&tx.signature_hash()).expect("sign tx");
        let mut raw = Vec::new();
        tx.into_signed(signature).encode_2718(&mut raw);
        raw
    }

    fn test_contracts() -> ContractAddresses {
        ContractAddresses {
            token: Address::from(TOKEN),
            channels: Address::from(CHANNELS),
            node_safe_registry: Address::from(REGISTRY),
            ..Default::default()
        }
    }

    fn network_policy() -> TransactionPolicy {
        TransactionPolicy::Whitelist(network_transaction_filter(&test_contracts()))
    }

    #[test]
    fn allow_all_accepts_nonempty() {
        assert!(TransactionPolicy::AllowAll.check(&[0x01, 0x02, 0x03]).is_ok());
    }

    #[test]
    fn allow_all_rejects_empty() {
        assert_eq!(TransactionPolicy::AllowAll.check(&[]), Err(FilterError::Empty));
    }

    #[test]
    fn network_filter_allows_token_approve() {
        let raw = signed_tx(TOKEN, approveCall::SELECTOR);
        assert!(network_policy().check(&raw).is_ok());
    }

    #[test]
    fn network_filter_allows_channels_safe_selector() {
        // The Safe-variant selector must be in the allow-set so unwrapped module calls match.
        let raw = signed_tx(CHANNELS, fundChannelSafeCall::SELECTOR);
        assert!(network_policy().check(&raw).is_ok());
    }

    #[test]
    fn network_filter_rejects_unknown_selector() {
        let raw = signed_tx(CHANNELS, [0xde, 0xad, 0xbe, 0xef]);
        assert!(matches!(
            network_policy().check(&raw),
            Err(FilterError::Unauthorized { .. })
        ));
    }

    #[test]
    fn network_filter_rejects_unknown_contract() {
        let raw = signed_tx([0x99; 20], approveCall::SELECTOR);
        assert!(matches!(
            network_policy().check(&raw),
            Err(FilterError::Unauthorized { .. })
        ));
    }
}
