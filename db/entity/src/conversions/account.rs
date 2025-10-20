use std::str::FromStr;

use hopr_crypto_types::types::OffchainPublicKey;
use hopr_primitive_types::{primitives::Address, traits::ToHex};

use super::balances::address_to_string;

impl TryFrom<crate::codegen::account::Model> for Address {
    type Error = crate::errors::DbEntityError;

    fn try_from(value: crate::codegen::account::Model) -> std::result::Result<Self, Self::Error> {
        let chain_key_hex = address_to_string(&value.chain_key);
        Ok(Address::from_str(&chain_key_hex).map_err(|e| Self::Error::Conversion(format!("{e}")))?)
    }
}

impl TryFrom<crate::codegen::account::Model> for OffchainPublicKey {
    type Error = crate::errors::DbEntityError;

    fn try_from(value: crate::codegen::account::Model) -> std::result::Result<Self, Self::Error> {
        Ok(OffchainPublicKey::from_hex(&value.packet_key).map_err(|e| Self::Error::Conversion(format!("{e}")))?)
    }
}
