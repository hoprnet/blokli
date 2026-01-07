use std::str::FromStr;

use alloy::{
    consensus::{SignableTransaction, TxEip1559},
    eips::eip2718::Encodable2718,
    primitives::{Address as AlloyAddress, TxKind, U256},
    signers::{Signer, local::PrivateKeySigner},
};
use anyhow::{Context, Result};

pub struct TransactionBuilder {
    signer: PrivateKeySigner,
    sender_address: String,
}

impl TransactionBuilder {
    pub fn new(private_key_hex: &[u8]) -> Result<Self> {
        let signer = PrivateKeySigner::from_slice(private_key_hex).context("Failed to construct signer")?;
        let address = format!("{:#x}", signer.address());

        Ok(Self {
            signer,
            sender_address: address,
        })
    }

    pub fn sender_address(&self) -> String {
        self.sender_address.clone()
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn build_eip1559_transaction_hex(
        &self,
        chain_id: u64,
        nonce: u64,
        recipient: &str,
        value: U256,
        max_fee_per_gas: u128,
        max_priority_fee_per_gas: u128,
        gas_limit: u64,
    ) -> Result<String> {
        let to = AlloyAddress::from_str(recipient).context("Invalid recipient address")?;
        let tx = TxEip1559 {
            chain_id,
            nonce,
            max_fee_per_gas,
            max_priority_fee_per_gas,
            gas_limit,
            to: TxKind::Call(to),
            value,
            access_list: Default::default(),
            input: Default::default(),
        };

        let tx_hash = tx.signature_hash();
        let signature = self
            .signer
            .sign_hash(&tx_hash)
            .await
            .context("Failed to sign transaction hash")?;
        let signed = tx.into_signed(signature);

        let mut encoded = Vec::new();
        signed.encode_2718(&mut encoded);

        Ok(format!("0x{}", hex::encode(encoded)))
    }
}
