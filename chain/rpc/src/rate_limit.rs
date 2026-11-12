//! Token-bucket rate limiting layer for the shared RPC client.
//!
//! The limiter paces every request that reaches the transport, so the indexer, the transaction executor, the
//! transaction monitor and the API share one budget. It is placed *inside* the retry layer, which means each retry
//! attempt consumes a token as well.
//!
//! The bucket holds at most `burst` tokens (one tenth of a second worth of requests, at least one) and refills
//! continuously at `max_requests_per_sec`. Over any window of `t` seconds, at most `burst + rate * t` requests
//! are started. Requests that find the bucket empty are delayed, in arrival order, until their token is available.

use std::{
    num::NonZeroU32,
    sync::{Arc, Mutex, PoisonError},
    task::{Context, Poll},
    time::{Duration, Instant},
};

use hopr_bindings::exports::alloy::{
    rpc::json_rpc::{RequestPacket, ResponsePacket},
    transports::{TransportError, TransportFut},
};
use tower::{Layer, Service};

/// Request rate applied when `max_requests_per_sec` is not configured.
pub const DEFAULT_MAX_RPC_REQUESTS_PER_SEC: u32 = 100;

/// Resolves the configured `max_requests_per_sec` into the effective limit.
///
/// `None` selects [`DEFAULT_MAX_RPC_REQUESTS_PER_SEC`], `Some(0)` means unlimited (returned as `None`), and
/// `Some(n)` is a hard cap of `n` requests per second.
pub fn resolve_max_requests_per_sec(configured: Option<u32>) -> Option<NonZeroU32> {
    match configured {
        None => NonZeroU32::new(DEFAULT_MAX_RPC_REQUESTS_PER_SEC),
        Some(value) => NonZeroU32::new(value),
    }
}

#[derive(Debug)]
struct Bucket {
    rate: f64,
    burst: f64,
    /// May be negative: requests have reserved tokens that are not yet refilled.
    tokens: f64,
    last_refill: Instant,
}

impl Bucket {
    fn new(max_requests_per_sec: NonZeroU32, now: Instant) -> Self {
        let rate = f64::from(max_requests_per_sec.get());
        let burst = (rate / 10.0).ceil().max(1.0);
        Self {
            rate,
            burst,
            tokens: burst,
            last_refill: now,
        }
    }

    /// Reserves one token and returns how long the caller must wait before using it.
    fn reserve(&mut self, now: Instant) -> Duration {
        let elapsed = now.saturating_duration_since(self.last_refill).as_secs_f64();
        self.last_refill = now;
        self.tokens = (self.tokens + elapsed * self.rate).min(self.burst);
        self.tokens -= 1.0;
        if self.tokens >= 0.0 {
            Duration::ZERO
        } else {
            Duration::from_secs_f64(-self.tokens / self.rate)
        }
    }
}

/// Layer that limits the rate of requests passing through the wrapped service.
#[derive(Debug, Clone)]
pub struct RateLimitLayer {
    bucket: Option<Arc<Mutex<Bucket>>>,
}

impl RateLimitLayer {
    /// Creates a layer enforcing `max_requests_per_sec`. `None` disables limiting.
    pub fn new(max_requests_per_sec: Option<NonZeroU32>) -> Self {
        Self {
            bucket: max_requests_per_sec.map(|rate| Arc::new(Mutex::new(Bucket::new(rate, Instant::now())))),
        }
    }
}

impl<S> Layer<S> for RateLimitLayer {
    type Service = RateLimitService<S>;

    fn layer(&self, inner: S) -> Self::Service {
        RateLimitService {
            inner,
            bucket: self.bucket.clone(),
        }
    }
}

/// Service created by [`RateLimitLayer`].
#[derive(Debug, Clone)]
pub struct RateLimitService<S> {
    inner: S,
    bucket: Option<Arc<Mutex<Bucket>>>,
}

impl<S> Service<RequestPacket> for RateLimitService<S>
where
    S: Service<RequestPacket, Response = ResponsePacket, Error = TransportError, Future = TransportFut<'static>>
        + Send
        + 'static
        + Clone,
{
    type Error = TransportError;
    type Future = TransportFut<'static>;
    type Response = ResponsePacket;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, request: RequestPacket) -> Self::Future {
        // Reserve synchronously so that waiting requests are served in arrival order.
        let delay = self.bucket.as_ref().map(|bucket| {
            bucket
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .reserve(Instant::now())
        });

        // Swap in a clone so the instance that was polled ready is the one that is called.
        let clone = self.inner.clone();
        let mut inner = std::mem::replace(&mut self.inner, clone);

        Box::pin(async move {
            if let Some(delay) = delay.filter(|delay| !delay.is_zero()) {
                futures_timer::Delay::new(delay).await;
            }
            inner.call(request).await
        })
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use hopr_bindings::exports::alloy::rpc::json_rpc::{Request, Response, ResponsePayload};
    use serde_json::value::RawValue;

    use super::*;

    #[test]
    fn resolve_applies_default_when_unset() {
        assert_eq!(
            resolve_max_requests_per_sec(None),
            NonZeroU32::new(DEFAULT_MAX_RPC_REQUESTS_PER_SEC)
        );
    }

    #[test]
    fn resolve_zero_is_unlimited() {
        assert_eq!(resolve_max_requests_per_sec(Some(0)), None);
    }

    #[test]
    fn resolve_positive_is_hard_cap() {
        assert_eq!(resolve_max_requests_per_sec(Some(7)), NonZeroU32::new(7));
    }

    #[test]
    fn bucket_allows_burst_then_paces() {
        let start = Instant::now();
        // 10 req/s => burst of 1, one token every 100 ms.
        let mut bucket = Bucket::new(NonZeroU32::new(10).expect("non-zero"), start);

        assert_eq!(bucket.reserve(start), Duration::ZERO);
        let waits: Vec<u128> = (0..4).map(|_| bucket.reserve(start).as_millis()).collect();
        assert_eq!(waits, vec![100, 200, 300, 400]);
    }

    #[test]
    fn bucket_rate_is_bounded_over_a_window() {
        let start = Instant::now();
        let rate = 50u32;
        let mut bucket = Bucket::new(NonZeroU32::new(rate).expect("non-zero"), start);

        // 500 simultaneous requests: the last one must be scheduled ~ (500 - burst) / rate seconds out.
        let max_wait = (0..500).map(|_| bucket.reserve(start)).max().expect("non-empty");
        let burst = 5.0; // ceil(50 / 10)
        let expected = (500.0 - burst) / f64::from(rate);
        assert!(
            (max_wait.as_secs_f64() - expected).abs() < 0.05,
            "{max_wait:?} vs {expected}"
        );
    }

    #[test]
    fn bucket_refills_but_never_beyond_burst() {
        let start = Instant::now();
        let mut bucket = Bucket::new(NonZeroU32::new(100).expect("non-zero"), start);
        // Burst is 10: drain it.
        for _ in 0..10 {
            assert_eq!(bucket.reserve(start), Duration::ZERO);
        }
        assert!(bucket.reserve(start) > Duration::ZERO);

        // After a long idle period only `burst` requests are free again.
        let later = start + Duration::from_secs(60);
        for _ in 0..10 {
            assert_eq!(bucket.reserve(later), Duration::ZERO);
        }
        assert!(bucket.reserve(later) > Duration::ZERO);
    }

    #[derive(Clone)]
    struct CountingService {
        calls: Arc<AtomicUsize>,
    }

    impl Service<RequestPacket> for CountingService {
        type Error = TransportError;
        type Future = TransportFut<'static>;
        type Response = ResponsePacket;

        fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
            Poll::Ready(Ok(()))
        }

        fn call(&mut self, request: RequestPacket) -> Self::Future {
            self.calls.fetch_add(1, Ordering::SeqCst);
            let id = request.requests().first().map(|r| r.id().clone());
            Box::pin(async move {
                let id = id.expect("request id");
                Ok(ResponsePacket::Single(Response {
                    id,
                    payload: ResponsePayload::Success(RawValue::from_string("1".into()).expect("raw")),
                }))
            })
        }
    }

    fn packet() -> RequestPacket {
        let request: Request<()> = Request::new("eth_blockNumber", 1.into(), ());
        RequestPacket::Single(request.serialize().expect("serialize"))
    }

    async fn run_calls(layer: &RateLimitLayer, count: usize) -> (usize, Duration) {
        let calls = Arc::new(AtomicUsize::new(0));
        let service = layer.layer(CountingService { calls: calls.clone() });
        let started = Instant::now();
        let futures = (0..count).map(|_| {
            let mut service = service.clone();
            service.call(packet())
        });
        for result in futures::future::join_all(futures).await {
            result.expect("call succeeds");
        }
        (calls.load(Ordering::SeqCst), started.elapsed())
    }

    #[tokio::test]
    async fn service_paces_concurrent_calls() {
        // 20 req/s => burst 2; 8 concurrent calls need ~ (8 - 2) / 20 = 300 ms.
        let layer = RateLimitLayer::new(NonZeroU32::new(20));
        let (calls, elapsed) = run_calls(&layer, 8).await;
        assert_eq!(calls, 8);
        assert!(elapsed >= Duration::from_millis(280), "too fast: {elapsed:?}");
        assert!(elapsed < Duration::from_millis(1000), "too slow: {elapsed:?}");
    }

    #[tokio::test]
    async fn service_without_limit_does_not_delay() {
        let layer = RateLimitLayer::new(None);
        let (calls, elapsed) = run_calls(&layer, 200).await;
        assert_eq!(calls, 200);
        assert!(elapsed < Duration::from_millis(200), "unexpected delay: {elapsed:?}");
    }
}
