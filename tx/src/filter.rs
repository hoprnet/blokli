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

    /// Gnosis Safe `MultiSend` batch entrypoint, invoked as a delegate call.
    ///
    /// `transactions` is the tightly packed concatenation of the batched calls; see
    /// [`decode_multi_send`] for the per-entry layout.
    function multiSend(bytes transactions) external payable;
}

/// Canonical Gnosis Safe `MultiSend` singleton, deployed at the same address on every chain.
///
/// A module `DelegateCall` is only unwrapped when it targets this address: it is the one delegate
/// target whose bytecode is known to merely replay the batched calls, so authorizing the batch
/// entries is equivalent to authorizing what the chain will execute.
const SAFE_MULTI_SEND_ADDRESS: [u8; 20] = [
    0x38, 0x86, 0x9b, 0xf6, 0x6a, 0x61, 0xcf, 0x6b, 0xdb, 0x99, 0x6a, 0x6a, 0xe4, 0x0d, 0x58, 0x53, 0xfd, 0x43, 0xb5,
    0x26,
];

/// Operation code for a plain `CALL` in Safe module and `MultiSend` payloads.
const OPERATION_CALL: u8 = 0;
/// Operation code for a `DELEGATECALL` in Safe module and `MultiSend` payloads.
const OPERATION_DELEGATE_CALL: u8 = 1;

/// Byte length of a `MultiSend` entry header: operation, target, value and data length.
const MULTI_SEND_HEADER_LEN: usize = 1 + 20 + 32 + 32;

/// A 4-byte Ethereum function selector (the first four bytes of the calldata).
pub type Selector = [u8; 4];

/// A single effective contract call carried by a transaction.
///
/// A plain transaction and a Safe-module call yield exactly one of these; a `MultiSend` batch
/// yields one per batched call. Every call must be in the allow-set for the transaction to pass.
///
/// # Example
///
/// ```
/// use blokli_tx::AuthorizedCall;
/// use hopr_types::primitive::prelude::Address;
///
/// // `filter_transaction` reports one of these per effective call it authorized.
/// let call = AuthorizedCall {
///     to: Address::from([0x02u8; 20]),
///     selector: [0x09, 0x5e, 0xa7, 0xb3], // ERC-20 `approve`
/// };
///
/// assert_eq!(call.selector, [0x09, 0x5e, 0xa7, 0xb3]);
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AuthorizedCall {
    /// Effective target contract of the call.
    pub to: Address,
    /// Effective 4-byte function selector of the call.
    pub selector: Selector,
}

/// The decoded, authorized details of a transaction that passed the filter.
///
/// # Example
///
/// ```
/// use blokli_tx::{FilterError, TransactionFilter};
///
/// // Nothing is authorized by an empty filter, so no `FilteredTransaction` is ever produced.
/// assert_eq!(
///     TransactionFilter::default().filter_transaction(&[]),
///     Err(FilterError::Empty)
/// );
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FilteredTransaction {
    /// Address recovered from the transaction signature.
    pub sender: Address,
    /// The effective calls the transaction performs, all of them authorized.
    pub calls: Vec<AuthorizedCall>,
    /// Whether the calls were unwrapped from a Safe-module `execTransactionFromModule` call.
    pub via_module: bool,
}

/// Filters signed Ethereum transactions against an allow-set of `(contract, selector)` pairs.
///
/// A transaction is authorized only if every effective call it performs matches a
/// `(contract, selector)` pair present in the allow-set. The effective calls are:
///
/// - a plain transaction: its own `(to, selector)`;
/// - a Safe-module `execTransactionFromModule` call with `operation = Call`: the inner `(to, selector)` (the outer
///   module address is not matched, as it is per-node);
/// - a Safe-module call with `operation = DelegateCall` targeting the canonical `MultiSend` singleton: every `(to,
///   selector)` in the batch, each of which must itself be a plain call.
///
/// Any other delegate call is rejected.
///
/// The sender is recovered to reject contract-creation and malformed transactions and is surfaced
/// in [`FilteredTransaction`] for logging, but it is not part of the matching key.
///
/// # Trust assumptions
///
/// Safe-module unwrapping is keyed on the outer 4-byte selector alone: the filter has no source of
/// truth for which addresses are genuine HOPR node modules (they are per-node and discovered by
/// indexing), so it cannot verify that the outer target really is one. A contract that exposes the
/// same `execTransactionFromModule` ABI is therefore unwrapped like a module, and authorization is
/// decided on the inner call while the chain executes the outer one. Verifying the outer target
/// against the indexed node modules is tracked as follow-up work.
///
/// For the same reason, operations that a Safe payload generator sends *directly* to the per-node
/// module — notably `deregisterNodeBySafe` — cannot be matched by a static allow-set and are
/// rejected.
///
/// # Example
///
/// ```
/// use blokli_tx::TransactionFilter;
/// use hopr_types::primitive::prelude::Address;
///
/// let token = Address::from([0x02u8; 20]);
/// let approve = [0x09, 0x5e, 0xa7, 0xb3]; // ERC-20 `approve`
/// let filter = TransactionFilter::from_pairs([(token, approve)]);
///
/// assert!(filter.filter_transaction(&[]).is_err());
/// ```
#[derive(Debug, Clone, Default)]
pub struct TransactionFilter {
    /// Maps a target contract to the set of selectors permitted on it.
    allowed: HashMap<Address, HashSet<Selector>>,
}

impl TransactionFilter {
    /// Create a filter from a pre-built allow-set map.
    ///
    /// # Example
    ///
    /// ```
    /// use std::collections::{HashMap, HashSet};
    ///
    /// use blokli_tx::TransactionFilter;
    /// use hopr_types::primitive::prelude::Address;
    ///
    /// let mut allowed = HashMap::new();
    /// allowed.insert(Address::from([0x02u8; 20]), HashSet::from([[0x09, 0x5e, 0xa7, 0xb3]]));
    ///
    /// let filter = TransactionFilter::new(allowed);
    /// assert!(filter.filter_transaction(&[]).is_err());
    /// ```
    pub fn new(allowed: HashMap<Address, HashSet<Selector>>) -> Self {
        Self { allowed }
    }

    /// Create a filter from an iterator of `(contract, selector)` pairs.
    ///
    /// # Example
    ///
    /// ```
    /// use blokli_tx::TransactionFilter;
    /// use hopr_types::primitive::prelude::Address;
    ///
    /// let filter = TransactionFilter::from_pairs([(Address::from([0x02u8; 20]), [0xa9, 0x05, 0x9c, 0xbb])]);
    /// assert!(filter.filter_transaction(&[]).is_err());
    /// ```
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
    /// bytes, trailing bytes after the envelope, an unsupported transaction type, a contract-creation
    /// transaction, a signature that fails sender recovery, calldata shorter than four bytes, a
    /// Safe-module call that fails to decode or requests an unsupported delegate call, a malformed
    /// `MultiSend` batch, or an effective call whose contract or selector is not allowed.
    ///
    /// # Example
    ///
    /// ```
    /// use blokli_tx::{FilterError, TransactionFilter};
    ///
    /// let filter = TransactionFilter::default();
    /// assert_eq!(filter.filter_transaction(&[]), Err(FilterError::Empty));
    /// assert!(matches!(
    ///     filter.filter_transaction(&[0xde, 0xad, 0xbe, 0xef]),
    ///     Err(FilterError::Decode(_))
    /// ));
    /// ```
    pub fn filter_transaction(&self, raw_tx: &[u8]) -> Result<FilteredTransaction> {
        if raw_tx.is_empty() {
            return Err(FilterError::Empty);
        }

        // `decode_2718` consumes a single envelope from the slice: anything left over is not part of
        // the transaction and must not be forwarded to the RPC.
        let mut remaining = raw_tx;
        let envelope = TxEnvelope::decode_2718(&mut remaining).map_err(|e| FilterError::Decode(e.to_string()))?;
        if !remaining.is_empty() {
            return Err(FilterError::TrailingBytes(remaining.len()));
        }

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

        // Unwrap Safe-module calls and match on the inner target(s); match other calls directly.
        let (calls, via_module) = if outer_selector == execTransactionFromModuleCall::SELECTOR {
            let call = execTransactionFromModuleCall::abi_decode(&input[..])
                .map_err(|e| FilterError::ModuleUnwrap(e.to_string()))?;

            let calls = match call.operation {
                OPERATION_CALL => vec![AuthorizedCall {
                    to: Address::from(call.to.into_array()),
                    selector: selector_of(&call.data[..])?,
                }],
                // The only delegate target that is transparent about what it executes is the
                // canonical MultiSend singleton, whose batch entries are validated individually.
                OPERATION_DELEGATE_CALL if call.to.into_array() == SAFE_MULTI_SEND_ADDRESS => {
                    decode_multi_send(&call.data[..])?
                }
                _ => return Err(FilterError::DelegateCallNotAllowed),
            };

            (calls, true)
        } else {
            (
                vec![AuthorizedCall {
                    to: Address::from(outer_to.into_array()),
                    selector: outer_selector,
                }],
                false,
            )
        };

        if calls.is_empty() {
            return Err(FilterError::MultiSendDecode("batch contains no calls".into()));
        }

        for call in &calls {
            let selectors = self
                .allowed
                .get(&call.to)
                .ok_or_else(|| FilterError::ContractNotAllowed {
                    contract: call.to.to_hex(),
                })?;

            if !selectors.contains(&call.selector) {
                return Err(FilterError::Unauthorized {
                    contract: call.to.to_hex(),
                    selector: format!("0x{}", hex::encode(call.selector)),
                });
            }
        }

        Ok(FilteredTransaction {
            sender: Address::from(sender.into_array()),
            calls,
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

/// Decode the calls batched in a Gnosis Safe `MultiSend` payload.
///
/// `transactions` is the tight packing of `operation (1) | to (20) | value (32) | len (32) | data`
/// repeated for each batched call. Only plain calls are accepted — a nested delegate call would
/// again execute unknown code under the Safe's own context.
fn decode_multi_send(input: &[u8]) -> Result<Vec<AuthorizedCall>> {
    let batch = multiSendCall::abi_decode(input).map_err(|e| FilterError::MultiSendDecode(e.to_string()))?;
    let packed = &batch.transactions[..];

    let mut calls = Vec::new();
    let mut offset = 0usize;
    while offset < packed.len() {
        let header = packed
            .get(offset..offset + MULTI_SEND_HEADER_LEN)
            .ok_or_else(|| FilterError::MultiSendDecode("truncated batch entry header".into()))?;

        if header[0] != OPERATION_CALL {
            return Err(FilterError::DelegateCallNotAllowed);
        }

        let to: [u8; 20] = header[1..21]
            .try_into()
            .map_err(|_| FilterError::MultiSendDecode("invalid batch entry target".into()))?;

        // The data length is a 32-byte big-endian word; anything beyond `usize` is out of range.
        let len_word: [u8; 32] = header[53..85]
            .try_into()
            .map_err(|_| FilterError::MultiSendDecode("invalid batch entry data length".into()))?;
        if len_word[..24] != [0u8; 24] {
            return Err(FilterError::MultiSendDecode(
                "batch entry data length out of range".into(),
            ));
        }
        let data_len =
            usize::try_from(u64::from_be_bytes(len_word[24..].try_into().map_err(|_| {
                FilterError::MultiSendDecode("invalid batch entry data length".into())
            })?))
            .map_err(|_| FilterError::MultiSendDecode("batch entry data length out of range".into()))?;

        offset += MULTI_SEND_HEADER_LEN;
        let data = packed
            .get(offset..offset + data_len)
            .ok_or_else(|| FilterError::MultiSendDecode("truncated batch entry data".into()))?;
        offset += data_len;

        calls.push(AuthorizedCall {
            to: Address::from(to),
            selector: selector_of(data)?,
        });
    }

    Ok(calls)
}

#[cfg(test)]
mod tests {
    use hopr_bindings::exports::alloy::{
        consensus::{SignableTransaction, TxEip1559, TxEip2930, TxLegacy},
        eips::eip2718::Encodable2718,
        primitives::{Address as AlloyAddress, Bytes, TxKind, U256},
        signers::{SignerSync, local::PrivateKeySigner},
        sol_types::SolCall,
    };
    use hopr_types::primitive::prelude::Address;

    use crate::{
        errors::FilterError,
        filter::{
            OPERATION_CALL, OPERATION_DELEGATE_CALL, SAFE_MULTI_SEND_ADDRESS, Selector, TransactionFilter,
            execTransactionFromModuleCall, multiSendCall,
        },
    };

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

    /// Sign and 2718-encode a legacy transaction with the given recipient and calldata.
    fn signed_legacy_tx(to: TxKind, input: Vec<u8>) -> Vec<u8> {
        let tx = TxLegacy {
            chain_id: Some(1),
            nonce: 0,
            gas_price: 1_000_000_000,
            gas_limit: 21_000,
            to,
            value: U256::ZERO,
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

    /// Tightly pack one `MultiSend` batch entry.
    fn multi_send_entry(operation: u8, to: [u8; 20], data: &[u8]) -> Vec<u8> {
        let mut encoded = Vec::with_capacity(85 + data.len());
        encoded.push(operation);
        encoded.extend_from_slice(&to);
        encoded.extend_from_slice(&[0u8; 32]);
        encoded.extend_from_slice(&U256::from(data.len()).to_be_bytes::<32>());
        encoded.extend_from_slice(data);
        encoded
    }

    /// Build the calldata for a module delegate call into `MultiSend` batching the given entries.
    fn multi_send_calldata(entries: Vec<u8>) -> Vec<u8> {
        let multi_send = multiSendCall {
            transactions: Bytes::from(entries),
        }
        .abi_encode();

        module_calldata(SAFE_MULTI_SEND_ADDRESS, multi_send, OPERATION_DELEGATE_CALL)
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
    fn legacy_allowed_call_passes() {
        let raw = signed_legacy_tx(TxKind::Call(AlloyAddress::from(CONTRACT)), calldata(SELECTOR_TRANSFER));
        let result = filter().filter_transaction(&raw).expect("authorized legacy call");
        insta::assert_debug_snapshot!(result);
    }

    #[test]
    fn legacy_disallowed_call_is_rejected() {
        let raw = signed_legacy_tx(TxKind::Call(AlloyAddress::from(OTHER)), calldata(SELECTOR_TRANSFER));
        assert!(matches!(
            filter().filter_transaction(&raw),
            Err(FilterError::ContractNotAllowed { .. })
        ));
    }

    #[test]
    fn safe_wrapped_call_passes_on_inner_target() {
        // Outer call targets the per-node module; inner call targets the allowed contract.
        let inner = calldata(SELECTOR_APPROVE);
        let outer = module_calldata(CONTRACT, inner, OPERATION_CALL);
        let raw = signed_tx(TxKind::Call(AlloyAddress::from(MODULE)), outer);
        let result = filter().filter_transaction(&raw).expect("authorized safe-wrapped call");
        insta::assert_debug_snapshot!(result);
    }

    #[test]
    fn multi_send_batch_passes_when_every_entry_is_allowed() {
        let mut entries = multi_send_entry(OPERATION_CALL, CONTRACT, &calldata(SELECTOR_APPROVE));
        entries.extend(multi_send_entry(OPERATION_CALL, CONTRACT, &calldata(SELECTOR_TRANSFER)));
        let raw = signed_tx(TxKind::Call(AlloyAddress::from(MODULE)), multi_send_calldata(entries));

        let result = filter().filter_transaction(&raw).expect("authorized multi-send batch");
        insta::assert_debug_snapshot!(result);
    }

    #[test]
    fn multi_send_batch_with_a_disallowed_entry_is_rejected() {
        let mut entries = multi_send_entry(OPERATION_CALL, CONTRACT, &calldata(SELECTOR_APPROVE));
        entries.extend(multi_send_entry(OPERATION_CALL, OTHER, &calldata(SELECTOR_TRANSFER)));
        let raw = signed_tx(TxKind::Call(AlloyAddress::from(MODULE)), multi_send_calldata(entries));

        assert!(matches!(
            filter().filter_transaction(&raw),
            Err(FilterError::ContractNotAllowed { .. })
        ));
    }

    #[test]
    fn multi_send_batch_with_a_nested_delegate_call_is_rejected() {
        let entries = multi_send_entry(OPERATION_DELEGATE_CALL, CONTRACT, &calldata(SELECTOR_APPROVE));
        let raw = signed_tx(TxKind::Call(AlloyAddress::from(MODULE)), multi_send_calldata(entries));

        assert_eq!(
            filter().filter_transaction(&raw),
            Err(FilterError::DelegateCallNotAllowed)
        );
    }

    #[test]
    fn truncated_multi_send_batch_is_rejected() {
        let mut entries = multi_send_entry(OPERATION_CALL, CONTRACT, &calldata(SELECTOR_APPROVE));
        entries.truncate(entries.len() - 4);
        let raw = signed_tx(TxKind::Call(AlloyAddress::from(MODULE)), multi_send_calldata(entries));

        assert!(matches!(
            filter().filter_transaction(&raw),
            Err(FilterError::MultiSendDecode(_))
        ));
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
            Err(FilterError::ContractNotAllowed { .. })
        ));
    }

    #[test]
    fn safe_wrapped_call_to_non_allowed_inner_target_is_rejected() {
        let inner = calldata(SELECTOR_TRANSFER);
        let outer = module_calldata(OTHER, inner, OPERATION_CALL);
        let raw = signed_tx(TxKind::Call(AlloyAddress::from(MODULE)), outer);
        assert!(matches!(
            filter().filter_transaction(&raw),
            Err(FilterError::ContractNotAllowed { .. })
        ));
    }

    #[test]
    fn safe_wrapped_delegate_call_to_other_target_is_rejected() {
        let inner = calldata(SELECTOR_TRANSFER);
        let outer = module_calldata(CONTRACT, inner, OPERATION_DELEGATE_CALL);
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
    fn trailing_bytes_after_the_envelope_are_rejected() {
        let mut raw = signed_tx(TxKind::Call(AlloyAddress::from(CONTRACT)), calldata(SELECTOR_TRANSFER));
        raw.extend_from_slice(&[0xde, 0xad, 0xbe, 0xef]);
        assert_eq!(filter().filter_transaction(&raw), Err(FilterError::TrailingBytes(4)));
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
