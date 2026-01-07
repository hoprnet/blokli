use std::str::FromStr;

use alloy::primitives::Address as AlloyAddress;
use hopr_chain_connector::ChainKeypair;
use hopr_crypto_types::keypairs::{Keypair, OffchainKeypair};
use hopr_internal_types::announcement::KeyBinding;
use hopr_primitive_types::prelude::Address;

#[derive(Clone, Debug)]
pub struct AnvilAccount {
    pub private_key: Vec<u8>,
    pub address: Address,
}

impl AnvilAccount {
    pub fn new(private_key: String, address: String) -> Self {
        let parsed_private_key =
            hex::decode(private_key.strip_prefix("0x").unwrap_or(&private_key)).expect("Invalid private key hex");
        let parsed_address = Address::from_str(&address).expect("Invalid address hex");

        Self {
            private_key: parsed_private_key,
            address: parsed_address,
        }
    }

    pub fn to_string_address(&self) -> String {
        format!("0x{}", hex::encode(self.address)).to_lowercase()
    }

    pub fn alloy_address(&self) -> AlloyAddress {
        AlloyAddress::from_str(&self.to_string_address()).expect("Invalid address hex")
    }

    pub fn chain_key_pair(&self) -> ChainKeypair {
        ChainKeypair::from_secret(&self.private_key).expect("Invalid private key hex")
    }

    pub fn keybinding(&self) -> KeyBinding {
        let offchain_key_pair = OffchainKeypair::from_secret(&self.private_key).expect("Invalid private key hex");

        KeyBinding::new(self.address, &offchain_key_pair)
    }
}
