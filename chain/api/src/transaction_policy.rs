//! Policy controlling which raw transactions the executor will submit.
//!
//! The policy is the integration point for the stand-alone [`blokli_tx`] filtering crate.
//! [`TransactionPolicy::AllowAll`] accepts any non-empty transaction (used by the standalone API
//! stubs and tests). [`TransactionPolicy::Whitelist`] enforces a `(contract, selector)` allow-set;
//! in production it is built by [`network_transaction_filter`] from the network's contract addresses,
//! so the set of relayable HOPR operations is a property of the network, not operator configuration.

use blokli_chain_types::ContractAddresses;
use blokli_tx::{FilterError, TransactionFilter};
use curvy_bindings::curvy_aggregator_alpha_v2::CurvyAggregatorAlphaV2::submitWithdrawalRequestCall;
use hopr_bindings::{
    exports::alloy::sol_types::SolCall,
    hopr_channels::HoprChannels::{
        closeIncomingChannelCall, closeIncomingChannelSafeCall, finalizeOutgoingChannelClosureCall,
        finalizeOutgoingChannelClosureSafeCall, fundChannelCall, fundChannelSafeCall,
        initiateOutgoingChannelClosureCall, initiateOutgoingChannelClosureSafeCall, redeemTicketCall,
        redeemTicketSafeCall,
    },
    hopr_node_safe_registry::HoprNodeSafeRegistry::{deregisterNodeBySafeCall, registerSafeByNodeCall},
    hopr_service_registry::HoprServiceRegistry::{
        registerServiceTypeCall, selfDeregisterCall, selfRegisterCall, selfUpdateCall,
    },
    hopr_ticket_price_oracle::HoprTicketPriceOracle::setTicketPriceCall,
    hopr_token::HoprToken::{approveCall, sendCall, transferCall},
    hopr_winning_probability_oracle::HoprWinningProbabilityOracle::setWinProbCall,
};
use hopr_types::primitive::prelude::Address;

/// Build the network transaction allow-set from its contract addresses.
///
/// Maps each HOPR contract to the function selectors bloklid relays for it. Channel operations are
/// included in both their direct and Safe-module (`*Safe`) variants, since the filter unwraps
/// `execTransactionFromModule` and matches the inner call. Token `approve`/`transfer`/`send` and the
/// safe-registry and service-registry operations cover the remaining relayable calls; node
/// announcements and Safe deployments are relayed as `send` on the token contract, so they need no
/// entry of their own. Paid service registration and updates arrive as a module delegate call into
/// the canonical `MultiSend`, whose batched `(token, approve)` and service-registry calls are
/// matched individually. When Curvy is configured, its aggregator's withdrawal submission
/// entrypoint is included as well. The network winning-probability and ticket-price update
/// entrypoints are also relayed for ticket parameter updates. The integration network's xHOPR
/// ERC-677 token accepts standard `transfer` calls.
///
/// Contracts that the network does not deploy carry the zero address; their pairs are dropped so
/// the allow-set never authorizes calls to `0x0`.
///
/// One relayable operation cannot be expressed here: `SafePayloadGenerator::deregister_node_by_safe`
/// sends `deregisterNodeBySafe` straight to the per-node management module rather than wrapping it,
/// and module addresses are per-node, so the filter cannot match it. The entry below covers the
/// registry-targeted form of that call only.
pub fn network_transaction_filter(contracts: &ContractAddresses) -> TransactionFilter {
    let token = contracts.token;
    let xhopr_token = contracts.xhopr_token;
    let channels = contracts.channels;
    let registry = contracts.node_safe_registry;
    let service_registry = contracts.service_registry;
    let winning_probability_oracle = contracts.winning_probability_oracle;
    let ticket_price_oracle = contracts.ticket_price_oracle;
    let curvy_aggregator = contracts.curvy_aggregator;

    let allowed = [
        (token, approveCall::SELECTOR),
        (token, transferCall::SELECTOR),
        (token, sendCall::SELECTOR),
        (xhopr_token, transferCall::SELECTOR),
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
        (service_registry, registerServiceTypeCall::SELECTOR),
        (service_registry, selfRegisterCall::SELECTOR),
        (service_registry, selfUpdateCall::SELECTOR),
        (service_registry, selfDeregisterCall::SELECTOR),
        (winning_probability_oracle, setWinProbCall::SELECTOR),
        (ticket_price_oracle, setTicketPriceCall::SELECTOR),
        (curvy_aggregator, submitWithdrawalRequestCall::SELECTOR),
    ];

    // Contracts a network does not deploy are left at the zero address; whitelisting them would
    // authorize calls to `0x0`.
    TransactionFilter::from_pairs(
        allowed
            .into_iter()
            .filter(|(contract, _)| *contract != Address::default()),
    )
}

/// Decides whether a raw signed transaction may be submitted to the chain.
///
/// Deliberately has no [`Default`]: an accidental default must not silently disable the allow-set,
/// so every construction site states which policy it wants.
#[derive(Debug, Clone)]
pub enum TransactionPolicy {
    /// Accept any non-empty transaction; no allow-set enforcement.
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
    use blokli_tx::FilterError;
    use curvy_bindings::curvy_aggregator_alpha_v2::CurvyAggregatorAlphaV2::submitWithdrawalRequestCall;
    use hopr_bindings::{
        exports::alloy::{
            consensus::{SignableTransaction, TxEip1559},
            eips::eip2718::Encodable2718,
            primitives::{Address as AlloyAddress, Bytes, TxKind, U256},
            signers::{SignerSync, local::PrivateKeySigner},
            sol,
            sol_types::SolCall,
        },
        hopr_channels::HoprChannels::fundChannelSafeCall,
        hopr_node_safe_registry::HoprNodeSafeRegistry::registerSafeByNodeCall,
        hopr_service_registry::HoprServiceRegistry::{
            registerServiceTypeCall, selfDeregisterCall, selfRegisterCall, selfUpdateCall,
        },
        hopr_ticket_price_oracle::HoprTicketPriceOracle::setTicketPriceCall,
        hopr_token::HoprToken::{approveCall, transferCall},
        hopr_winning_probability_oracle::HoprWinningProbabilityOracle::setWinProbCall,
    };
    use hopr_types::primitive::prelude::Address;

    use crate::transaction_policy::{TransactionPolicy, network_transaction_filter};

    sol! {
        function execTransactionFromModule(address to, uint256 value, bytes data, uint8 operation) external returns (bool);
        function multiSend(bytes transactions) external payable;
    }

    const KEY: &str = "ac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80";
    const TOKEN: [u8; 20] = [0x11; 20];
    const XHOPR_TOKEN: [u8; 20] = [0x77; 20];
    const CHANNELS: [u8; 20] = [0x22; 20];
    const REGISTRY: [u8; 20] = [0x33; 20];
    const CURVY_AGGREGATOR: [u8; 20] = [0x44; 20];
    const SERVICE_REGISTRY: [u8; 20] = [0x55; 20];
    const WINNING_PROBABILITY_ORACLE: [u8; 20] = [0x66; 20];
    const TICKET_PRICE_ORACLE: [u8; 20] = [0x88; 20];
    /// Per-node Safe management module; never part of the allow-set, only an unwrapping target.
    const MODULE: [u8; 20] = [0x99; 20];
    /// Canonical Gnosis Safe `MultiSend` singleton, the only permitted delegate-call target.
    const SAFE_MULTI_SEND: [u8; 20] = [
        0x38, 0x86, 0x9b, 0xf6, 0x6a, 0x61, 0xcf, 0x6b, 0xdb, 0x99, 0x6a, 0x6a, 0xe4, 0x0d, 0x58, 0x53, 0xfd, 0x43,
        0xb5, 0x26,
    ];

    fn calldata(selector: [u8; 4]) -> Vec<u8> {
        let mut input = selector.to_vec();
        input.extend_from_slice(&[0u8; 32]);
        input
    }

    fn sign(to: [u8; 20], input: Vec<u8>) -> Vec<u8> {
        let signer: PrivateKeySigner = KEY.parse().expect("valid private key");
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

    /// A direct transaction to `to` calling `selector`.
    fn signed_tx(to: [u8; 20], selector: [u8; 4]) -> Vec<u8> {
        sign(to, calldata(selector))
    }

    /// A transaction to the per-node module wrapping an inner call, as `SafePayloadGenerator` emits.
    fn signed_module_tx(inner_to: [u8; 20], selector: [u8; 4]) -> Vec<u8> {
        let input = execTransactionFromModuleCall {
            to: AlloyAddress::from(inner_to),
            value: U256::ZERO,
            data: Bytes::from(calldata(selector)),
            operation: 0,
        }
        .abi_encode();

        sign(MODULE, input)
    }

    /// A paid service-registry transaction: a module delegate call into `MultiSend` batching an
    /// allowance on the token and the service-registry call itself.
    fn signed_paid_service_tx(registry_selector: [u8; 4]) -> Vec<u8> {
        fn entry(to: [u8; 20], data: &[u8]) -> Vec<u8> {
            let mut encoded = Vec::with_capacity(85 + data.len());
            encoded.push(0); // operation: Call
            encoded.extend_from_slice(&to);
            encoded.extend_from_slice(&[0u8; 32]);
            encoded.extend_from_slice(&U256::from(data.len()).to_be_bytes::<32>());
            encoded.extend_from_slice(data);
            encoded
        }

        let mut transactions = entry(TOKEN, &calldata(approveCall::SELECTOR));
        transactions.extend(entry(SERVICE_REGISTRY, &calldata(registry_selector)));

        let multi_send = multiSendCall {
            transactions: Bytes::from(transactions),
        }
        .abi_encode();

        let input = execTransactionFromModuleCall {
            to: AlloyAddress::from(SAFE_MULTI_SEND),
            value: U256::ZERO,
            data: Bytes::from(multi_send),
            operation: 1, // DelegateCall, as the Safe payload generator emits for MultiSend
        }
        .abi_encode();

        sign(MODULE, input)
    }

    fn test_contracts() -> ContractAddresses {
        ContractAddresses {
            token: Address::from(TOKEN),
            xhopr_token: Address::from(XHOPR_TOKEN),
            channels: Address::from(CHANNELS),
            node_safe_registry: Address::from(REGISTRY),
            curvy_aggregator: Address::from(CURVY_AGGREGATOR),
            service_registry: Address::from(SERVICE_REGISTRY),
            winning_probability_oracle: Address::from(WINNING_PROBABILITY_ORACLE),
            ticket_price_oracle: Address::from(TICKET_PRICE_ORACLE),
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
    fn network_filter_allows_xhopr_transfer() {
        let raw = signed_tx(XHOPR_TOKEN, transferCall::SELECTOR);
        assert!(network_policy().check(&raw).is_ok());
    }

    #[test]
    fn network_filter_allows_safe_wrapped_channel_operation() {
        // As `SafePayloadGenerator` emits it: outer call to the per-node module, inner call to
        // `channels` with the `*Safe` selector.
        let raw = signed_module_tx(CHANNELS, fundChannelSafeCall::SELECTOR);
        assert!(network_policy().check(&raw).is_ok());
    }

    #[test]
    fn network_filter_allows_safe_registration() {
        let raw = signed_tx(REGISTRY, registerSafeByNodeCall::SELECTOR);
        assert!(network_policy().check(&raw).is_ok());
    }

    #[test]
    fn network_filter_allows_curvy_withdrawal_submission() {
        let raw = signed_tx(CURVY_AGGREGATOR, submitWithdrawalRequestCall::SELECTOR);
        assert!(network_policy().check(&raw).is_ok());
    }

    #[test]
    fn network_filter_allows_service_registry_operations() {
        for selector in [
            registerServiceTypeCall::SELECTOR,
            selfRegisterCall::SELECTOR,
            selfUpdateCall::SELECTOR,
            selfDeregisterCall::SELECTOR,
        ] {
            let raw = signed_tx(SERVICE_REGISTRY, selector);
            assert!(network_policy().check(&raw).is_ok());
        }
    }

    #[test]
    fn network_filter_allows_paid_service_registration_batch() {
        // Paid service writes are relayed as a module delegate call into `MultiSend`.
        for selector in [selfRegisterCall::SELECTOR, selfUpdateCall::SELECTOR] {
            let raw = signed_paid_service_tx(selector);
            assert!(network_policy().check(&raw).is_ok());
        }
    }

    #[test]
    fn network_filter_rejects_paid_service_batch_with_unknown_target() {
        let policy = TransactionPolicy::Whitelist(network_transaction_filter(&ContractAddresses {
            // Without a service registry, the batched registry call has no allowed pair to match.
            service_registry: Address::default(),
            ..test_contracts()
        }));
        let raw = signed_paid_service_tx(selfRegisterCall::SELECTOR);

        assert!(matches!(
            policy.check(&raw),
            Err(FilterError::ContractNotAllowed { .. })
        ));
    }

    #[test]
    fn network_filter_allows_winning_probability_update() {
        let raw = signed_tx(WINNING_PROBABILITY_ORACLE, setWinProbCall::SELECTOR);
        assert!(network_policy().check(&raw).is_ok());
    }

    #[test]
    fn network_filter_allows_ticket_price_update() {
        let raw = signed_tx(TICKET_PRICE_ORACLE, setTicketPriceCall::SELECTOR);
        assert!(network_policy().check(&raw).is_ok());
    }

    #[test]
    fn network_filter_rejects_curvy_withdrawal_when_curvy_is_not_configured() {
        let mut contracts = test_contracts();
        contracts.curvy_aggregator = Address::default();
        let policy = TransactionPolicy::Whitelist(network_transaction_filter(&contracts));
        let raw = signed_tx(CURVY_AGGREGATOR, submitWithdrawalRequestCall::SELECTOR);

        assert!(matches!(
            policy.check(&raw),
            Err(FilterError::ContractNotAllowed { .. })
        ));
    }

    #[test]
    fn network_filter_never_whitelists_undeployed_contracts() {
        // Undeployed contracts carry the zero address; nothing may be relayed to it.
        let mut contracts = test_contracts();
        contracts.xhopr_token = Address::default();
        contracts.service_registry = Address::default();
        let policy = TransactionPolicy::Whitelist(network_transaction_filter(&contracts));

        for selector in [transferCall::SELECTOR, selfRegisterCall::SELECTOR] {
            let raw = signed_tx([0u8; 20], selector);
            assert!(matches!(
                policy.check(&raw),
                Err(FilterError::ContractNotAllowed { .. })
            ));
        }
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
        let raw = signed_tx([0x9a; 20], approveCall::SELECTOR);
        assert!(matches!(
            network_policy().check(&raw),
            Err(FilterError::ContractNotAllowed { .. })
        ));
    }
}
