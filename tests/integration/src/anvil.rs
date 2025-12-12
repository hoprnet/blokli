use std::str::FromStr;

use alloy::primitives::Address as AlloyAddress;
use hopr_chain_connector::ChainKeypair;
use hopr_crypto_types::keypairs::{Keypair, OffchainKeypair};
use hopr_primitive_types::prelude::Address;

#[derive(Clone, Debug)]
pub struct AnvilAccount {
    pub private_key: String,
    pub address: String,
}

impl AnvilAccount {
    pub fn alloy_address(&self) -> AlloyAddress {
        AlloyAddress::from_str(&self.address).expect("Invalid address hex")
    }

    pub fn hopr_address(&self) -> Address {
        Address::from_str(&self.address).expect("Invalid address hex")
    }

    pub fn offchain_key_pair(&self) -> OffchainKeypair {
        let key_bytes = hex::decode(self.private_key.strip_prefix("0x").unwrap_or(&self.private_key))
            .expect("Invalid hex in private key");
        OffchainKeypair::from_secret(&key_bytes).expect("Invalid private key hex")
    }

    pub fn chain_key_pair(&self) -> ChainKeypair {
        let key_bytes = hex::decode(self.private_key.strip_prefix("0x").unwrap_or(&self.private_key))
            .expect("Invalid hex in private key");

        ChainKeypair::from_secret(&key_bytes).expect("Invalid private key hex")
    }
}
