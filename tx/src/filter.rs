//! Decoding and allow-set filtering of signed Ethereum transactions.

use std::{
    collections::{HashMap, HashSet},
    fmt,
};

use alloy_sol_types::{SolCall, sol};
use hopr_bindings::{
    constants::SAFE_MULTISEND_ADDRESS,
    exports::alloy::{
        consensus::{Transaction, TxEnvelope, TxType, transaction::SignerRecoverable},
        eips::eip2718::Decodable2718,
    },
    hopr_node_management_module::HoprNodeManagementModule::execTransactionFromModuleCall,
};
use hopr_types::primitive::{prelude::Address, traits::ToHex};

use crate::errors::{FilterError, Result};

sol! {
    /// Gnosis Safe `MultiSend` batch entrypoint, invoked as a delegate call.
    ///
    /// `transactions` is the tightly packed concatenation of the batched calls; see
    /// [`decode_multi_send`] for the per-entry layout. Declared here because `hopr-bindings`
    /// carries no `MultiSend` binding.
    function multiSend(bytes transactions) external payable;
}

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
/// A plain transaction and a Safe-module call yield one; a `MultiSend` batch yields one per
/// batched call.
///
/// # Example
///
/// ```
/// use blokli_tx::AuthorizedCall;
/// use hopr_types::primitive::prelude::Address;
///
/// let call = AuthorizedCall {
///     to: Address::from([0x02u8; 20]),
///     selector: Some([0x09, 0x5e, 0xa7, 0xb3]), // ERC-20 `approve`
/// };
///
/// assert_eq!(call.selector, Some([0x09, 0x5e, 0xa7, 0xb3]));
///
/// let value_transfer = AuthorizedCall {
///     to: Address::from([0x03u8; 20]),
///     selector: None,
/// };
///
/// assert_eq!(value_transfer.to_string(), "0x0303030303030303030303030303030303030303:value");
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AuthorizedCall {
    /// Effective target contract of the call.
    pub to: Address,
    /// Effective 4-byte function selector, or `None` when the call carries no calldata and so
    /// only transfers native value.
    pub selector: Option<Selector>,
}

impl fmt::Display for AuthorizedCall {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.selector {
            Some(selector) => write!(f, "{}:0x{}", self.to.to_hex(), hex::encode(selector)),
            None => write!(f, "{}:value", self.to.to_hex()),
        }
    }
}

/// The decoded, authorized details of a transaction that passed the filter.
///
/// # Example
///
/// ```
/// use blokli_tx::{FilterError, TransactionFilter};
///
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
/// A transaction passes only if every effective call it performs is in the allow-set. The
/// effective calls are:
///
/// - a plain transaction: its own `(to, selector)`;
/// - a Safe-module `execTransactionFromModule` call with `operation = Call`: the inner `(to, selector)`, since the
///   outer module address is per-node;
/// - a Safe-module `DelegateCall` into the canonical `MultiSend` singleton: every `(to, selector)` in the batch, each
///   of which must itself be a plain call.
///
/// Any other delegate call is rejected. The sender is recovered to reject contract-creation and
/// malformed transactions, but is not part of the matching key.
///
/// # Calls without a `(contract, selector)` pair
///
/// Two operation shapes have no pair to match and are refused unless opted in:
///
/// - no calldata at all — a native value transfer. See [`TransactionFilter::allowing_value_transfers`].
/// - a per-node Safe management module as target, such as `deregisterNodeBySafe`. See
///   [`TransactionFilter::allowing_on_any_target`].
///
/// # Trust assumptions
///
/// The filter has no source of truth for which addresses are genuine node modules, so it unwraps
/// on the outer selector alone: any contract exposing the same ABI is unwrapped like a module, and
/// authorization is decided on the inner call while the chain executes the outer one. Selectors
/// allowed on any target rest on the same assumption. Checking the target against the indexed node
/// modules is follow-up work.
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
    /// Selectors permitted on any target, for calls whose destination is per-node.
    any_target: HashSet<Selector>,
    /// Whether calls carrying no calldata, which only move native value, are permitted.
    allow_value_transfers: bool,
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
        Self {
            allowed,
            ..Default::default()
        }
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
        Self {
            allowed,
            ..Default::default()
        }
    }

    /// Permit the given selectors on any target.
    ///
    /// For operations whose destination is a per-node Safe management module, and so not knowable
    /// when the allow-set is built. Every other selector keeps matching on its target.
    ///
    /// # Example
    ///
    /// ```
    /// use blokli_tx::TransactionFilter;
    /// use hopr_types::primitive::prelude::Address;
    ///
    /// let deregister = [0x91, 0x60, 0x7c, 0x4c]; // `deregisterNodeBySafe(address)`
    /// let filter = TransactionFilter::default().allowing_on_any_target([deregister]);
    ///
    /// assert!(filter.filter_transaction(&[]).is_err());
    /// ```
    pub fn allowing_on_any_target(mut self, selectors: impl IntoIterator<Item = Selector>) -> Self {
        self.any_target.extend(selectors);
        self
    }

    /// Permit calls that carry no calldata and so only move native value.
    ///
    /// Their destination is not matched: a native transfer names an arbitrary recipient.
    ///
    /// # Example
    ///
    /// ```
    /// use blokli_tx::TransactionFilter;
    ///
    /// let filter = TransactionFilter::default().allowing_value_transfers();
    ///
    /// assert!(filter.filter_transaction(&[]).is_err());
    /// ```
    pub fn allowing_value_transfers(mut self) -> Self {
        self.allow_value_transfers = true;
        self
    }

    /// Decode a raw signed transaction and verify it against the allow-set.
    ///
    /// # Errors
    /// Returns the [`FilterError`] naming the reason for rejection.
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
        let outer_selector = call_selector(&input[..])?;

        // Unwrap Safe-module calls and match on the inner target(s); match other calls directly.
        let (calls, via_module) = if outer_selector == Some(execTransactionFromModuleCall::SELECTOR) {
            // `abi_decode_validate` range-checks the parameter words. `abi_decode` would keep only
            // the low byte of `operation`, reading `0x0100` as `Call` while the chain reverts it.
            let call = execTransactionFromModuleCall::abi_decode_validate(&input[..])
                .map_err(|e| FilterError::ModuleUnwrap(e.to_string()))?;

            let calls = match call.operation {
                OPERATION_CALL => vec![AuthorizedCall {
                    to: Address::from(call.to.into_array()),
                    selector: call_selector(&call.data[..])?,
                }],
                // The only delegate target that is transparent about what it executes is the
                // canonical MultiSend singleton, whose batch entries are validated individually.
                OPERATION_DELEGATE_CALL if call.to == SAFE_MULTISEND_ADDRESS => decode_multi_send(&call.data[..])?,
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
            // No calldata, so no pair to match: authorized by the value-transfer setting alone.
            let Some(selector) = call.selector else {
                if !self.allow_value_transfers {
                    return Err(FilterError::ValueTransferNotAllowed);
                }
                continue;
            };

            // Likewise for a per-node destination, which no network-derived allow-set can carry.
            if self.any_target.contains(&selector) {
                continue;
            }

            let selectors = self
                .allowed
                .get(&call.to)
                .ok_or_else(|| FilterError::ContractNotAllowed {
                    contract: call.to.to_hex(),
                })?;

            if !selectors.contains(&selector) {
                return Err(FilterError::Unauthorized {
                    contract: call.to.to_hex(),
                    selector: format!("0x{}", hex::encode(selector)),
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

/// Extract the leading 4-byte selector from calldata.
///
/// Empty calldata yields `None`: the call only moves native value. Non-empty calldata shorter
/// than a selector is malformed and yields [`FilterError::MissingSelector`].
fn call_selector(input: &[u8]) -> Result<Option<Selector>> {
    if input.is_empty() {
        return Ok(None);
    }

    input
        .get(..4)
        .ok_or(FilterError::MissingSelector)?
        .try_into()
        .map(Some)
        .map_err(|_| FilterError::MissingSelector)
}

/// Decode the calls batched in a Gnosis Safe `MultiSend` payload.
///
/// `transactions` is the tight packing of `operation (1) | to (20) | value (32) | len (32) | data`
/// repeated for each batched call. Only plain calls are accepted — a nested delegate call would
/// again execute unknown code under the Safe's own context.
fn decode_multi_send(input: &[u8]) -> Result<Vec<AuthorizedCall>> {
    let batch = multiSendCall::abi_decode_validate(input).map_err(|e| FilterError::MultiSendDecode(e.to_string()))?;
    let packed = &batch.transactions[..];

    let mut calls = Vec::new();
    let mut offset = 0usize;
    while offset < packed.len() {
        let header = offset
            .checked_add(MULTI_SEND_HEADER_LEN)
            .and_then(|end| packed.get(offset..end))
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
        // `data_len` is attacker-controlled up to `u64::MAX`, so the end of the slice must be
        // computed with a checked add: a wrapping one would panic or silently yield a bogus range.
        let end = offset
            .checked_add(data_len)
            .ok_or_else(|| FilterError::MultiSendDecode("batch entry data length out of range".into()))?;
        let data = packed
            .get(offset..end)
            .ok_or_else(|| FilterError::MultiSendDecode("truncated batch entry data".into()))?;
        offset = end;

        calls.push(AuthorizedCall {
            to: Address::from(to),
            selector: call_selector(data)?,
        });
    }

    Ok(calls)
}

#[cfg(test)]
mod tests {
    use hopr_bindings::{
        constants::SAFE_MULTISEND_ADDRESS,
        exports::alloy::{
            consensus::{SignableTransaction, TxEip1559, TxEip2930, TxLegacy},
            eips::eip2718::Encodable2718,
            primitives::{Address as AlloyAddress, Bytes, TxKind, U256},
            signers::{SignerSync, local::PrivateKeySigner},
            sol_types::SolCall,
        },
        hopr_node_management_module::HoprNodeManagementModule::execTransactionFromModuleCall,
    };
    use hopr_types::primitive::prelude::Address;

    use crate::{
        errors::FilterError,
        filter::{OPERATION_CALL, OPERATION_DELEGATE_CALL, Selector, TransactionFilter, multiSendCall},
    };

    const KEY: &str = "ac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80";
    const CONTRACT: [u8; 20] = [0x11; 20];
    const MODULE: [u8; 20] = [0x22; 20];
    const OTHER: [u8; 20] = [0x33; 20];
    const SELECTOR_APPROVE: Selector = [0x09, 0x5e, 0xa7, 0xb3];
    const SELECTOR_TRANSFER: Selector = [0xa9, 0x05, 0x9c, 0xbb];
    /// Stands in for a selector whose target is per-node and thus not in any static allow-set.
    const SELECTOR_ANY_TARGET: Selector = [0x1b, 0x2c, 0x3d, 0x4e];

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

        module_calldata(SAFE_MULTISEND_ADDRESS.into_array(), multi_send, OPERATION_DELEGATE_CALL)
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
    fn multi_send_entry_with_an_oversized_data_length_is_rejected() {
        // A header claiming `u64::MAX` bytes of data must be rejected, not overflow the slice range.
        let mut entries = multi_send_entry(OPERATION_CALL, CONTRACT, &calldata(SELECTOR_APPROVE));
        entries[53..85].copy_from_slice(&U256::from(u64::MAX).to_be_bytes::<32>());
        let raw = signed_tx(TxKind::Call(AlloyAddress::from(MODULE)), multi_send_calldata(entries));

        assert!(matches!(
            filter().filter_transaction(&raw),
            Err(FilterError::MultiSendDecode(_))
        ));
    }

    #[test]
    fn plain_value_transfer_is_rejected_by_default() {
        // No calldata at all: the transaction only moves native value, which a
        // `(contract, selector)` pair cannot express.
        let raw = signed_tx(TxKind::Call(AlloyAddress::from(OTHER)), Vec::new());
        assert_eq!(
            filter().filter_transaction(&raw),
            Err(FilterError::ValueTransferNotAllowed)
        );
    }

    #[test]
    fn plain_value_transfer_passes_when_allowed() {
        // As `BasicPayloadGenerator::transfer::<XDai>` emits it: value only, empty input.
        let raw = signed_tx(TxKind::Call(AlloyAddress::from(OTHER)), Vec::new());
        let result = filter()
            .allowing_value_transfers()
            .filter_transaction(&raw)
            .expect("authorized value transfer");
        insta::assert_debug_snapshot!(result);
    }

    #[test]
    fn safe_wrapped_value_transfer_is_rejected_by_default() {
        let outer = module_calldata(OTHER, Vec::new(), OPERATION_CALL);
        let raw = signed_tx(TxKind::Call(AlloyAddress::from(MODULE)), outer);
        assert_eq!(
            filter().filter_transaction(&raw),
            Err(FilterError::ValueTransferNotAllowed)
        );
    }

    #[test]
    fn safe_wrapped_value_transfer_passes_when_allowed() {
        // As `SafePayloadGenerator::transfer::<XDai>` emits it: a module `Call` carrying the amount
        // as `value` and no inner payload.
        let outer = module_calldata(OTHER, Vec::new(), OPERATION_CALL);
        let raw = signed_tx(TxKind::Call(AlloyAddress::from(MODULE)), outer);
        let result = filter()
            .allowing_value_transfers()
            .filter_transaction(&raw)
            .expect("authorized safe-wrapped value transfer");
        insta::assert_debug_snapshot!(result);
    }

    #[test]
    fn multi_send_entry_without_data_is_a_value_transfer() {
        let entries = multi_send_entry(OPERATION_CALL, OTHER, &[]);
        let raw = signed_tx(TxKind::Call(AlloyAddress::from(MODULE)), multi_send_calldata(entries));

        assert_eq!(
            filter().filter_transaction(&raw),
            Err(FilterError::ValueTransferNotAllowed)
        );
    }

    #[test]
    fn any_target_selector_passes_on_an_unknown_contract() {
        let raw = signed_tx(TxKind::Call(AlloyAddress::from(OTHER)), calldata(SELECTOR_ANY_TARGET));
        let result = filter()
            .allowing_on_any_target([SELECTOR_ANY_TARGET])
            .filter_transaction(&raw)
            .expect("authorized target-agnostic call");
        insta::assert_debug_snapshot!(result);
    }

    #[test]
    fn any_target_selector_does_not_widen_other_selectors() {
        let raw = signed_tx(TxKind::Call(AlloyAddress::from(OTHER)), calldata(SELECTOR_TRANSFER));
        assert!(matches!(
            filter()
                .allowing_on_any_target([SELECTOR_ANY_TARGET])
                .filter_transaction(&raw),
            Err(FilterError::ContractNotAllowed { .. })
        ));
    }

    #[test]
    fn module_operation_with_dirty_upper_bytes_is_rejected() {
        // A non-validating decoder keeps only the low byte of a `uint8` word, so `0x0100` would
        // read as `Call`, authorize the inner call and relay a transaction that the Safe's own
        // enum check reverts on chain.
        let mut outer = module_calldata(CONTRACT, calldata(SELECTOR_APPROVE), OPERATION_CALL);
        // Head words after the 4-byte selector: `to`, `value`, the `data` offset, then `operation`.
        let operation_word = 4 + 3 * 32;
        outer[operation_word..operation_word + 32].copy_from_slice(&U256::from(0x0100).to_be_bytes::<32>());
        let raw = signed_tx(TxKind::Call(AlloyAddress::from(MODULE)), outer);

        assert!(matches!(
            filter().filter_transaction(&raw),
            Err(FilterError::ModuleUnwrap(_))
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
