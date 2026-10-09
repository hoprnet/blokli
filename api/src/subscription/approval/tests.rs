use std::{sync::LazyLock, time::Duration};

use async_broadcast::{Sender, broadcast};
use async_graphql::{Error, Result, Value};
use blokli_api_types::SafeHoprApproval;
use blokli_chain_indexer::state::IndexerEvent;
use futures::{Future, StreamExt, future, poll, stream::BoxStream};
use hopr_types::primitive::prelude::{Address, HoprBalance};
use tokio::sync::oneshot;

use super::safe_hopr_approval_stream;
use crate::errors;

static OWNER: LazyLock<Address> = LazyLock::new(|| [0xab; 20].into());
static SPENDER: LazyLock<Address> = LazyLock::new(|| [0xcd; 20].into());
static OTHER: LazyLock<Address> = LazyLock::new(|| [0xef; 20].into());

type ApprovalStream = BoxStream<'static, Result<SafeHoprApproval>>;

fn stream(
    initial: impl Future<Output = Result<HoprBalance>> + Send + 'static,
    capacity: usize,
) -> (Sender<IndexerEvent>, Sender<()>, ApprovalStream) {
    let (mut events, receiver) = broadcast(capacity);
    events.set_overflow(true);
    let (mut shutdown, shutdown_receiver) = broadcast(capacity);
    shutdown.set_overflow(true);
    let stream = safe_hopr_approval_stream(*OWNER, *SPENDER, initial, receiver, shutdown_receiver).boxed();
    (events, shutdown, stream)
}

fn update(owner: Address, spender: Address, amount: u64) -> IndexerEvent {
    IndexerEvent::HoprApprovalUpdated {
        owner,
        spender,
        allowance: HoprBalance::new_base(amount),
    }
}

async fn next(stream: &mut ApprovalStream) -> Option<Result<SafeHoprApproval>> {
    tokio::time::timeout(Duration::from_secs(1), stream.next())
        .await
        .expect("subscription did not make progress")
}

fn assert_code(error: &Error, code: &str) {
    assert_eq!(
        error.extensions.as_ref().and_then(|extensions| extensions.get("code")),
        Some(&Value::from(code)),
    );
}

#[tokio::test]
async fn snapshot_preserves_full_precision_and_normalizes_addresses() {
    let amount: HoprBalance = "115792089237316195423570985008687907853269984665640564039457.584007913129639935 wxHOPR"
        .parse()
        .unwrap();
    let (_events, _shutdown, mut stream) = stream(future::ready(Ok(amount)), 4);
    let snapshot = next(&mut stream).await.unwrap().unwrap();
    assert_eq!(snapshot.owner, "0xabababababababababababababababababababab");
    assert_eq!(snapshot.spender, "0xcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcd");
    assert_eq!(snapshot.allowance.0.parse::<HoprBalance>().unwrap(), amount);
}

#[tokio::test]
async fn buffers_overlapping_updates_and_filters_owner_and_spender() {
    let (snapshot, initial) = oneshot::channel();
    let (events, _shutdown, mut stream) = stream(async move { Ok(initial.await.unwrap()) }, 5);
    assert!(poll!(stream.next()).is_pending());
    events.try_broadcast(IndexerEvent::SafeDeployed(*OWNER)).unwrap();
    events.try_broadcast(update(*OTHER, *SPENDER, 1)).unwrap();
    events.try_broadcast(update(*OWNER, *OTHER, 2)).unwrap();
    // Finalized events can be older than the current RPC snapshot, or be replayed.
    events.try_broadcast(update(*OWNER, *SPENDER, 3)).unwrap();
    events.try_broadcast(update(*OWNER, *SPENDER, 3)).unwrap();
    snapshot.send(HoprBalance::new_base(10)).unwrap();
    for expected in [10, 3, 3] {
        let item = next(&mut stream).await.unwrap().unwrap();
        assert_eq!(item.allowance.0, HoprBalance::new_base(expected).to_string());
    }
    assert!(poll!(stream.next()).is_pending());
}

#[tokio::test]
async fn snapshot_error_is_reported_once_then_the_stream_ends() {
    let error = errors::graphql_error(errors::rpc_query_failed("query HOPR allowance", "unavailable"));
    let expected_message = error.message.clone();
    let expected_code = error.extensions.clone();
    let (_events, _shutdown, mut stream) = stream(future::ready(Err(error)), 4);
    let error = next(&mut stream).await.unwrap().unwrap_err();
    assert_eq!(error.message, expected_message);
    assert_eq!(error.extensions, expected_code);
    assert!(next(&mut stream).await.is_none());
}

#[tokio::test]
async fn reorg_cancels_a_pending_snapshot() {
    let (snapshot, initial) = oneshot::channel::<HoprBalance>();
    let (_events, shutdown, mut stream) = stream(async move { Ok(initial.await.unwrap()) }, 5);
    assert!(poll!(stream.next()).is_pending());
    shutdown.try_broadcast(()).unwrap();
    assert!(next(&mut stream).await.is_none());
    assert!(snapshot.is_closed(), "the pending RPC future must be dropped");
}

#[tokio::test]
async fn reorg_takes_priority_over_a_ready_snapshot() {
    let (_events, shutdown, mut stream) = stream(future::ready(Ok(HoprBalance::new_base(10))), 4);
    shutdown.try_broadcast(()).unwrap();
    assert!(next(&mut stream).await.is_none());
}

#[tokio::test]
async fn reorg_takes_priority_over_buffered_live_updates() {
    let (events, shutdown, mut stream) = stream(future::ready(Ok(HoprBalance::new_base(10))), 4);
    next(&mut stream).await.unwrap().unwrap();
    events.try_broadcast(update(*OWNER, *SPENDER, 1)).unwrap();
    shutdown.try_broadcast(()).unwrap();
    assert!(next(&mut stream).await.is_none());
}

#[tokio::test]
async fn event_overflow_reports_lag_and_ends_the_stream() {
    let (events, _shutdown, mut stream) = stream(future::ready(Ok(HoprBalance::new_base(10))), 1);
    next(&mut stream).await.unwrap().unwrap();
    events.try_broadcast(update(*OWNER, *SPENDER, 1)).unwrap();
    events.try_broadcast(update(*OWNER, *SPENDER, 2)).unwrap();
    assert_code(
        &next(&mut stream).await.unwrap().unwrap_err(),
        errors::codes::SUBSCRIPTION_LAGGED,
    );
    assert!(next(&mut stream).await.is_none());
}

#[tokio::test]
async fn shutdown_overflow_reports_lag_and_ends_the_stream() {
    let (_events, shutdown, mut stream) = stream(future::pending(), 1);
    shutdown.try_broadcast(()).unwrap();
    shutdown.try_broadcast(()).unwrap();
    assert_code(
        &next(&mut stream).await.unwrap().unwrap_err(),
        errors::codes::SUBSCRIPTION_LAGGED,
    );
    assert!(next(&mut stream).await.is_none());
}

#[tokio::test]
async fn closing_the_event_bus_ends_live_streaming() {
    let (events, _shutdown, mut stream) = stream(future::ready(Ok(HoprBalance::new_base(10))), 4);
    next(&mut stream).await.unwrap().unwrap();
    drop(events);
    assert!(next(&mut stream).await.is_none());
}

#[tokio::test]
async fn closing_the_shutdown_bus_cancels_the_snapshot() {
    let (_events, shutdown, mut stream) = stream(future::pending(), 4);
    drop(shutdown);
    assert!(next(&mut stream).await.is_none());
}
