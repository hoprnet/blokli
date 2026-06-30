//! Decoding and whitelist filtering of signed Ethereum transactions.

use std::collections::{HashMap, HashSet};

use hopr_bindings::exports::alloy::{
    consensus::{Transaction, TxEnvelope, TxType, transaction::SignerRecoverable},
    eips::eip2718::Decodable2718,
};
use hopr_types::primitive::{prelude::Address, traits::ToHex};

use crate::errors::{FilterError, Result};

/// A 4-byte Ethereum function selector (the first four bytes of the calldata).
pub type Selector = [u8; 4];

/// The decoded, authorized details of a transaction that passed the filter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FilteredTransaction {
    /// Address recovered from the transaction signature.
    pub sender: Address,
    /// Destination contract address (the transaction's `to` field).
    pub to: Address,
    /// 4-byte function selector extracted from the calldata.
    pub selector: Selector,
}

/// Filters signed Ethereum transactions against an injected whitelist of
/// `(sender, contract, selector)` triples.
///
/// A transaction is authorized only if the recovered sender, the destination
/// contract and the called function selector together form a triple present in
/// the whitelist. Keying on the contract in addition to the selector is
/// deliberate: 4-byte selectors are not unique across contracts (for example
/// `transfer`/`approve` exist on every ERC-20), so matching on the selector
/// alone would let an authorized sender invoke the same function on unintended
/// contracts.
#[derive(Debug, Clone, Default)]
pub struct TransactionFilter {
    /// Maps a `(sender, contract)` pair to the set of selectors it may call.
    whitelist: HashMap<(Address, Address), HashSet<Selector>>,
}

impl TransactionFilter {
    /// Create a filter from a pre-built whitelist map.
    pub fn new(whitelist: HashMap<(Address, Address), HashSet<Selector>>) -> Self {
        Self { whitelist }
    }

    /// Create a filter from an iterator of `(sender, contract, selector)` triples.
    pub fn from_triples(triples: impl IntoIterator<Item = (Address, Address, Selector)>) -> Self {
        let mut whitelist: HashMap<(Address, Address), HashSet<Selector>> = HashMap::new();
        for (sender, contract, selector) in triples {
            whitelist.entry((sender, contract)).or_default().insert(selector);
        }
        Self { whitelist }
    }

    /// Decode a raw signed transaction and verify it against the whitelist.
    ///
    /// On success the recovered sender, destination contract and function
    /// selector are returned in a [`FilteredTransaction`].
    ///
    /// # Errors
    /// Returns a [`FilterError`] describing why the transaction was rejected:
    /// empty input ([`FilterError::Empty`]), undecodable bytes
    /// ([`FilterError::Decode`]), an unsupported transaction type
    /// ([`FilterError::UnsupportedType`]), a contract-creation transaction
    /// ([`FilterError::ContractCreation`]), a signature that fails sender
    /// recovery ([`FilterError::SenderRecovery`]), calldata shorter than four
    /// bytes ([`FilterError::MissingSelector`]), or a triple that is not
    /// whitelisted ([`FilterError::Unauthorized`]).
    pub fn filter_transaction(&self, raw_tx: &[u8]) -> Result<FilteredTransaction> {
        if raw_tx.is_empty() {
            return Err(FilterError::Empty);
        }

        let envelope = TxEnvelope::decode_2718(&mut &raw_tx[..]).map_err(|e| FilterError::Decode(e.to_string()))?;

        match envelope.tx_type() {
            TxType::Legacy | TxType::Eip1559 => {}
            _ => return Err(FilterError::UnsupportedType),
        }

        // A missing recipient marks a contract creation, which is never allowed.
        let to = envelope.to().ok_or(FilterError::ContractCreation)?;

        let sender = envelope
            .recover_signer()
            .map_err(|e| FilterError::SenderRecovery(e.to_string()))?;

        let selector: Selector = envelope
            .input()
            .get(..4)
            .ok_or(FilterError::MissingSelector)?
            .try_into()
            .map_err(|_| FilterError::MissingSelector)?;

        let sender = Address::from(sender.into_array());
        let to = Address::from(to.into_array());

        let authorized = self
            .whitelist
            .get(&(sender, to))
            .is_some_and(|selectors| selectors.contains(&selector));

        if !authorized {
            return Err(FilterError::Unauthorized {
                sender: sender.to_hex(),
                contract: to.to_hex(),
                selector: format!("0x{}", hex::encode(selector)),
            });
        }

        Ok(FilteredTransaction { sender, to, selector })
    }
}

#[cfg(test)]
mod tests {
    use hopr_bindings::exports::alloy::{
        consensus::{SignableTransaction, TxEip1559, TxEip2930, TxLegacy},
        eips::eip2718::Encodable2718,
        primitives::{Address as AlloyAddress, Bytes, TxKind, U256},
        signers::{SignerSync, local::PrivateKeySigner},
    };

    use super::*;

    // Anvil well-known development keys, giving deterministic sender addresses.
    const KEY_A: &str = "ac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80";
    const KEY_B: &str = "59c6995e998f97a5a0044966f0945389dc9e86dae88c7a8412f4603b6b78690d";

    const CONTRACT: [u8; 20] = [0x11; 20];
    const OTHER_CONTRACT: [u8; 20] = [0x22; 20];
    const SELECTOR_TRANSFER: Selector = [0xa9, 0x05, 0x9c, 0xbb];
    const SELECTOR_APPROVE: Selector = [0x09, 0x5e, 0xa7, 0xb3];

    fn signer(key: &str) -> PrivateKeySigner {
        key.parse().expect("valid private key")
    }

    fn hopr_addr(alloy: AlloyAddress) -> Address {
        Address::from(alloy.into_array())
    }

    /// Calldata = selector followed by a 32-byte zero-padded argument.
    fn calldata(selector: Selector) -> Vec<u8> {
        let mut data = selector.to_vec();
        data.extend_from_slice(&[0u8; 32]);
        data
    }

    fn legacy_tx(signer: &PrivateKeySigner, to: TxKind, input: Vec<u8>) -> Vec<u8> {
        let tx = TxLegacy {
            chain_id: Some(1),
            nonce: 0,
            gas_price: 1_000_000_000,
            gas_limit: 21_000,
            to,
            value: U256::ZERO,
            input: Bytes::from(input),
        };
        let signature = signer.sign_hash_sync(&tx.signature_hash()).expect("sign legacy tx");
        let mut raw = Vec::new();
        tx.into_signed(signature).encode_2718(&mut raw);
        raw
    }

    fn eip1559_tx(signer: &PrivateKeySigner, to: TxKind, input: Vec<u8>) -> Vec<u8> {
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
        let signature = signer.sign_hash_sync(&tx.signature_hash()).expect("sign eip1559 tx");
        let mut raw = Vec::new();
        tx.into_signed(signature).encode_2718(&mut raw);
        raw
    }

    fn eip2930_tx(signer: &PrivateKeySigner, to: TxKind, input: Vec<u8>) -> Vec<u8> {
        let tx = TxEip2930 {
            chain_id: 1,
            nonce: 0,
            gas_price: 1_000_000_000,
            gas_limit: 21_000,
            to,
            value: U256::ZERO,
            access_list: Default::default(),
            input: Bytes::from(input),
        };
        let signature = signer.sign_hash_sync(&tx.signature_hash()).expect("sign eip2930 tx");
        let mut raw = Vec::new();
        tx.into_signed(signature).encode_2718(&mut raw);
        raw
    }

    /// A filter that authorizes `KEY_A` to call `transfer` on `CONTRACT`.
    fn filter_for(signer: &PrivateKeySigner) -> TransactionFilter {
        TransactionFilter::from_triples([(
            hopr_addr(signer.address()),
            hopr_addr(AlloyAddress::from(CONTRACT)),
            SELECTOR_TRANSFER,
        )])
    }

    #[test]
    fn legacy_authorized_passes() {
        let signer = signer(KEY_A);
        let filter = filter_for(&signer);
        let raw = legacy_tx(
            &signer,
            TxKind::Call(AlloyAddress::from(CONTRACT)),
            calldata(SELECTOR_TRANSFER),
        );

        let result = filter.filter_transaction(&raw).expect("authorized legacy tx");

        insta::assert_debug_snapshot!(result);
    }

    #[test]
    fn eip1559_authorized_passes() {
        let signer = signer(KEY_A);
        let filter = filter_for(&signer);
        let raw = eip1559_tx(
            &signer,
            TxKind::Call(AlloyAddress::from(CONTRACT)),
            calldata(SELECTOR_TRANSFER),
        );

        let result = filter.filter_transaction(&raw).expect("authorized eip1559 tx");

        insta::assert_debug_snapshot!(result);
    }

    #[test]
    fn unauthorized_sender_is_rejected() {
        let authorized = signer(KEY_A);
        let stranger = signer(KEY_B);
        let filter = filter_for(&authorized);
        let raw = eip1559_tx(
            &stranger,
            TxKind::Call(AlloyAddress::from(CONTRACT)),
            calldata(SELECTOR_TRANSFER),
        );

        let result = filter.filter_transaction(&raw);

        assert!(matches!(result, Err(FilterError::Unauthorized { .. })));
    }

    #[test]
    fn disallowed_selector_is_rejected() {
        let signer = signer(KEY_A);
        let filter = filter_for(&signer);
        let raw = eip1559_tx(
            &signer,
            TxKind::Call(AlloyAddress::from(CONTRACT)),
            calldata(SELECTOR_APPROVE),
        );

        let result = filter.filter_transaction(&raw);

        assert!(matches!(result, Err(FilterError::Unauthorized { .. })));
    }

    #[test]
    fn authorized_selector_on_wrong_contract_is_rejected() {
        let signer = signer(KEY_A);
        let filter = filter_for(&signer);
        let raw = eip1559_tx(
            &signer,
            TxKind::Call(AlloyAddress::from(OTHER_CONTRACT)),
            calldata(SELECTOR_TRANSFER),
        );

        let result = filter.filter_transaction(&raw);

        assert!(matches!(result, Err(FilterError::Unauthorized { .. })));
    }

    #[test]
    fn contract_creation_is_rejected() {
        let signer = signer(KEY_A);
        let filter = filter_for(&signer);
        let raw = eip1559_tx(&signer, TxKind::Create, calldata(SELECTOR_TRANSFER));

        let result = filter.filter_transaction(&raw);

        assert_eq!(result, Err(FilterError::ContractCreation));
    }

    #[test]
    fn calldata_shorter_than_selector_is_rejected() {
        let signer = signer(KEY_A);
        let filter = filter_for(&signer);
        let raw = eip1559_tx(&signer, TxKind::Call(AlloyAddress::from(CONTRACT)), vec![0x01, 0x02]);

        let result = filter.filter_transaction(&raw);

        assert_eq!(result, Err(FilterError::MissingSelector));
    }

    #[test]
    fn empty_calldata_is_rejected() {
        let signer = signer(KEY_A);
        let filter = filter_for(&signer);
        let raw = eip1559_tx(&signer, TxKind::Call(AlloyAddress::from(CONTRACT)), Vec::new());

        let result = filter.filter_transaction(&raw);

        assert_eq!(result, Err(FilterError::MissingSelector));
    }

    #[test]
    fn empty_input_is_rejected() {
        let filter = TransactionFilter::default();

        let result = filter.filter_transaction(&[]);

        assert_eq!(result, Err(FilterError::Empty));
    }

    #[test]
    fn malformed_bytes_are_rejected() {
        let filter = TransactionFilter::default();

        let result = filter.filter_transaction(&[0xde, 0xad, 0xbe, 0xef]);

        assert!(matches!(result, Err(FilterError::Decode(_))));
    }

    #[test]
    fn eip2930_type_is_unsupported() {
        let signer = signer(KEY_A);
        let filter = filter_for(&signer);
        let raw = eip2930_tx(
            &signer,
            TxKind::Call(AlloyAddress::from(CONTRACT)),
            calldata(SELECTOR_TRANSFER),
        );

        let result = filter.filter_transaction(&raw);

        assert_eq!(result, Err(FilterError::UnsupportedType));
    }
}
