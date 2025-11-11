use hopr_crypto_types::types::OffchainPublicKey;
use hopr_primitive_types::{primitives::Address, traits::ToHex};

impl TryFrom<crate::codegen::account::Model> for Address {
    type Error = crate::errors::DbEntityError;

    fn try_from(value: crate::codegen::account::Model) -> std::result::Result<Self, Self::Error> {
        if value.chain_key.len() != 20 {
            return Err(Self::Error::Conversion(format!(
                "Invalid chain_key length: expected 20 bytes, got {}",
                value.chain_key.len()
            )));
        }
        let mut addr_bytes = [0u8; 20];
        addr_bytes.copy_from_slice(&value.chain_key);
        Ok(Address::new(&addr_bytes))
    }
}

impl TryFrom<crate::codegen::account::Model> for OffchainPublicKey {
    type Error = crate::errors::DbEntityError;

    fn try_from(value: crate::codegen::account::Model) -> std::result::Result<Self, Self::Error> {
        Ok(OffchainPublicKey::from_hex(&value.packet_key).map_err(|e| Self::Error::Conversion(format!("{e}")))?)
    }
}
