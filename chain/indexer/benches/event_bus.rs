//! Benchmarks the subscription event bus (`IndexerState`) under fan-out.
//!
//! Every GraphQL subscription holds one receiver of the bus and sees every published event,
//! including the many it ignores. This measures how the cost of delivering a burst of
//! `ChannelUpdated` events grows with the number of subscribers, for subscribers that ignore
//! channel events and for subscribers that consume them, and reports the heap allocated per
//! delivered event and the heap the bus retains when nobody is subscribed.
//!
//! Run with `cargo bench -p blokli-chain-indexer --bench event_bus`.

use std::{
    alloc::{GlobalAlloc, Layout, System},
    borrow::Borrow,
    hint::black_box,
    sync::atomic::{AtomicUsize, Ordering},
};

use blokli_api_types::{
    Account, Channel, ChannelStatePosition, ChannelStatus, ChannelUpdate, TicketParameters, TokenValueString, UInt64,
};
use blokli_chain_indexer::state::{IndexerEvent, IndexerState};
use criterion::{BenchmarkId, Criterion, Throughput};
use tokio::{
    runtime::{Builder, Runtime},
    task::JoinSet,
};

/// Counts heap allocations so the bench can report bytes allocated per delivered event.
struct CountingAllocator;

static ALLOCATED_BYTES: AtomicUsize = AtomicUsize::new(0);
static LIVE_BYTES: AtomicUsize = AtomicUsize::new(0);

unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        ALLOCATED_BYTES.fetch_add(layout.size(), Ordering::Relaxed);
        LIVE_BYTES.fetch_add(layout.size(), Ordering::Relaxed);
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        LIVE_BYTES.fetch_sub(layout.size(), Ordering::Relaxed);
        unsafe { System.dealloc(ptr, layout) }
    }
}

#[global_allocator]
static GLOBAL: CountingAllocator = CountingAllocator;

/// Production default of `indexer.subscription.event_bus_capacity`.
const BUS_CAPACITY: usize = 1000;
/// Events per burst; below the bus capacity so no subscriber overflows.
const EVENTS: usize = 500;
const SUBSCRIBERS: [usize; 3] = [10, 100, 1000];

#[derive(Clone, Copy)]
enum Interest {
    /// The subscriber filters for another event type, like `ticketParametersUpdated`.
    Ignores,
    /// The subscriber yields every channel update, like `openedChannelGraphUpdated`.
    Consumes,
}

fn account(keyid: i64) -> Account {
    Account {
        keyid,
        chain_key: format!("0x{keyid:040x}"),
        packet_key: format!("12D3KooW{keyid:044}"),
        safe_address: Some(format!("0x{:040x}", keyid + 1)),
        multi_addresses: vec![
            format!("/ip4/10.0.{}.{}/tcp/9091", keyid % 250, keyid % 200),
            format!("/ip4/10.0.{}.{}/udp/9091/quic-v1", keyid % 250, keyid % 200),
        ],
    }
}

fn channel_event(i: usize) -> IndexerEvent {
    let n = u64::try_from(i).expect("event number fits u64");
    let i = i64::try_from(i).expect("event number fits i64");
    IndexerEvent::ChannelUpdated(Box::new(ChannelUpdate {
        channel: Channel {
            concrete_channel_id: format!("{i:064x}"),
            source: i,
            destination: i + 1,
            balance: TokenValueString("1000000000000000000 wxHOPR".to_string()),
            status: ChannelStatus::Open,
            epoch: 1,
            ticket_index: UInt64(n),
            closure_time: None,
        },
        source: account(i),
        destination: account(i + 1),
        position: Some(ChannelStatePosition {
            block: UInt64(n),
            transaction_index: UInt64(0),
            log_index: UInt64(0),
        }),
    }))
}

/// Mirrors the receive loop of a subscription: inspect the event, clone the payload it yields.
fn handle<E: Borrow<IndexerEvent>>(event: E, interest: Interest) {
    match (event.borrow(), interest) {
        (IndexerEvent::ChannelUpdated(update), Interest::Consumes) => {
            black_box(ChannelUpdate::clone(update));
        }
        (IndexerEvent::TicketParametersUpdated(params), _) => {
            black_box(TicketParameters::clone(params));
        }
        _ => {}
    }
}

/// Publishes one burst to `subscribers` receivers and waits until every receiver has seen all of it.
async fn deliver_burst(state: &IndexerState, subscribers: usize, interest: Interest, events: &[IndexerEvent]) {
    let mut tasks = JoinSet::new();
    for _ in 0..subscribers {
        let mut receiver = state.subscribe_to_events();
        let expected = events.len();
        tasks.spawn(async move {
            for _ in 0..expected {
                handle(receiver.recv().await.expect("no overflow below capacity"), interest);
            }
        });
    }
    for event in events {
        state.publish_event(event.clone());
    }
    while let Some(result) = tasks.join_next().await {
        result.expect("subscriber task panicked");
    }
}

fn runtime() -> Runtime {
    Builder::new_multi_thread()
        .worker_threads(4)
        .enable_all()
        .build()
        .expect("tokio runtime")
}

/// Prints heap figures that criterion's timings do not show.
fn report_memory(rt: &Runtime, events: &[IndexerEvent]) {
    let before = LIVE_BYTES.load(Ordering::Relaxed);
    let state = IndexerState::new(BUS_CAPACITY, 10);
    for i in 0..BUS_CAPACITY {
        state.publish_event(channel_event(i));
    }
    let retained = LIVE_BYTES.load(Ordering::Relaxed).saturating_sub(before);
    println!(
        "event_bus/retained_heap_without_subscribers: {} KiB after {BUS_CAPACITY} events",
        retained / 1024
    );
    drop(state);

    for interest in [Interest::Ignores, Interest::Consumes] {
        let label = match interest {
            Interest::Ignores => "ignores",
            Interest::Consumes => "consumes",
        };
        for subscribers in SUBSCRIBERS {
            let state = IndexerState::new(BUS_CAPACITY, 10);
            let start = ALLOCATED_BYTES.load(Ordering::Relaxed);
            rt.block_on(deliver_burst(&state, subscribers, interest, events));
            let allocated = ALLOCATED_BYTES.load(Ordering::Relaxed) - start;
            println!(
                "event_bus/allocated_per_delivery/{label}/{subscribers}: {} bytes",
                allocated / (subscribers * events.len())
            );
        }
    }
}

fn bench_fan_out(c: &mut Criterion, rt: &Runtime, events: &[IndexerEvent]) {
    for (interest, name) in [
        (Interest::Ignores, "event_bus_fan_out/ignores"),
        (Interest::Consumes, "event_bus_fan_out/consumes"),
    ] {
        let mut group = c.benchmark_group(name);
        group.sample_size(10);
        for subscribers in SUBSCRIBERS {
            group.throughput(Throughput::Elements((subscribers * events.len()) as u64));
            group.bench_with_input(BenchmarkId::from_parameter(subscribers), &subscribers, |b, &n| {
                b.to_async(rt).iter(|| async {
                    let state = IndexerState::new(BUS_CAPACITY, 10);
                    deliver_burst(&state, n, interest, events).await;
                });
            });
        }
        group.finish();
    }
}

fn main() {
    let rt = runtime();
    let events: Vec<IndexerEvent> = (0..EVENTS).map(channel_event).collect();
    report_memory(&rt, &events);

    let mut criterion = Criterion::default().configure_from_args();
    bench_fan_out(&mut criterion, &rt, &events);
    criterion.final_summary();
}
