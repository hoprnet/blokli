//! Decoding and allow-set filtering of signed Ethereum transactions.

use std::collections::{HashMap, HashSet};

use alloy_sol_types::{SolCall, sol};
use hopr_bindings::exports::alloy::{
    consensus::{Transaction, TxEnvelope, TxType, transaction::SignerRecoverable},
    eips::eip2718::Decodable2718,
};
use hopr_types::primitive::{prelude::Address, traits::ToHex};

use crate::errors::{FilterError, Result};

sol! {
    /// Standard Gnosis Safe module execution entrypoint.
    ///
    /// HOPR node operations are relayed wrapped in this call: the transaction's `to` is the node's
    /// own management module and the real contract call is carried in `data`. The filter unwraps it
    /// to validate the inner `(to, selector)` against the allow-set.
    function execTransactionFromModule(address to, uint256 value, bytes data, uint8 operation) external returns (bool);
}

/// A 4-byte Ethereum function selector (the first four bytes of the calldata).
pub type Selector = [u8; 4];

/// The decoded, authorized details of a transaction that passed the filter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FilteredTransaction {
    /// Address recovered from the transaction signature.
    pub sender: Address,
    /// Effective target contract — the inner target when unwrapped from a Safe-module call.
    pub to: Address,
    /// Effective 4-byte function selector — the inner selector when unwrapped.
    pub selector: Selector,
    /// Whether the call was unwrapped from a Safe-module `execTransactionFromModule` call.
    pub via_module: bool,
}

/// Filters signed Ethereum transactions against an allow-set of `(contract, selector)` pairs.
///
/// A transaction is authorized only if its effective destination contract and called function
/// selector form a pair present in the allow-set. When the transaction is a Safe-module
/// `execTransactionFromModule` call, the inner call is decoded and its `(to, selector)` is used as
/// the effective pair (the outer module address is not matched, as it is per-node). Plain
/// `DelegateCall` module operations are always rejected.
///
/// The sender is recovered to reject contract-creation and malformed transactions and is surfaced
/// in [`FilteredTransaction`] for logging, but it is not part of the matching key.
#[derive(Debug, Clone, Default)]
pub struct TransactionFilter {
    /// Maps a target contract to the set of selectors permitted on it.
    allowed: HashMap<Address, HashSet<Selector>>,
}

impl TransactionFilter {
    /// Create a filter from a pre-built allow-set map.
    pub fn new(allowed: HashMap<Address, HashSet<Selector>>) -> Self {
        Self { allowed }
    }

    /// Create a filter from an iterator of `(contract, selector)` pairs.
    pub fn from_pairs(pairs: impl IntoIterator<Item = (Address, Selector)>) -> Self {
        let mut allowed: HashMap<Address, HashSet<Selector>> = HashMap::new();
        for (contract, selector) in pairs {
            allowed.entry(contract).or_default().insert(selector);
        }
        Self { allowed }
    }

    /// Decode a raw signed transaction and verify it against the allow-set.
    ///
    /// # Errors
    /// Returns a [`FilterError`] describing why the transaction was rejected: empty input, undecodable
    /// bytes, an unsupported transaction type, a contract-creation transaction, a signature that fails
    /// sender recovery, calldata shorter than four bytes, a Safe-module call that fails to decode or
    /// requests a delegate call, or an effective `(contract, selector)` pair that is not allowed.
    pub fn filter_transaction(&self, raw_tx: &[u8]) -> Result<FilteredTransaction> {
        if raw_tx.is_empty() {
            return Err(FilterError::Empty);
        }

        let envelope = TxEnvelope::decode_2718(&mut &raw_tx[..]).map_err(|e| FilterError::Decode(e.to_string()))?;

        match envelope.tx_type() {
            TxType::Legacy | TxType::Eip1559 => {}
            _ => return Err(FilterError::UnsupportedType),
        }

        let outer_to = envelope.to().ok_or(FilterError::ContractCreation)?;
        let sender = envelope
            .recover_signer()
            .map_err(|e| FilterError::SenderRecovery(e.to_string()))?;

        let input = envelope.input();
        let outer_selector = selector_of(&input[..])?;

        // Unwrap Safe-module calls and match on the inner target; match other calls directly.
        let (to, selector, via_module) = if outer_selector == execTransactionFromModuleCall::SELECTOR {
            let call = execTransactionFromModuleCall::abi_decode(&input[..])
                .map_err(|e| FilterError::ModuleUnwrap(e.to_string()))?;
            // Operation 0 = Call, 1 = DelegateCall. Only plain calls are relayed.
            if call.operation != 0 {
                return Err(FilterError::DelegateCallNotAllowed);
            }
            (call.to, selector_of(&call.data[..])?, true)
        } else {
            (outer_to, outer_selector, false)
        };

        let sender = Address::from(sender.into_array());
        let to = Address::from(to.into_array());

        let authorized = self
            .allowed
            .get(&to)
            .is_some_and(|selectors| selectors.contains(&selector));

        if !authorized {
            return Err(FilterError::Unauthorized {
                contract: to.to_hex(),
                selector: format!("0x{}", hex::encode(selector)),
            });
        }

        Ok(FilteredTransaction {
            sender,
            to,
            selector,
            via_module,
        })
    }
}

/// Extract the leading 4-byte selector from calldata, or [`FilterError::MissingSelector`].
fn selector_of(input: &[u8]) -> Result<Selector> {
    input
        .get(..4)
        .ok_or(FilterError::MissingSelector)?
        .try_into()
        .map_err(|_| FilterError::MissingSelector)
}

#[cfg(test)]
mod tests {
    use hopr_bindings::exports::alloy::{
        consensus::{SignableTransaction, TxEip1559, TxEip2930},
        eips::eip2718::Encodable2718,
        primitives::{Address as AlloyAddress, Bytes, TxKind, U256},
        signers::{SignerSync, local::PrivateKeySigner},
    };
    use hopr_types::primitive::prelude::Address;

    use super::*;

    const KEY: &str = "ac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80";
    const CONTRACT: [u8; 20] = [0x11; 20];
    const MODULE: [u8; 20] = [0x22; 20];
    const OTHER: [u8; 20] = [0x33; 20];
    const SELECTOR_APPROVE: Selector = [0x09, 0x5e, 0xa7, 0xb3];
    const SELECTOR_TRANSFER: Selector = [0xa9, 0x05, 0x9c, 0xbb];

    fn signer() -> PrivateKeySigner {
        KEY.parse().expect("valid private key")
    }

    fn hopr_addr(raw: [u8; 20]) -> Address {
        Address::from(raw)
    }

    fn calldata(selector: Selector) -> Vec<u8> {
        let mut data = selector.to_vec();
        data.extend_from_slice(&[0u8; 32]);
        data
    }

    /// Sign and 2718-encode an EIP-1559 transaction with the given recipient and calldata.
    fn signed_tx(to: TxKind, input: Vec<u8>) -> Vec<u8> {
        let tx = TxEip1559 {
            chain_id: 1,
            nonce: 0,
            gas_limit: 21_000,
            max_fee_per_gas: 1_000_000_000,
            max_priority_fee_per_gas: 1_000_000_000,
            to,
            value: U256::ZERO,
            access_list: Default::default(),
            input: Bytes::from(input),
        };
        let signature = signer().sign_hash_sync(&tx.signature_hash()).expect("sign tx");
        let mut raw = Vec::new();
        tx.into_signed(signature).encode_2718(&mut raw);
        raw
    }

    /// Build the calldata for a Safe-module `execTransactionFromModule` wrapping the given inner call.
    fn module_calldata(inner_to: [u8; 20], inner: Vec<u8>, operation: u8) -> Vec<u8> {
        execTransactionFromModuleCall {
            to: AlloyAddress::from(inner_to),
            value: U256::ZERO,
            data: Bytes::from(inner),
            operation,
        }
        .abi_encode()
    }

    /// A filter allowing `transfer`/`approve` on `CONTRACT`.
    fn filter() -> TransactionFilter {
        TransactionFilter::from_pairs([
            (hopr_addr(CONTRACT), SELECTOR_TRANSFER),
            (hopr_addr(CONTRACT), SELECTOR_APPROVE),
        ])
    }

    #[test]
    fn direct_allowed_call_passes() {
        let raw = signed_tx(TxKind::Call(AlloyAddress::from(CONTRACT)), calldata(SELECTOR_TRANSFER));
        let result = filter().filter_transaction(&raw).expect("authorized direct call");
        insta::assert_debug_snapshot!(result);
    }

    #[test]
    fn safe_wrapped_call_passes_on_inner_target() {
        // Outer call targets the per-node module; inner call targets the allowed contract.
        let inner = calldata(SELECTOR_APPROVE);
        let outer = module_calldata(CONTRACT, inner, 0);
        let raw = signed_tx(TxKind::Call(AlloyAddress::from(MODULE)), outer);
        let result = filter().filter_transaction(&raw).expect("authorized safe-wrapped call");
        insta::assert_debug_snapshot!(result);
    }

    #[test]
    fn direct_disallowed_selector_is_rejected() {
        let raw = signed_tx(
            TxKind::Call(AlloyAddress::from(CONTRACT)),
            calldata([0xde, 0xad, 0xbe, 0xef]),
        );
        assert!(matches!(
            filter().filter_transaction(&raw),
            Err(FilterError::Unauthorized { .. })
        ));
    }

    #[test]
    fn call_to_non_allowed_contract_is_rejected() {
        let raw = signed_tx(TxKind::Call(AlloyAddress::from(OTHER)), calldata(SELECTOR_TRANSFER));
        assert!(matches!(
            filter().filter_transaction(&raw),
            Err(FilterError::Unauthorized { .. })
        ));
    }

    #[test]
    fn safe_wrapped_call_to_non_allowed_inner_target_is_rejected() {
        let inner = calldata(SELECTOR_TRANSFER);
        let outer = module_calldata(OTHER, inner, 0);
        let raw = signed_tx(TxKind::Call(AlloyAddress::from(MODULE)), outer);
        assert!(matches!(
            filter().filter_transaction(&raw),
            Err(FilterError::Unauthorized { .. })
        ));
    }

    #[test]
    fn safe_wrapped_delegate_call_is_rejected() {
        let inner = calldata(SELECTOR_TRANSFER);
        let outer = module_calldata(CONTRACT, inner, 1); // operation 1 = DelegateCall
        let raw = signed_tx(TxKind::Call(AlloyAddress::from(MODULE)), outer);
        assert_eq!(
            filter().filter_transaction(&raw),
            Err(FilterError::DelegateCallNotAllowed)
        );
    }

    #[test]
    fn contract_creation_is_rejected() {
        let raw = signed_tx(TxKind::Create, calldata(SELECTOR_TRANSFER));
        assert_eq!(filter().filter_transaction(&raw), Err(FilterError::ContractCreation));
    }

    #[test]
    fn short_calldata_is_rejected() {
        let raw = signed_tx(TxKind::Call(AlloyAddress::from(CONTRACT)), vec![0x01, 0x02]);
        assert_eq!(filter().filter_transaction(&raw), Err(FilterError::MissingSelector));
    }

    #[test]
    fn empty_input_is_rejected() {
        assert_eq!(
            TransactionFilter::default().filter_transaction(&[]),
            Err(FilterError::Empty)
        );
    }

    #[test]
    fn malformed_bytes_are_rejected() {
        assert!(matches!(
            TransactionFilter::default().filter_transaction(&[0xde, 0xad, 0xbe, 0xef]),
            Err(FilterError::Decode(_))
        ));
    }

    #[test]
    fn eip2930_type_is_unsupported() {
        let tx = TxEip2930 {
            chain_id: 1,
            nonce: 0,
            gas_price: 1_000_000_000,
            gas_limit: 21_000,
            to: TxKind::Call(AlloyAddress::from(CONTRACT)),
            value: U256::ZERO,
            access_list: Default::default(),
            input: Bytes::from(calldata(SELECTOR_TRANSFER)),
        };
        let signature = signer().sign_hash_sync(&tx.signature_hash()).expect("sign tx");
        let mut raw = Vec::new();
        tx.into_signed(signature).encode_2718(&mut raw);
        assert_eq!(filter().filter_transaction(&raw), Err(FilterError::UnsupportedType));
    }
}
