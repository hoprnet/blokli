use std::collections::BTreeSet;

use anyhow::Result;
use blokli_db::{BlokliDbGeneralModelOperations, TargetDb, api::logs::BlokliDbLogOperations, db::BlokliDb};
use blokli_db_entity::prelude::{Log, LogStatus};
use hopr_types::{
    crypto::prelude::Hash,
    primitive::prelude::{Address, SerializableLog},
};
use sea_orm::{EntityTrait, PaginatorTrait};

type LogIdentity = (u64, u64, u64);

const UNIQUE_LOG_COUNT: usize = 1_280;

fn log_identity(log: &SerializableLog) -> LogIdentity {
    (log.block_number, log.tx_index, log.log_index)
}

fn build_log(index: usize) -> SerializableLog {
    let block_number = 80_000 + (index / 16) as u64;
    let within_block = index % 16;
    let tx_index = (within_block / 4) as u64;
    let log_index = (within_block % 4) as u64;

    let mut address_bytes = [0_u8; 20];
    address_bytes[0] = 0x42;
    address_bytes[12..].copy_from_slice(&(index as u64).to_be_bytes());

    let mut topic_seed = vec![0x11];
    topic_seed.extend_from_slice(&(index as u64).to_be_bytes());

    let block_hash_seed = vec![0x22];
    let block_hash_seed_bytes = [block_hash_seed.as_slice(), &block_number.to_be_bytes()].concat();

    let mut tx_hash_seed = vec![0x33];
    tx_hash_seed.extend_from_slice(&block_number.to_be_bytes());
    tx_hash_seed.extend_from_slice(&tx_index.to_be_bytes());

    let mut data = Vec::from((index as u64).to_be_bytes());
    data.extend_from_slice(&block_number.to_be_bytes());
    data.push(within_block as u8);

    SerializableLog {
        address: Address::new(&address_bytes),
        topics: [Hash::create(&[topic_seed.as_slice()]).into()].into(),
        data: data.into(),
        tx_index,
        block_number,
        block_hash: Hash::create(&[block_hash_seed_bytes.as_slice()]).into(),
        tx_hash: Hash::create(&[tx_hash_seed.as_slice()]).into(),
        log_index,
        removed: false,
        processed: Some(false),
        ..Default::default()
    }
}

fn build_unique_logs() -> Vec<SerializableLog> {
    (0..UNIQUE_LOG_COUNT).map(build_log).collect()
}

fn build_duplicate_heavy_batch(unique_logs: &[SerializableLog]) -> Vec<SerializableLog> {
    let mut batch = Vec::with_capacity(unique_logs.len() * 3);
    let offset = unique_logs.len() / 2;

    for (index, log) in unique_logs.iter().enumerate() {
        batch.push(log.clone());

        if index >= offset {
            batch.push(unique_logs[index - offset].clone());
        }

        if index % 5 == 0 {
            batch.push(log.clone());
        }

        if index % 17 == 0 {
            batch.push(unique_logs[index / 17].clone());
        }
    }

    batch
}

fn processed_targets(unique_logs: &[SerializableLog]) -> Vec<SerializableLog> {
    unique_logs
        .iter()
        .enumerate()
        .filter(|(index, _)| index % 13 == 0 || index % 29 == 0)
        .map(|(_, log)| log.clone())
        .collect()
}

fn assert_all_ok(results: &[blokli_db::api::errors::Result<()>]) {
    assert!(results.iter().all(|result| result.is_ok()));
}

async fn assert_readable_state(
    db: &BlokliDb,
    expected_logs: &[SerializableLog],
    processed_identities: &BTreeSet<LogIdentity>,
) -> Result<()> {
    let stored_logs = db.get_logs(None, None).await?;

    assert_eq!(stored_logs.len(), expected_logs.len());
    assert_eq!(db.get_logs_count(None, None).await?, expected_logs.len() as u64);

    for (stored_log, expected_log) in stored_logs.iter().zip(expected_logs.iter()) {
        assert_eq!(log_identity(stored_log), log_identity(expected_log));

        let should_be_processed = processed_identities.contains(&log_identity(expected_log));
        assert_eq!(stored_log.processed, Some(should_be_processed));
        assert_eq!(stored_log.processed_at.is_some(), should_be_processed);
    }

    for expected_log in expected_logs.iter().step_by(137) {
        let stored_log = db
            .get_log(expected_log.block_number, expected_log.tx_index, expected_log.log_index)
            .await?;

        let should_be_processed = processed_identities.contains(&log_identity(expected_log));
        assert_eq!(stored_log.processed, Some(should_be_processed));
        assert_eq!(stored_log.processed_at.is_some(), should_be_processed);
    }

    let expected_processed_blocks = expected_logs
        .iter()
        .filter(|log| processed_identities.contains(&log_identity(log)))
        .map(|log| log.block_number)
        .collect::<BTreeSet<_>>();
    let expected_unprocessed_blocks = expected_logs
        .iter()
        .filter(|log| !processed_identities.contains(&log_identity(log)))
        .map(|log| log.block_number)
        .collect::<BTreeSet<_>>();

    assert_eq!(
        db.get_logs_block_numbers(None, None, Some(true))
            .await?
            .into_iter()
            .collect::<BTreeSet<_>>(),
        expected_processed_blocks,
    );
    assert_eq!(
        db.get_logs_block_numbers(None, None, Some(false))
            .await?
            .into_iter()
            .collect::<BTreeSet<_>>(),
        expected_unprocessed_blocks,
    );

    Ok(())
}

async fn assert_raw_state(
    db: &BlokliDb,
    expected_logs: &[SerializableLog],
    processed_identities: &BTreeSet<LogIdentity>,
) -> Result<()> {
    let conn = db.conn(TargetDb::Logs);

    assert_eq!(Log::find().count(conn).await?, expected_logs.len() as u64);
    assert_eq!(LogStatus::find().count(conn).await?, expected_logs.len() as u64);

    let expected_identities = expected_logs.iter().map(log_identity).collect::<BTreeSet<_>>();
    let joined_logs = Log::find().find_also_related(LogStatus).all(conn).await?;
    let mut seen_identities = BTreeSet::new();

    assert_eq!(joined_logs.len(), expected_logs.len());

    for (raw_log, raw_status) in joined_logs {
        let raw_status = raw_status.expect("missing log_status row");
        let identity = (
            raw_log.block_number as u64,
            raw_log.tx_index as u64,
            raw_log.log_index as u64,
        );

        assert!(expected_identities.contains(&identity));
        assert!(seen_identities.insert(identity));
        assert_eq!(raw_status.log_id, raw_log.id);
        assert_eq!(raw_status.block_number, raw_log.block_number);
        assert_eq!(raw_status.tx_index, raw_log.tx_index);
        assert_eq!(raw_status.log_index, raw_log.log_index);

        let should_be_processed = processed_identities.contains(&identity);
        assert_eq!(raw_status.processed, should_be_processed);
        assert_eq!(raw_status.processed_at.is_some(), should_be_processed);
    }

    assert_eq!(seen_identities, expected_identities);

    Ok(())
}

#[tokio::test]
async fn store_logs_duplicate_heavy_multi_chunk_is_idempotent() -> Result<()> {
    let db = BlokliDb::new_in_memory().await?;
    let expected_logs = build_unique_logs();
    let first_batch = build_duplicate_heavy_batch(&expected_logs);
    let mut second_batch = first_batch.clone();
    second_batch.reverse();

    let first_results = db.store_logs(first_batch).await?;
    let second_results = db.store_logs(second_batch).await?;

    assert_all_ok(&first_results);
    assert_all_ok(&second_results);

    let processed_identities = BTreeSet::new();
    assert_readable_state(&db, &expected_logs, &processed_identities).await?;
    assert_raw_state(&db, &expected_logs, &processed_identities).await?;

    Ok(())
}

#[tokio::test]
async fn store_logs_duplicate_heavy_multi_chunk_keeps_log_status_consistent() -> Result<()> {
    let db = BlokliDb::new_in_memory().await?;
    let expected_logs = build_unique_logs();
    let batch = build_duplicate_heavy_batch(&expected_logs);
    let processing_targets = processed_targets(&expected_logs);

    let first_results = db.store_logs(batch.clone()).await?;
    let second_results = db.store_logs(batch).await?;

    assert_all_ok(&first_results);
    assert_all_ok(&second_results);

    let mut processing_request = Vec::with_capacity(processing_targets.len() * 3);
    for (index, log) in processing_targets.iter().enumerate() {
        processing_request.push(log.clone());
        processing_request.push(log.clone());
        processing_request.push(processing_targets[processing_targets.len() - 1 - index].clone());
    }

    db.set_logs_processed_explicit(processing_request).await?;

    let processed_identities = processing_targets.iter().map(log_identity).collect::<BTreeSet<_>>();

    assert_readable_state(&db, &expected_logs, &processed_identities).await?;
    assert_raw_state(&db, &expected_logs, &processed_identities).await?;

    Ok(())
}
