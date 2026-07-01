use hopr_types::{
    crypto::types::Hash,
    primitive::prelude::{Address, SerializableLog, ToHex},
};
use sea_orm::Set;

use crate::{
    errors::{DbEntityError, Result},
    log, log_status,
};

fn u64_to_i64(value: u64, field_name: &str) -> Result<i64> {
    i64::try_from(value).map_err(|_| DbEntityError::Conversion(format!("{field_name} {value} exceeds i64::MAX")))
}

fn i64_to_u64(value: i64, field_name: &str) -> Result<u64> {
    u64::try_from(value)
        .map_err(|_| DbEntityError::Conversion(format!("{field_name} {value} is negative or exceeds u64::MAX")))
}

impl TryFrom<SerializableLog> for log::ActiveModel {
    type Error = DbEntityError;

    fn try_from(value: SerializableLog) -> Result<Self> {
        Ok(log::ActiveModel {
            address: Set(value.address.as_ref().to_vec()),
            topics: Set(value.topics.into_iter().flatten().collect()),
            data: Set(value.data),
            block_number: Set(u64_to_i64(value.block_number, "block_number")?),
            transaction_hash: Set(value.tx_hash.to_vec()),
            tx_index: Set(u64_to_i64(value.tx_index, "tx_index")?),
            block_hash: Set(value.block_hash.to_vec()),
            log_index: Set(u64_to_i64(value.log_index, "log_index")?),
            removed: Set(value.removed),
            ..Default::default()
        })
    }
}

impl TryFrom<log::Model> for SerializableLog {
    type Error = DbEntityError;

    fn try_from(value: log::Model) -> Result<Self> {
        let tx_hash: [u8; 32] = value
            .transaction_hash
            .try_into()
            .map_err(|_| DbEntityError::Conversion("Invalid tx_hash".into()))?;
        let block_hash: [u8; 32] = value
            .block_hash
            .try_into()
            .map_err(|_| DbEntityError::Conversion("Invalid block_hash".into()))?;
        let address = Address::new(value.address.as_ref());

        let mut topic_chunks = value.topics.chunks_exact(32);
        let topics: Vec<[u8; 32]> = topic_chunks
            .by_ref()
            .map(|chunk| {
                chunk
                    .try_into()
                    .map_err(|_| DbEntityError::Conversion("Invalid topic".into()))
            })
            .collect::<Result<_>>()?;
        if !topic_chunks.remainder().is_empty() {
            return Err(DbEntityError::Conversion("Invalid topics length".into()));
        }

        let log = SerializableLog {
            address,
            topics,
            data: value.data,
            block_number: i64_to_u64(value.block_number, "block_number")?,
            tx_hash,
            tx_index: i64_to_u64(value.tx_index, "tx_index")?,
            block_hash,
            log_index: i64_to_u64(value.log_index, "log_index")?,
            removed: value.removed,
            ..Default::default()
        };

        Ok(log)
    }
}

impl TryFrom<SerializableLog> for log_status::ActiveModel {
    type Error = DbEntityError;

    fn try_from(value: SerializableLog) -> Result<Self> {
        let processed = value.processed.unwrap_or(false);
        let processed_at = value.processed_at.map(|p| p.naive_utc());
        let checksum = value
            .checksum
            .map(|c| {
                Hash::from_hex(&c)
                    .map(|hash| hash.as_ref().to_vec())
                    .map_err(|e| DbEntityError::Conversion(format!("Invalid checksum: {e}")))
            })
            .transpose()?;

        Ok(log_status::ActiveModel {
            block_number: Set(u64_to_i64(value.block_number, "block_number")?),
            tx_index: Set(u64_to_i64(value.tx_index, "tx_index")?),
            log_index: Set(u64_to_i64(value.log_index, "log_index")?),
            processed: Set(processed),
            processed_at: Set(processed_at),
            checksum: Set(checksum),
            ..Default::default()
        })
    }
}

#[cfg(test)]
mod tests {
    use hopr_types::crypto::prelude::Hash;

    use super::*;

    fn serializable_log(block_number: u64, tx_index: u64, log_index: u64) -> SerializableLog {
        SerializableLog {
            address: Address::new(b"my address 123456789"),
            topics: [Hash::create(&[b"my topic"]).into()].into(),
            data: [1, 2, 3, 4].into(),
            tx_index,
            block_number,
            block_hash: Hash::create(&[b"my block hash"]).into(),
            tx_hash: Hash::create(&[b"my tx hash"]).into(),
            log_index,
            removed: false,
            ..Default::default()
        }
    }

    #[test]
    fn rejects_log_active_model_position_over_i64() {
        let err = log::ActiveModel::try_from(serializable_log(9_223_372_036_854_775_808_u64, 0, 0))
            .expect_err("block_number should exceed i64::MAX");

        assert!(matches!(err, DbEntityError::Conversion(message) if message.contains("block_number")));
    }

    #[test]
    fn rejects_negative_model_position() {
        let model = log::Model {
            id: 1,
            tx_index: 0,
            log_index: 0,
            block_number: -1,
            block_hash: Hash::create(&[b"my block hash"]).as_ref().to_vec(),
            transaction_hash: Hash::create(&[b"my tx hash"]).as_ref().to_vec(),
            address: Address::new(b"my address 123456789").as_ref().to_vec(),
            topics: Hash::create(&[b"my topic"]).as_ref().to_vec(),
            data: vec![1, 2, 3, 4],
            removed: false,
        };

        let err = SerializableLog::try_from(model).expect_err("negative block_number should be rejected");

        assert!(matches!(err, DbEntityError::Conversion(message) if message.contains("block_number")));
    }
}
