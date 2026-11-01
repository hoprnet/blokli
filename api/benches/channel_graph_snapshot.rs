//! Benchmarks the initial snapshot of the `openedChannelGraphUpdated` subscription against
//! PostgreSQL seeded with a production-shaped channel graph.
//!
//! Every HOPR node opens this subscription on start and again after each reconnect, so the
//! snapshot's cost is paid once per client per connect. The channel graph only ever has one
//! current state per channel, but `channel_state` keeps every historical state (each balance or
//! ticket index change appends a row), so the snapshot must not grow with that history.
//!
//! Besides criterion's timings, the bench prints the peak heap a single snapshot needs.
//!
//! ```text
//! BLOKLI_BENCH_POSTGRES_URL=postgres://postgres@127.0.0.1:5433/postgres \
//!     cargo bench -p blokli-api --bench channel_graph_snapshot
//! ```

#[path = "../tests/common/mod.rs"]
#[allow(dead_code, unused_imports)]
mod common;

use std::{
    alloc::{GlobalAlloc, Layout, System},
    sync::atomic::{AtomicUsize, Ordering},
    time::Duration,
};

use async_graphql::Schema;
use blokli_api::{mutation::MutationRoot, query::QueryRoot, subscription::SubscriptionRoot};
use blokli_db::{
    BlokliDbGeneralModelOperations, TargetDb,
    db::{BlokliDb, BlokliDbConfig},
};
use criterion::{BenchmarkId, Criterion};
use futures::{StreamExt, future::join_all};
use sea_orm::{ConnectionTrait, Database};
use tokio::runtime::{Builder, Runtime};
use url::Url;

/// Tracks live and peak heap so the bench can report the memory one snapshot needs.
struct PeakAllocator;

static LIVE_BYTES: AtomicUsize = AtomicUsize::new(0);
static PEAK_BYTES: AtomicUsize = AtomicUsize::new(0);

unsafe impl GlobalAlloc for PeakAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let live = LIVE_BYTES.fetch_add(layout.size(), Ordering::Relaxed) + layout.size();
        PEAK_BYTES.fetch_max(live, Ordering::Relaxed);
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        LIVE_BYTES.fetch_sub(layout.size(), Ordering::Relaxed);
        unsafe { System.dealloc(ptr, layout) }
    }
}

#[global_allocator]
static GLOBAL: PeakAllocator = PeakAllocator;

const POSTGRES_URL_VAR: &str = "BLOKLI_BENCH_POSTGRES_URL";
const BENCH_DB: &str = "blokli_bench_api";
const ACCOUNTS: i64 = 1_000;
const ANNOUNCEMENTS_PER_ACCOUNT: i64 = 5;

const SNAPSHOT_QUERY: &str = r#"
    subscription {
        openedChannelGraphUpdated {
            channel { concreteChannelId source destination balance status epoch ticketIndex }
            source { keyid chainKey packetKey safeAddress multiAddresses }
            destination { keyid chainKey packetKey safeAddress multiAddresses }
        }
    }
"#;

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
    let db = BlokliDb::new(url.as_str(), None, BlokliDbConfig::default())
        .await
        .expect("open bench database");
    db.ensure_singletons().await.expect("singletons");
    db
}

/// Seeds `channels` channels between `ACCOUNTS` accounts, each channel with `states` historical
/// states. Every tenth channel ends closed, so the snapshot holds 90% of the channels.
async fn seed_graph(db: &BlokliDb, channels: i64, states: i64) -> usize {
    let sql = format!(
        "INSERT INTO account (chain_key, packet_key, published_block, published_tx_index, published_log_index)
         SELECT decode(lpad(to_hex(g), 40, '0'), 'hex'), lpad(to_hex(g), 64, '0'), g, 0, 0
         FROM generate_series(1, {ACCOUNTS}) g;
         INSERT INTO account_state (account_id, safe_address, published_block, published_tx_index, published_log_index)
         SELECT id, decode(lpad(to_hex(id + 5000000), 40, '0'), 'hex'), id, 0, 0 FROM account;
         INSERT INTO announcement (account_id, multiaddress, published_block, published_tx_index, published_log_index)
         SELECT id, '/ip4/10.0.0.' || (k % 250) || '/tcp/' || (9000 + k), id * 10 + k, 0, 0
         FROM account, generate_series(1, {ANNOUNCEMENTS_PER_ACCOUNT}) k;
         INSERT INTO channel (concrete_channel_id, source, destination)
         SELECT lpad(to_hex(g), 64, '0'), 1 + (g % {ACCOUNTS}), 1 + ((g * 7 + 1) % {ACCOUNTS})
         FROM generate_series(1, {channels}) g;
         INSERT INTO channel_state (channel_id, balance, status, epoch, ticket_index, corrupted_state, \
         published_block, published_tx_index, published_log_index, reorg_correction)
         SELECT c.id, decode(lpad(to_hex(1000000 + s), 24, '0'), 'hex'),
                CASE WHEN s = {states} AND c.id % 10 = 0 THEN 0 ELSE 1 END, 1, s, false,
                100000 + s * {channels} + c.id, 0, 0, false
         FROM channel c, generate_series(1, {states}) s;
         UPDATE chain_info SET last_indexed_block = {last_block}, last_indexed_tx_index = 0, last_indexed_log_index = \
         0;
         ANALYZE;",
        last_block = 100_000 + (states + 1) * channels + channels,
    );
    db.conn(TargetDb::Index)
        .execute_unprepared(&sql)
        .await
        .expect("seed graph");
    usize::try_from(channels - channels / 10).expect("non-negative channel count")
}

type BenchSchema = Schema<QueryRoot, MutationRoot, SubscriptionRoot>;

/// Opens the subscription and reads the snapshot. Returns the number of entries.
async fn read_snapshot(schema: &BenchSchema, expected: usize) -> usize {
    let mut stream = schema.execute_stream(SNAPSHOT_QUERY).boxed();
    let mut received = 0;
    while received < expected {
        let response = tokio::time::timeout(Duration::from_secs(120), stream.next())
            .await
            .expect("snapshot entry in time")
            .expect("stream open");
        assert!(response.errors.is_empty(), "{:?}", response.errors);
        received += 1;
    }
    received
}

fn bench_snapshot(c: &mut Criterion, rt: &Runtime, admin_url: &str) {
    let mut group = c.benchmark_group("channel_graph_snapshot");
    group.sample_size(10).measurement_time(Duration::from_secs(20));

    for (channels, states) in [(2_000_i64, 10_i64), (2_000, 100), (10_000, 20)] {
        let (db, expected) = rt.block_on(async {
            let db = fresh_db(admin_url).await;
            let expected = seed_graph(&db, channels, states).await;
            (db, expected)
        });
        let (schema, _indexer_state) = rt.block_on(async { common::create_test_schema(&db) });
        let label = format!("{channels}ch_x_{states}states");

        // Peak heap of one snapshot, measured outside criterion.
        let baseline = LIVE_BYTES.load(Ordering::Relaxed);
        PEAK_BYTES.store(baseline, Ordering::Relaxed);
        assert_eq!(rt.block_on(read_snapshot(&schema, expected)), expected);
        let peak = PEAK_BYTES.load(Ordering::Relaxed).saturating_sub(baseline);
        println!("channel_graph_snapshot/peak_heap/{label}: {} KiB", peak / 1024);

        group.bench_with_input(BenchmarkId::new("one_client", &label), &schema, |b, schema| {
            b.to_async(rt).iter(|| read_snapshot(schema, expected));
        });
        // A reconnect wave: many nodes open the subscription at once.
        group.bench_with_input(BenchmarkId::new("16_clients", &label), &schema, |b, schema| {
            b.to_async(rt).iter(|| {
                join_all((0..16).map(|_| {
                    let schema = schema.clone();
                    tokio::spawn(async move { read_snapshot(&schema, expected).await })
                }))
            });
        });
    }
    group.finish();
}

fn main() {
    let Ok(admin_url) = std::env::var(POSTGRES_URL_VAR) else {
        eprintln!("{POSTGRES_URL_VAR} is not set; skipping the channel graph benchmarks");
        return;
    };
    let rt = Builder::new_multi_thread().enable_all().build().expect("tokio runtime");

    let mut criterion = Criterion::default().configure_from_args();
    bench_snapshot(&mut criterion, &rt, &admin_url);
    criterion.final_summary();
}
