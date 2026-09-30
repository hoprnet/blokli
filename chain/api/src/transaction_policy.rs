//! Policy controlling which raw transactions the executor will submit.
//!
//! The integration point for the stand-alone [`blokli_tx`] filtering crate.
//! [`TransactionPolicy::AllowAll`] accepts any non-empty transaction and is for tests and schema
//! export only. [`TransactionPolicy::Whitelist`] enforces an allow-set, built in production by
//! [`network_transaction_filter`] — so what is relayable is a property of the network, not of
//! operator configuration.

use blokli_chain_types::ContractAddresses;
use blokli_tx::{FilterError, FilteredTransaction, TransactionFilter};
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
/// Maps each HOPR contract to the selectors bloklid relays for it. A few entries are not obvious
/// from the list below:
///
/// - channel operations appear in both direct and `*Safe` variants, because the filter unwraps `execTransactionFromModule`
///   and matches the inner call;
/// - node announcements and Safe deployments ride on the token's `send`, so they need no entry;
/// - paid service registration arrives as a module delegate call into `MultiSend`, whose batched `(token, approve)` and
///   service-registry calls are matched individually;
/// - undeployed contracts carry the zero address and are dropped, so nothing authorizes calls to `0x0`.
///
/// Two relayable operations have no `(contract, selector)` pair and use the filter's escape
/// hatches instead:
///
/// - `SafePayloadGenerator::deregister_node_by_safe` targets the per-node management module directly, so its selector is
///   allowed on any target;
/// - both payload generators relay native xDAI with no calldata, so value transfers are permitted for any destination.
///   This does not make those destinations allowed contracts — anything carrying a selector is still matched normally.
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

    // Whitelisting an undeployed contract would authorize calls to `0x0`.
    TransactionFilter::from_pairs(
        allowed
            .into_iter()
            .filter(|(contract, _)| *contract != Address::default()),
    )
    .allowing_on_any_target([deregisterNodeBySafeCall::SELECTOR])
    .allowing_value_transfers()
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
    /// An empty payload is rejected in all modes. A whitelist returns the decoded sender and
    /// effective calls so the caller can record what it authorized; [`TransactionPolicy::AllowAll`]
    /// decodes nothing and returns `None`.
    ///
    /// # Errors
    /// Returns a [`FilterError`] when the payload is empty or the whitelist rejects it.
    pub fn check(&self, raw_tx: &[u8]) -> Result<Option<FilteredTransaction>, FilterError> {
        if raw_tx.is_empty() {
            return Err(FilterError::Empty);
        }

        match self {
            TransactionPolicy::AllowAll => Ok(None),
            TransactionPolicy::Whitelist(filter) => filter.filter_transaction(raw_tx).map(Some),
        }
    }
}

#[cfg(test)]
mod tests {
    use blokli_chain_types::ContractAddresses;
    use blokli_tx::FilterError;
    use curvy_bindings::curvy_aggregator_alpha_v2::CurvyAggregatorAlphaV2::submitWithdrawalRequestCall;
    use hopr_bindings::{
        constants::SAFE_MULTISEND_ADDRESS,
        exports::alloy::{
            consensus::{SignableTransaction, TxEip1559},
            eips::eip2718::Encodable2718,
            primitives::{Address as AlloyAddress, Bytes, TxKind, U256},
            signers::{SignerSync, local::PrivateKeySigner},
            sol,
            sol_types::SolCall,
        },
        hopr_channels::HoprChannels::fundChannelSafeCall,
        hopr_node_management_module::HoprNodeManagementModule::execTransactionFromModuleCall,
        hopr_node_safe_registry::HoprNodeSafeRegistry::{deregisterNodeBySafeCall, registerSafeByNodeCall},
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

    /// A Safe-wrapped native transfer: a module `Call` carrying the amount as `value` and no payload.
    fn signed_module_native_transfer(destination: [u8; 20]) -> Vec<u8> {
        let input = execTransactionFromModuleCall {
            to: AlloyAddress::from(destination),
            value: U256::from(1_000u64),
            data: Bytes::new(),
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
            to: SAFE_MULTISEND_ADDRESS,
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
    fn network_filter_allows_deregistration_on_the_per_node_module() {
        // `SafePayloadGenerator::deregister_node_by_safe` sends the call to the node's own
        // management module, whose address no allow-set can carry.
        let raw = signed_tx(MODULE, deregisterNodeBySafeCall::SELECTOR);
        assert!(network_policy().check(&raw).is_ok());
    }

    #[test]
    fn network_filter_rejects_other_selectors_on_the_per_node_module() {
        // Only `deregisterNodeBySafe` is matched without a target; the module is not whitelisted.
        let raw = signed_tx(MODULE, registerSafeByNodeCall::SELECTOR);
        assert!(matches!(
            network_policy().check(&raw),
            Err(FilterError::ContractNotAllowed { .. })
        ));
    }

    #[test]
    fn network_filter_allows_direct_native_transfer() {
        // `BasicPayloadGenerator::transfer::<XDai>` carries the amount as `value` and no calldata,
        // so there is no selector to match.
        let raw = sign([0xab; 20], Vec::new());
        assert!(network_policy().check(&raw).is_ok());
    }

    #[test]
    fn network_filter_allows_safe_wrapped_native_transfer() {
        // `SafePayloadGenerator::transfer::<XDai>` wraps an empty payload in a module `Call`.
        let raw = signed_module_native_transfer([0xab; 20]);
        assert!(network_policy().check(&raw).is_ok());
    }

    #[test]
    fn network_filter_rejects_unknown_calls_on_a_native_transfer_destination() {
        // Permitting value transfers must not turn their destination into an allowed contract.
        let raw = signed_tx([0xab; 20], approveCall::SELECTOR);
        assert!(matches!(
            network_policy().check(&raw),
            Err(FilterError::ContractNotAllowed { .. })
        ));
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
