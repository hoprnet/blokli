//! Decoding of supported HOPR node-management operations from raw signed transactions.
//!
//! Blokli relays arbitrary signed transactions and cannot judge them semantically. For the
//! small set of HOPR node-management calls, however, the calldata is deterministic enough to
//! recognise the *logical action* a node is attempting. This module performs that recognition
//! and nothing else: it is pure, synchronous, and performs no chain or database access.
//!
//! The shapes recognised here mirror `hopr_types::chain::payload` exactly, because that is
//! what every HOPR node actually emits. Two generators exist:
//!
//! * `SafePayloadGenerator` (a node with a Safe — the production configuration) sends every operation to its
//!   **node-management module** as `execTransactionFromModule(target, value, data)`. The inner `data` is a `*Safe`
//!   channels call, which names the acting Safe in its `selfAddress` argument, or — for an announcement — an ERC777
//!   `send` on the **token** addressed to the announcements contract.
//! * `BasicPayloadGenerator` (a node without a Safe) calls the channels contract directly with the plain call variants,
//!   where `msg.sender` is the signing node itself.
//!
//! Announcement never uses `HoprAnnouncements.announce`: both generators go through the
//! ERC777 hook, which carries the key binding and the multiaddress in its payload. The plain
//! `announce`/`announceSafe` entry points are still decoded, because the contract exposes them
//! and a future client may use them.
//!
//! Anything that does not decode into one of the supported operations yields `None`, and the
//! caller must fall back to generic Blokli behaviour.

use blokli_chain_types::AlloyAddressExt;
use hopr_bindings::{
    exports::alloy::{
        consensus::{Transaction, TxEnvelope, transaction::SignerRecoverable},
        eips::eip2718::Decodable2718,
        sol_types::SolCall,
    },
    hopr_announcements::HoprAnnouncements,
    hopr_channels::HoprChannels,
    hopr_node_management_module::HoprNodeManagementModule,
    hopr_token::HoprToken,
};
use hopr_types::{crypto::types::Hash, primitive::prelude::Address};

/// Addresses of the HOPR contracts a supported call may target.
///
/// Decoding is gated on these rather than on selectors alone, so a foreign contract that
/// happens to share a selector cannot be mistaken for a HOPR operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HoprContracts {
    /// wxHOPR token, the target of the ERC777 announcement hook.
    pub token: Address,
    /// Channels contract.
    pub channels: Address,
    /// Announcements contract.
    pub announcements: Address,
}

/// A supported HOPR node-management operation, as decoded from transaction calldata.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HoprOperation {
    /// The node publishes its multiaddress, optionally binding its packet key.
    Announce {
        /// Digest of the announcement payload.
        ///
        /// The payload is carried differently by each entry point — ERC777 user data, or a
        /// bare multiaddress string — so it is reduced to a digest, which is all the
        /// deduplication key needs and keeps the key bounded.
        payload_digest: Hash,
    },
    /// The node funds an outgoing channel towards `destination`, opening it if it is closed.
    FundChannel {
        /// Counterparty at the receiving end of the channel.
        destination: Address,
        /// Funding amount in wxHOPR base units. `uint96` always fits a `u128`.
        amount: u128,
    },
    /// The node starts, or extends, the closure notice period of its outgoing channel.
    InitiateOutgoingChannelClosure {
        /// Counterparty at the receiving end of the channel.
        destination: Address,
    },
    /// The node settles its outgoing channel once the notice period has elapsed.
    FinalizeOutgoingChannelClosure {
        /// Counterparty at the receiving end of the channel.
        destination: Address,
    },
}

impl HoprOperation {
    /// Stable, low-cardinality label for metrics and structured error reporting.
    pub fn name(&self) -> &'static str {
        match self {
            HoprOperation::Announce { .. } => "announce",
            HoprOperation::FundChannel { .. } => "fund_channel",
            HoprOperation::InitiateOutgoingChannelClosure { .. } => "initiate_channel_closure",
            HoprOperation::FinalizeOutgoingChannelClosure { .. } => "finalize_channel_closure",
        }
    }

    /// Counterparty of a channel operation, or `None` for an announcement.
    pub fn destination(&self) -> Option<Address> {
        match self {
            HoprOperation::Announce { .. } => None,
            HoprOperation::FundChannel { destination, .. }
            | HoprOperation::InitiateOutgoingChannelClosure { destination }
            | HoprOperation::FinalizeOutgoingChannelClosure { destination } => Some(*destination),
        }
    }
}

/// A supported HOPR operation together with the identities that issued it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecodedHoprAction {
    /// Hash of the signed transaction, as the submitting client computed it.
    ///
    /// A refused action is never broadcast, so this hash will never appear on chain. It is
    /// recorded anyway because it is the only identifier the client and Blokli already share:
    /// it lets an operator join Blokli's refusal against the submitting node's own logs.
    pub transaction_hash: Hash,
    /// Account that signed and pays for the transaction — the node's chain key.
    ///
    /// Unlike the call target this cannot be chosen freely by a client, which makes it the
    /// correct identity for deduplication and invalid-action accounting.
    pub signer: Address,
    /// The outer call target: the node-management module, or the contract called directly.
    pub target: Address,
    /// The channel source, where the calldata determines it.
    ///
    /// A `*Safe` call names it in `selfAddress`; a direct plain call makes it the signer.
    /// It is `None` only for a module-routed plain call, where the source is the Safe
    /// registered for [`target`](Self::target) and only a database lookup can resolve it.
    pub explicit_source: Option<Address>,
    /// The decoded operation.
    pub operation: HoprOperation,
}

/// Decode a raw signed transaction into a supported HOPR operation.
///
/// Returns `None` when the envelope cannot be decoded, the signature does not recover, the
/// transaction creates a contract, or the calldata is not one of the supported operations.
/// A `None` result is not an error: it means the HOPR-aware policy does not apply and the
/// transaction keeps generic Blokli behaviour.
pub fn decode_hopr_action(raw_tx: &[u8], contracts: &HoprContracts) -> Option<DecodedHoprAction> {
    let envelope = TxEnvelope::decode_2718(&mut &raw_tx[..]).ok()?;
    let signer = envelope.recover_signer().ok()?.to_hopr_address();
    let target = envelope.to()?.to_hopr_address();
    let transaction_hash = Hash::from(envelope.tx_hash().0);
    let input = envelope.input();

    // A node with a Safe routes everything through its module.
    if let Ok(exec) = HoprNodeManagementModule::execTransactionFromModuleCall::abi_decode(input) {
        let (explicit_source, operation) = decode_call(exec.to.to_hopr_address(), &exec.data, contracts)?;
        return Some(DecodedHoprAction {
            transaction_hash,
            signer,
            target,
            explicit_source,
            operation,
        });
    }

    // A node without a Safe calls the contract directly, so it is itself the channel source.
    let (explicit_source, operation) = decode_call(target, input, contracts)?;
    Some(DecodedHoprAction {
        transaction_hash,
        signer,
        target,
        explicit_source: explicit_source.or(Some(signer)),
        operation,
    })
}

/// Decode a call to one of the HOPR contracts.
///
/// Returns the acting Safe where the calldata names it, together with the operation.
fn decode_call(target: Address, data: &[u8], contracts: &HoprContracts) -> Option<(Option<Address>, HoprOperation)> {
    if target == contracts.token {
        // The ERC777 hook: both payload generators announce this way. Only a transfer
        // addressed to the announcements contract is an announcement.
        let call = HoprToken::sendCall::abi_decode(data).ok()?;
        if call.recipient.to_hopr_address() != contracts.announcements {
            return None;
        }
        return Some((
            None,
            HoprOperation::Announce {
                payload_digest: Hash::create(&[call.data.as_ref()]),
            },
        ));
    }

    if target == contracts.announcements {
        if let Ok(call) = HoprAnnouncements::announceSafeCall::abi_decode(data) {
            return Some((
                Some(call.selfAddress.to_hopr_address()),
                HoprOperation::Announce {
                    payload_digest: Hash::create(&[call.baseMultiaddr.as_bytes()]),
                },
            ));
        }
        if let Ok(call) = HoprAnnouncements::announceCall::abi_decode(data) {
            return Some((
                None,
                HoprOperation::Announce {
                    payload_digest: Hash::create(&[call.baseMultiaddr.as_bytes()]),
                },
            ));
        }
        return None;
    }

    if target == contracts.channels {
        // `*Safe` variants first: they are what a node with a Safe emits.
        if let Ok(call) = HoprChannels::fundChannelSafeCall::abi_decode(data) {
            return Some((
                Some(call.selfAddress.to_hopr_address()),
                HoprOperation::FundChannel {
                    destination: call.account.to_hopr_address(),
                    amount: call.amount.to::<u128>(),
                },
            ));
        }
        if let Ok(call) = HoprChannels::initiateOutgoingChannelClosureSafeCall::abi_decode(data) {
            return Some((
                Some(call.selfAddress.to_hopr_address()),
                HoprOperation::InitiateOutgoingChannelClosure {
                    destination: call.destination.to_hopr_address(),
                },
            ));
        }
        if let Ok(call) = HoprChannels::finalizeOutgoingChannelClosureSafeCall::abi_decode(data) {
            return Some((
                Some(call.selfAddress.to_hopr_address()),
                HoprOperation::FinalizeOutgoingChannelClosure {
                    destination: call.destination.to_hopr_address(),
                },
            ));
        }
        if let Ok(call) = HoprChannels::fundChannelCall::abi_decode(data) {
            return Some((
                None,
                HoprOperation::FundChannel {
                    destination: call.account.to_hopr_address(),
                    amount: call.amount.to::<u128>(),
                },
            ));
        }
        if let Ok(call) = HoprChannels::initiateOutgoingChannelClosureCall::abi_decode(data) {
            return Some((
                None,
                HoprOperation::InitiateOutgoingChannelClosure {
                    destination: call.destination.to_hopr_address(),
                },
            ));
        }
        if let Ok(call) = HoprChannels::finalizeOutgoingChannelClosureCall::abi_decode(data) {
            return Some((
                None,
                HoprOperation::FinalizeOutgoingChannelClosure {
                    destination: call.destination.to_hopr_address(),
                },
            ));
        }
        return None;
    }

    None
}

#[cfg(test)]
mod tests {
    use hopr_bindings::exports::alloy::{
        consensus::{SignableTransaction, TxLegacy},
        eips::eip2718::Encodable2718,
        primitives::{Address as AlloyAddress, Bytes, TxKind, U256, aliases::U96},
        signers::{Signer, local::PrivateKeySigner},
    };

    use super::*;

    const TOKEN: [u8; 20] = [0x11; 20];
    const CHANNELS: [u8; 20] = [0x22; 20];
    const ANNOUNCEMENTS: [u8; 20] = [0x33; 20];
    const MODULE: [u8; 20] = [0xAA; 20];
    const SAFE: [u8; 20] = [0xDD; 20];
    const DESTINATION: [u8; 20] = [0xBB; 20];

    fn contracts() -> HoprContracts {
        HoprContracts {
            token: Address::from(TOKEN),
            channels: Address::from(CHANNELS),
            announcements: Address::from(ANNOUNCEMENTS),
        }
    }

    async fn sign(signer: &PrivateKeySigner, to: [u8; 20], input: Vec<u8>) -> Vec<u8> {
        let tx = TxLegacy {
            chain_id: Some(1),
            nonce: 0,
            gas_price: 1_000_000_000,
            gas_limit: 400_000,
            to: TxKind::Call(AlloyAddress::from_slice(&to)),
            value: U256::ZERO,
            input: input.into(),
        };
        let signature = signer.sign_hash(&tx.signature_hash()).await.expect("signing failed");
        let mut encoded = Vec::new();
        tx.into_signed(signature).encode_2718(&mut encoded);
        encoded
    }

    /// Mirrors `hopr_types`' `module_payload`: how a node with a Safe wraps every call.
    fn via_module(target: [u8; 20], inner: Vec<u8>) -> Vec<u8> {
        HoprNodeManagementModule::execTransactionFromModuleCall {
            to: AlloyAddress::from_slice(&target),
            value: U256::ZERO,
            data: Bytes::from(inner),
            operation: 0,
        }
        .abi_encode()
    }

    /// Mirrors the ERC777 announcement hook used by both payload generators.
    fn erc777_announce(recipient: [u8; 20], payload: &[u8]) -> Vec<u8> {
        HoprToken::sendCall {
            recipient: AlloyAddress::from_slice(&recipient),
            amount: U256::from(1_000u64),
            data: Bytes::copy_from_slice(payload),
        }
        .abi_encode()
    }

    // ---- SafePayloadGenerator shapes: module-routed `*Safe` calls ----

    #[tokio::test]
    async fn safe_node_fund_channel_names_its_safe() {
        let signer = PrivateKeySigner::random();
        let inner = HoprChannels::fundChannelSafeCall {
            selfAddress: AlloyAddress::from_slice(&SAFE),
            account: AlloyAddress::from_slice(&DESTINATION),
            amount: U96::from(1_234u64),
        }
        .abi_encode();

        let raw = sign(&signer, MODULE, via_module(CHANNELS, inner)).await;
        let action = decode_hopr_action(&raw, &contracts()).expect("should decode");

        assert_eq!(action.signer, signer.address().to_hopr_address());
        assert_eq!(action.target, Address::from(MODULE));
        assert_eq!(action.explicit_source, Some(Address::from(SAFE)));
        assert_eq!(
            action.operation,
            HoprOperation::FundChannel {
                destination: Address::from(DESTINATION),
                amount: 1_234,
            }
        );
    }

    #[tokio::test]
    async fn safe_node_closure_calls_are_decoded() {
        let signer = PrivateKeySigner::random();

        let initiate = HoprChannels::initiateOutgoingChannelClosureSafeCall {
            selfAddress: AlloyAddress::from_slice(&SAFE),
            destination: AlloyAddress::from_slice(&DESTINATION),
        }
        .abi_encode();
        let raw = sign(&signer, MODULE, via_module(CHANNELS, initiate)).await;
        let action = decode_hopr_action(&raw, &contracts()).expect("should decode");
        assert_eq!(action.explicit_source, Some(Address::from(SAFE)));
        assert_eq!(
            action.operation,
            HoprOperation::InitiateOutgoingChannelClosure {
                destination: Address::from(DESTINATION),
            }
        );

        let finalize = HoprChannels::finalizeOutgoingChannelClosureSafeCall {
            selfAddress: AlloyAddress::from_slice(&SAFE),
            destination: AlloyAddress::from_slice(&DESTINATION),
        }
        .abi_encode();
        let raw = sign(&signer, MODULE, via_module(CHANNELS, finalize)).await;
        let action = decode_hopr_action(&raw, &contracts()).expect("should decode");
        assert_eq!(
            action.operation,
            HoprOperation::FinalizeOutgoingChannelClosure {
                destination: Address::from(DESTINATION),
            }
        );
    }

    #[tokio::test]
    async fn safe_node_announcement_goes_through_the_erc777_hook() {
        let signer = PrivateKeySigner::random();
        let raw = sign(
            &signer,
            MODULE,
            via_module(TOKEN, erc777_announce(ANNOUNCEMENTS, b"key-binding-and-multiaddr")),
        )
        .await;

        let action = decode_hopr_action(&raw, &contracts()).expect("should decode");
        assert_eq!(action.target, Address::from(MODULE));
        assert!(matches!(action.operation, HoprOperation::Announce { .. }));
        assert_eq!(action.operation.name(), "announce");
        assert_eq!(action.operation.destination(), None);
    }

    #[tokio::test]
    async fn the_announcement_digest_follows_the_payload() {
        let signer = PrivateKeySigner::random();
        let first = sign(
            &signer,
            MODULE,
            via_module(TOKEN, erc777_announce(ANNOUNCEMENTS, b"/ip4/10.0.0.1/tcp/9091")),
        )
        .await;
        let second = sign(
            &signer,
            MODULE,
            via_module(TOKEN, erc777_announce(ANNOUNCEMENTS, b"/ip4/10.0.0.2/tcp/9091")),
        )
        .await;

        let a = decode_hopr_action(&first, &contracts())
            .expect("should decode")
            .operation;
        let b = decode_hopr_action(&second, &contracts())
            .expect("should decode")
            .operation;
        // Two different multiaddresses must not deduplicate against each other.
        assert_ne!(a, b);
    }

    // ---- BasicPayloadGenerator shapes: direct plain calls ----

    #[tokio::test]
    async fn a_node_without_a_safe_is_its_own_channel_source() {
        let signer = PrivateKeySigner::random();
        let input = HoprChannels::fundChannelCall {
            account: AlloyAddress::from_slice(&DESTINATION),
            amount: U96::from(7u64),
        }
        .abi_encode();

        let raw = sign(&signer, CHANNELS, input).await;
        let action = decode_hopr_action(&raw, &contracts()).expect("should decode");

        // A direct call makes `msg.sender` the channel source, which is the signing node.
        assert_eq!(action.explicit_source, Some(signer.address().to_hopr_address()));
        assert_eq!(
            action.operation,
            HoprOperation::FundChannel {
                destination: Address::from(DESTINATION),
                amount: 7,
            }
        );
    }

    #[tokio::test]
    async fn a_node_without_a_safe_announces_through_the_token_too() {
        let signer = PrivateKeySigner::random();
        let raw = sign(&signer, TOKEN, erc777_announce(ANNOUNCEMENTS, b"payload")).await;

        let action = decode_hopr_action(&raw, &contracts()).expect("should decode");
        assert!(matches!(action.operation, HoprOperation::Announce { .. }));
        assert_eq!(action.explicit_source, Some(signer.address().to_hopr_address()));
    }

    // ---- Everything else keeps generic behaviour ----

    #[tokio::test]
    async fn a_token_transfer_elsewhere_is_not_an_announcement() {
        let signer = PrivateKeySigner::random();
        // An ordinary ERC777 send that happens to pass through the token contract.
        let raw = sign(&signer, TOKEN, erc777_announce([0x99; 20], b"payload")).await;
        assert!(decode_hopr_action(&raw, &contracts()).is_none());
    }

    #[tokio::test]
    async fn a_foreign_contract_sharing_a_selector_is_ignored() {
        let signer = PrivateKeySigner::random();
        let input = HoprChannels::fundChannelCall {
            account: AlloyAddress::from_slice(&DESTINATION),
            amount: U96::from(1u64),
        }
        .abi_encode();

        // Same calldata, different target: decoding is gated on the contract address.
        let raw = sign(&signer, [0x99; 20], input).await;
        assert!(decode_hopr_action(&raw, &contracts()).is_none());
    }

    #[tokio::test]
    async fn unsupported_calldata_is_not_a_hopr_action() {
        let signer = PrivateKeySigner::random();

        let raw = sign(&signer, MODULE, via_module(CHANNELS, vec![0xDE, 0xAD, 0xBE, 0xEF])).await;
        assert!(decode_hopr_action(&raw, &contracts()).is_none());

        let raw = sign(&signer, CHANNELS, Vec::new()).await;
        assert!(decode_hopr_action(&raw, &contracts()).is_none());
    }

    #[test]
    fn undecodable_input_is_not_a_hopr_action() {
        assert!(decode_hopr_action(&[], &contracts()).is_none());
        assert!(decode_hopr_action(&[0x01, 0x02, 0x03], &contracts()).is_none());
    }
}
