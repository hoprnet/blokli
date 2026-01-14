use std::str::FromStr;

use hopr_bindings::exports::alloy::primitives::Address as AlloyAddress;
use hopr_chain_connector::ChainKeypair;
use hopr_crypto_types::keypairs::{Keypair, OffchainKeypair};
use hopr_internal_types::announcement::KeyBinding;
use hopr_primitive_types::prelude::Address;

#[derive(Clone, Debug)]
pub struct AnvilAccount {
    pub keypair: ChainKeypair,
    pub address: Address,
}

impl AnvilAccount {
    pub fn new(private_key: String, address: String) -> Self {
        let parsed_private_key =
            hex::decode(private_key.strip_prefix("0x").unwrap_or(&private_key)).expect("Invalid private key hex");

        let keypair = ChainKeypair::from_secret(&parsed_private_key).expect("Invalid private key hex");

        let parsed_address = Address::from_str(&address).expect("Invalid address hex");

        Self {
            keypair,
            address: parsed_address,
        }
    }

    pub fn to_alloy_address(&self) -> AlloyAddress {
        AlloyAddress::from_str(&self.address.to_string()).expect("Invalid address hex")
    }

    pub fn keybinding(&self) -> KeyBinding {
        KeyBinding::new(self.address, &self.offchain_keypair())
    }

    pub fn offchain_keypair(&self) -> OffchainKeypair {
        OffchainKeypair::from_secret(self.keypair.secret().as_ref()).expect("Invalid private key hex")
    }
}
