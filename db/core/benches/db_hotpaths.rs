//! Benchmarks database operations that run on every block or on every API request, against
//! PostgreSQL seeded with a production-sized history.
//!
//! - `log_checksums`: the indexer checksums new logs after every finalized block, including empty ones. Its cost must
//!   not grow with the number of logs already checksummed.
//! - `safe_lookup`: single-safe lookups behind the `safe`/`safeBy*` GraphQL queries and the `safeDeployed`
//!   subscription. Their cost must not grow with the number of safes.
//!
//! Each scenario creates a fresh `blokli_bench` database through the server named by
//! `BLOKLI_BENCH_POSTGRES_URL` (a URL whose user may create databases), for example:
//!
//! ```text
//! BLOKLI_BENCH_POSTGRES_URL=postgres://postgres@127.0.0.1:5433/postgres \
//!     cargo bench -p blokli-db --bench db_hotpaths
//! ```

use std::time::{Duration, Instant};

use blokli_db::{
    BlokliDbGeneralModelOperations, TargetDb,
    api::logs::BlokliDbLogOperations,
    db::{BlokliDb, BlokliDbConfig},
};
use blokli_db_entity::conversions::safe_aggregation::{fetch_safe_by_address, fetch_safes_by_chain_key};
use criterion::{BenchmarkId, Criterion};
use sea_orm::{ConnectionTrait, Database};
use tokio::runtime::{Builder, Runtime};
use url::Url;

const POSTGRES_URL_VAR: &str = "BLOKLI_BENCH_POSTGRES_URL";
const BENCH_DB: &str = "blokli_bench";
/// Logs per block in the seeded history.
const LOGS_PER_BLOCK: i64 = 4;

/// Drops and recreates the bench database, then opens it with migrations applied.
async fn fresh_db(admin_url: &str) -> BlokliDb {
    let admin = Database::connect(admin_url).await.expect("connect to admin database");
    admin
        .execute_unprepared(&format!("DROP DATABASE IF EXISTS {BENCH_DB} WITH (FORCE)"))
        .await
        .expect("drop bench database");
    admin
        .execute_unprepared(&format!("CREATE DATABASE {BENCH_DB}"))
        .await
        .expect("create bench database");
    admin.close().await.expect("close admin connection");

    let mut url = Url::parse(admin_url).expect("valid postgres url");
    url.set_path(BENCH_DB);
    BlokliDb::new(url.as_str(), None, BlokliDbConfig::default())
        .await
        .expect("open bench database")
}

async fn exec(db: &BlokliDb, target: TargetDb, sql: &str) {
    db.conn(target).execute_unprepared(sql).await.expect(sql);
}

/// Inserts `count` logs, `LOGS_PER_BLOCK` per block, starting at log number `first`, either already
/// checksummed (seeded history) or waiting for their checksum (a freshly indexed block).
fn insert_logs_sql(first: i64, count: i64, checksummed: bool) -> String {
    let checksum = if checksummed {
        "decode(md5(id::text) || md5(id::text), 'hex')"
    } else {
        "NULL"
    };
    format!(
        "INSERT INTO log (address, topics, data, block_number, transaction_hash, tx_index, block_hash, log_index, \
         removed)
         SELECT decode(lpad(to_hex(g % 16), 40, '0'), 'hex'), decode(repeat('ab', 64), 'hex'),
                decode(repeat('cd', 96), 'hex'), g / {LOGS_PER_BLOCK}, decode(lpad(to_hex(g), 64, '0'), 'hex'),
                g % {LOGS_PER_BLOCK}, decode(lpad(to_hex(g / {LOGS_PER_BLOCK}), 64, '0'), 'hex'), 0, false
         FROM generate_series({first}, {last}) g;
         INSERT INTO log_status (log_id, tx_index, log_index, block_number, processed, processed_at, checksum)
         SELECT id, tx_index, log_index, block_number, true, now(), {checksum} FROM log
         WHERE block_number BETWEEN {first} / {LOGS_PER_BLOCK} AND {last} / {LOGS_PER_BLOCK};",
        last = first + count - 1
    )
}

/// A history of `logs` logs that are all checksummed already.
async fn seed_checksummed_logs(db: &BlokliDb, logs: i64) {
    exec(db, TargetDb::Logs, &insert_logs_sql(LOGS_PER_BLOCK, logs, true)).await;
    exec(db, TargetDb::Logs, "ANALYZE log_status; ANALYZE log;").await;
}

fn bench_log_checksums(c: &mut Criterion, rt: &Runtime, admin_url: &str) {
    let mut group = c.benchmark_group("log_checksums");
    group.sample_size(20).measurement_time(Duration::from_secs(10));

    for logs in [100_000_i64, 1_000_000] {
        let db = rt.block_on(async {
            let db = fresh_db(admin_url).await;
            seed_checksummed_logs(&db, logs).await;
            db
        });

        // An empty block: nothing to checksum, which is what most blocks look like.
        group.bench_with_input(BenchmarkId::new("empty_block", logs), &db, |b, db| {
            b.to_async(rt)
                .iter(|| async { db.update_logs_checksums().await.expect("checksums") });
        });

        // A block with new logs: only the new logs should be read and checksummed.
        let mut next_log = LOGS_PER_BLOCK + logs;
        group.bench_with_input(BenchmarkId::new("block_with_4_logs", logs), &db, |b, db| {
            b.iter_custom(|iters| {
                rt.block_on(async {
                    let mut elapsed = Duration::ZERO;
                    for _ in 0..iters {
                        // round up to the next block so a block is never half checksummed
                        next_log = (next_log + LOGS_PER_BLOCK - 1) / LOGS_PER_BLOCK * LOGS_PER_BLOCK;
                        exec(db, TargetDb::Logs, &insert_logs_sql(next_log, LOGS_PER_BLOCK, false)).await;
                        next_log += LOGS_PER_BLOCK;
                        let start = Instant::now();
                        db.update_logs_checksums().await.expect("checksums");
                        elapsed += start.elapsed();
                    }
                    elapsed
                })
            });
        });
    }
    group.finish();
}

/// `safes` safes, each with one contract state and three threshold changes.
async fn seed_safes(db: &BlokliDb, safes: i64) {
    exec(
        db,
        TargetDb::Index,
        &format!(
            "INSERT INTO hopr_safe_contract (address)
             SELECT decode(lpad(to_hex(g), 40, '0'), 'hex') FROM generate_series(1, {safes}) g;
             INSERT INTO hopr_safe_contract_state (hopr_safe_contract_id, module_address, chain_key, published_block, \
             published_tx_index, published_log_index)
             SELECT id, address, decode(lpad(to_hex(id + 1000000000), 40, '0'), 'hex'), id, 0, 0 FROM \
             hopr_safe_contract;
             INSERT INTO hopr_safe_threshold_state (hopr_safe_contract_id, threshold, published_block, \
             published_tx_index, published_log_index)
             SELECT id, k::text, id * 10 + k, 0, 0 FROM hopr_safe_contract, generate_series(1, 3) k;
             ANALYZE;"
        ),
    )
    .await;
}

fn address_bytes(n: i64) -> Vec<u8> {
    let mut bytes = vec![0u8; 20];
    bytes[12..].copy_from_slice(&n.to_be_bytes());
    bytes
}

fn bench_safe_lookup(c: &mut Criterion, rt: &Runtime, admin_url: &str) {
    let mut group = c.benchmark_group("safe_lookup");
    group.sample_size(20).measurement_time(Duration::from_secs(10));

    for safes in [1_000_i64, 10_000, 50_000] {
        let db = rt.block_on(async {
            let db = fresh_db(admin_url).await;
            seed_safes(&db, safes).await;
            db
        });
        let conn = db.conn(TargetDb::Index);
        let middle = safes / 2;
        let safe_address = address_bytes(middle);
        let chain_key = address_bytes(middle + 1_000_000_000);

        let found = rt
            .block_on(fetch_safe_by_address(conn, &safe_address))
            .expect("lookup")
            .expect("seeded safe exists");
        assert_eq!(found.threshold.as_deref(), Some("3"), "latest threshold is returned");
        let by_key = rt.block_on(fetch_safes_by_chain_key(conn, &chain_key)).expect("lookup");
        assert_eq!(by_key.len(), 1, "seeded safe is found by chain key");

        group.bench_with_input(BenchmarkId::new("by_address", safes), &safe_address, |b, address| {
            b.to_async(rt)
                .iter(|| async { fetch_safe_by_address(conn, address).await.expect("lookup") });
        });
        group.bench_with_input(BenchmarkId::new("by_chain_key", safes), &chain_key, |b, key| {
            b.to_async(rt)
                .iter(|| async { fetch_safes_by_chain_key(conn, key).await.expect("lookup") });
        });
    }
    group.finish();
}

fn main() {
    let Ok(admin_url) = std::env::var(POSTGRES_URL_VAR) else {
        eprintln!("{POSTGRES_URL_VAR} is not set; skipping the database benchmarks");
        return;
    };
    let rt = Builder::new_multi_thread().enable_all().build().expect("tokio runtime");

    let mut criterion = Criterion::default().configure_from_args();
    bench_log_checksums(&mut criterion, &rt, &admin_url);
    bench_safe_lookup(&mut criterion, &rt, &admin_url);
    criterion.final_summary();
}
