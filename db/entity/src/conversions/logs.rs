// Allow casts for blockchain indices that never exceed i64::MAX in practice
#![allow(clippy::cast_possible_wrap, clippy::cast_sign_loss)]

use hopr_crypto_types::types::Hash;
use hopr_primitive_types::prelude::{Address, SerializableLog, ToHex};
use sea_orm::Set;

use crate::{errors::DbEntityError, log, log_status};

impl From<SerializableLog> for log::ActiveModel {
    fn from(value: SerializableLog) -> Self {
        log::ActiveModel {
            address: Set(value.address.as_ref().to_vec()),
            topics: Set(value.topics.into_iter().flatten().collect()),
            data: Set(value.data),
            block_number: Set(value.block_number as i64),
            transaction_hash: Set(value.tx_hash.to_vec()),
            tx_index: Set(value.tx_index as i64),
            block_hash: Set(value.block_hash.to_vec()),
            log_index: Set(value.log_index as i64),
            removed: Set(value.removed),
            ..Default::default()
        }
    }
}

impl TryFrom<log::Model> for SerializableLog {
    type Error = DbEntityError;

    fn try_from(value: log::Model) -> Result<Self, Self::Error> {
        let tx_hash: [u8; 32] = value
            .transaction_hash
            .try_into()
            .map_err(|_| DbEntityError::Conversion("Invalid tx_hash".into()))?;
        let block_hash: [u8; 32] = value
            .block_hash
            .try_into()
            .map_err(|_| DbEntityError::Conversion("Invalid block_hash".into()))?;
        let address = Address::new(value.address.as_ref());

        let mut topics: Vec<[u8; 32]> = Vec::new();
        for chunk in value.topics.chunks_exact(32) {
            let mut topic = [0u8; 32];
            topic.copy_from_slice(chunk);
            topics.push(topic);
        }

        let log = SerializableLog {
            address,
            topics,
            data: value.data,
            block_number: value.block_number as u64,
            tx_hash,
            tx_index: value.tx_index as u64,
            block_hash,
            log_index: value.log_index as u64,
            removed: value.removed,
            ..Default::default()
        };

        Ok(log)
    }
}

impl From<SerializableLog> for log_status::ActiveModel {
    fn from(value: SerializableLog) -> Self {
        let processed = value.processed.unwrap_or(false);
        let processed_at = value.processed_at.map(|p| p.naive_utc());
        let checksum = value
            .checksum
            .map(|c| Hash::from_hex(&c).expect("Invalid checksum").as_ref().to_vec());

        log_status::ActiveModel {
            block_number: Set(value.block_number as i64),
            tx_index: Set(value.tx_index as i64),
            log_index: Set(value.log_index as i64),
            processed: Set(processed),
            processed_at: Set(processed_at),
            checksum: Set(checksum),
            ..Default::default()
        }
    }
}
