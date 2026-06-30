//! Policy controlling which raw transactions the executor will submit.
//!
//! The policy is the integration point for the stand-alone [`blokli_tx`] filtering crate.
//! When [`TransactionPolicy::AllowAll`] is selected (the default), any non-empty transaction is
//! accepted - this preserves the historical behaviour for deployments that do not configure a
//! whitelist. When [`TransactionPolicy::Whitelist`] is selected, every transaction must match a
//! `(sender, contract, selector)` triple in the configured [`TransactionFilter`].

use blokli_tx::{FilterError, TransactionFilter};

/// Decides whether a raw signed transaction may be submitted to the chain.
#[derive(Debug, Clone, Default)]
pub enum TransactionPolicy {
    /// Accept any non-empty transaction; no whitelist enforcement.
    #[default]
    AllowAll,
    /// Enforce a `blokli-tx` whitelist of `(sender, contract, selector)` triples.
    Whitelist(TransactionFilter),
}

impl TransactionPolicy {
    /// Check a raw signed transaction against the policy.
    ///
    /// An empty payload is rejected in all modes. Under [`TransactionPolicy::Whitelist`] the
    /// transaction is decoded and matched against the whitelist via
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
    use blokli_tx::TransactionFilter;
    use hopr_bindings::exports::alloy::{
        consensus::{SignableTransaction, TxEip1559},
        eips::eip2718::Encodable2718,
        primitives::{Address as AlloyAddress, Bytes, TxKind, U256},
        signers::{SignerSync, local::PrivateKeySigner},
    };
    use hopr_types::primitive::prelude::Address;

    use super::*;

    const KEY: &str = "ac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80";
    const CONTRACT: [u8; 20] = [0x11; 20];
    const SELECTOR: [u8; 4] = [0xa9, 0x05, 0x9c, 0xbb];

    fn signed_tx(signer: &PrivateKeySigner, to: [u8; 20], selector: [u8; 4]) -> Vec<u8> {
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

    #[test]
    fn allow_all_accepts_nonempty() {
        let policy = TransactionPolicy::AllowAll;
        assert!(policy.check(&[0x01, 0x02, 0x03]).is_ok());
    }

    #[test]
    fn allow_all_rejects_empty() {
        let policy = TransactionPolicy::AllowAll;
        assert_eq!(policy.check(&[]), Err(FilterError::Empty));
    }

    #[test]
    fn whitelist_accepts_authorized() {
        let signer: PrivateKeySigner = KEY.parse().unwrap();
        let sender = Address::from(signer.address().into_array());
        let contract = Address::from(CONTRACT);
        let policy = TransactionPolicy::Whitelist(TransactionFilter::from_triples([(sender, contract, SELECTOR)]));

        let raw = signed_tx(&signer, CONTRACT, SELECTOR);
        assert!(policy.check(&raw).is_ok());
    }

    #[test]
    fn whitelist_rejects_unauthorized() {
        let signer: PrivateKeySigner = KEY.parse().unwrap();
        // Empty whitelist: nothing is authorized.
        let policy = TransactionPolicy::Whitelist(TransactionFilter::default());

        let raw = signed_tx(&signer, CONTRACT, SELECTOR);
        assert!(matches!(policy.check(&raw), Err(FilterError::Unauthorized { .. })));
    }
}
