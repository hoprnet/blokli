//! Chain tip tracking and confirmation mode handling.
//!
//! The tracker observes the chain head through a [`TipSource`] and derives a confirmed block number
//! from the configured [`ConfirmationMode`]. Every successful update is published on a
//! `tokio::sync::watch` channel so consumers can wait for the confirmed tip without polling the RPC
//! endpoint themselves.

use std::{
    collections::VecDeque,
    fmt::{self, Formatter},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};

use async_trait::async_trait;
use hopr_bindings::exports::alloy::{
    eips::BlockNumberOrTag,
    primitives::{B256, U64},
    providers::Provider,
    rpc::client::BatchRequest,
};
#[cfg(all(feature = "telemetry", not(test)))]
use hopr_types::telemetry::{MultiGauge, SimpleCounter, SimpleGauge};
use serde::{Deserialize, Serialize};
use strum::{Display, EnumString};
use tokio::{
    sync::{Mutex as AsyncMutex, watch},
    task::JoinHandle,
};
#[cfg(all(feature = "telemetry", not(test)))]
use tracing::error;
use tracing::{debug, info, warn};

use crate::{
    errors::{Result, RpcError},
    head::{HeadSourceConfig, HeadSubscriber, record_ws_reconnect_metric, redact_ws_url, run_head_driver},
    rpc::HoprProvider,
    transport::HttpRequestor,
};

/// Rule used to derive the confirmed block from the observed chain tip.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, Display, EnumString)]
#[serde(rename_all = "snake_case")]
#[strum(serialize_all = "snake_case")]
pub enum ConfirmationMode {
    /// Use a fixed number of blocks behind the chain head.
    #[default]
    Depth,
    /// Use the safe tip when it is healthy, otherwise fall back to the configured depth.
    SafeOrDepth,
    /// Use the safe tip only.
    Safe,
    /// Use the finalized tip only.
    Finalized,
}

/// Health of the Fast Confirmation Rule as observed through the safe block tag.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Display, EnumString)]
#[serde(rename_all = "snake_case")]
#[strum(serialize_all = "snake_case")]
pub enum FcrState {
    /// The endpoint does not expose a safe block.
    Unavailable,
    /// The safe block is too far behind the latest block.
    Degraded,
    /// The safe block is close enough to the latest block to be useful.
    Healthy,
}

/// Source of head updates used by the tracker.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Display, EnumString)]
#[serde(rename_all = "snake_case")]
#[strum(serialize_all = "snake_case")]
pub enum HeadSource {
    /// The head is polled over HTTP.
    Polling,
    /// The head is delivered through a WebSocket subscription.
    #[serde(rename = "websocket")]
    #[strum(serialize = "websocket")]
    WebSocket,
}

/// A chain tip with the confirmed block derived from it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChainTip {
    /// Latest block observed on the endpoint.
    pub latest: u64,
    /// Safe block reported by the endpoint, when available.
    pub safe: Option<u64>,
    /// Finalized block reported by the endpoint, when available.
    pub finalized: Option<u64>,
    /// Block considered confirmed under the configured confirmation mode.
    ///
    /// This value is monotonic: it is the indexing watermark and never moves backwards.
    pub confirmed: u64,
    /// Source that produced the observation.
    pub source: HeadSource,
    /// Health of the safe block at the time of the observation.
    pub fcr: FcrState,
}

/// One raw reading of the chain tip from the RPC endpoint.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TipObservation {
    /// Latest block number.
    pub latest: u64,
    /// Timestamp of the latest block header in seconds since the Unix epoch.
    pub latest_timestamp: Option<u64>,
    /// Safe block number, when the endpoint reports one.
    pub safe: Option<u64>,
    /// Finalized block number, when the endpoint reports one.
    pub finalized: Option<u64>,
}

const HEADER_TIMESTAMP_CAPACITY: usize = 256;

#[derive(Debug)]
struct HeaderTimestamps {
    capacity: usize,
    entries: VecDeque<(u64, u64)>,
}

impl Default for HeaderTimestamps {
    fn default() -> Self {
        Self::new(HEADER_TIMESTAMP_CAPACITY)
    }
}

impl HeaderTimestamps {
    fn new(capacity: usize) -> Self {
        let capacity = capacity.max(1);
        Self {
            capacity,
            entries: VecDeque::with_capacity(capacity),
        }
    }

    fn insert(&mut self, block: u64, timestamp: u64) {
        if let Some((_, existing_timestamp)) = self
            .entries
            .iter_mut()
            .find(|(existing_block, _)| *existing_block == block)
        {
            *existing_timestamp = timestamp;
            return;
        }

        while self.entries.len() >= self.capacity {
            self.entries.pop_front();
        }

        self.entries.push_back((block, timestamp));
    }

    fn get(&self, block: u64) -> Option<u64> {
        self.entries
            .iter()
            .find(|(existing_block, _)| *existing_block == block)
            .map(|(_, timestamp)| *timestamp)
    }
}

/// Configuration of the confirmed-tip computation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ConfirmationConfig {
    /// Confirmation rule.
    pub mode: ConfirmationMode,
    /// Number of blocks behind latest used by depth-based modes.
    pub depth: u64,
    /// Latest-to-safe gap at or above which FCR is considered degraded.
    pub max_safe_gap: u64,
}

impl Default for ConfirmationConfig {
    fn default() -> Self {
        Self {
            mode: ConfirmationMode::Depth,
            depth: 3,
            max_safe_gap: 32,
        }
    }
}

/// Result of applying one observation to a tracker.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TipUpdate {
    /// New tip to publish.
    pub tip: ChainTip,
    /// Whether the tip may be published on the watch channel.
    ///
    /// A mode that needs the safe or finalized tag publishes nothing until that tag has been
    /// observed at least once, so a missing tag is never mistaken for a confirmation height of
    /// zero. Once a tip exists, later observations without the tag keep the last height.
    pub publish: bool,
    /// Whether the safe block moved backwards compared with the previous tip.
    pub safe_regressed: bool,
    /// Whether this update entered the safe-or-depth fallback from a non-fallback update.
    pub entered_fallback: bool,
}

/// Errors produced while deriving or tracking the confirmed tip.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum TipError {
    /// The observation violates the ordering between latest, safe and finalized.
    #[error("invalid chain tip observation: {0}")]
    InvalidObservation(String),
}

/// Computes the next tip from a previous tip and a raw observation.
pub fn next_tip(
    previous: Option<&ChainTip>,
    observation: &TipObservation,
    cfg: &ConfirmationConfig,
    source: HeadSource,
) -> std::result::Result<TipUpdate, TipError> {
    if let Some(safe) = observation.safe {
        if safe > observation.latest {
            return Err(TipError::InvalidObservation(format!(
                "safe block {safe} is above latest block {}",
                observation.latest
            )));
        }
    }

    if let Some(finalized) = observation.finalized {
        if finalized > observation.latest {
            return Err(TipError::InvalidObservation(format!(
                "finalized block {finalized} is above latest block {}",
                observation.latest
            )));
        }

        if let Some(safe) = observation.safe {
            if finalized > safe {
                return Err(TipError::InvalidObservation(format!(
                    "finalized block {finalized} is above safe block {safe}"
                )));
            }
        }
    }

    let fcr = match observation.safe {
        None => FcrState::Unavailable,
        Some(safe) if observation.latest.saturating_sub(safe) >= cfg.max_safe_gap => FcrState::Degraded,
        Some(_) => FcrState::Healthy,
    };

    let depth_candidate = observation.latest.saturating_sub(cfg.depth);
    let previous_confirmed = previous.map_or(0, |tip| tip.confirmed);
    let previous_fallback =
        previous.is_some_and(|tip| cfg.mode == ConfirmationMode::SafeOrDepth && tip.fcr != FcrState::Healthy);

    let (candidate, uses_fallback) = match cfg.mode {
        ConfirmationMode::Depth => (depth_candidate, false),
        ConfirmationMode::SafeOrDepth => match observation.safe {
            Some(safe) if fcr == FcrState::Healthy => (safe.max(depth_candidate), false),
            _ => (depth_candidate, true),
        },
        ConfirmationMode::Safe => (observation.safe.unwrap_or(previous_confirmed), false),
        ConfirmationMode::Finalized => (observation.finalized.unwrap_or(previous_confirmed), false),
    };

    let confirmed = candidate.max(previous_confirmed);
    let safe_regressed = matches!(
        (previous.and_then(|tip| tip.safe), observation.safe),
        (Some(previous_safe), Some(safe)) if safe < previous_safe
    );
    let publish = match cfg.mode {
        ConfirmationMode::Depth | ConfirmationMode::SafeOrDepth => true,
        ConfirmationMode::Safe => observation.safe.is_some() || previous.is_some(),
        ConfirmationMode::Finalized => observation.finalized.is_some() || previous.is_some(),
    };

    Ok(TipUpdate {
        tip: ChainTip {
            latest: observation.latest,
            safe: observation.safe,
            finalized: observation.finalized,
            confirmed,
            source,
            fcr,
        },
        publish,
        safe_regressed,
        entered_fallback: uses_fallback && !previous_fallback,
    })
}

/// Source of raw chain tip observations.
#[async_trait]
pub trait TipSource: Send + Sync {
    /// Reads the current chain tip from the underlying endpoint.
    async fn observe(&self) -> Result<TipObservation>;

    /// Reads the block hash for the given block number from the underlying endpoint.
    ///
    /// The default implementation reports that the lookup is unavailable, so sources that only
    /// implement [`TipSource::observe`] keep working outside the WebSocket startup probe.
    async fn block_hash(&self, _block: u64) -> Result<B256> {
        Err(RpcError::Other("block hash lookup is not supported".to_string()))
    }
}

#[derive(Debug, Clone, Deserialize)]
struct RpcBlockHeader {
    number: U64,
    timestamp: U64,
    /// Block hash. Only required for the block hash lookup, so observations keep working with
    /// endpoints or fixtures that omit it.
    #[serde(default)]
    hash: Option<B256>,
}

/// Production [`TipSource`] that reads latest, safe and finalized headers through one batched RPC
/// request.
#[derive(Debug, Clone)]
pub(crate) struct RpcTipSource<R: HttpRequestor + 'static + Clone> {
    provider: HoprProvider<R>,
}

impl<R: HttpRequestor + 'static + Clone> RpcTipSource<R> {
    pub(crate) fn new(provider: HoprProvider<R>) -> Self {
        Self { provider }
    }
}

#[async_trait]
impl<R: HttpRequestor + 'static + Clone> TipSource for RpcTipSource<R> {
    async fn observe(&self) -> Result<TipObservation> {
        let mut batch = BatchRequest::new(self.provider.client());

        let latest_waiter = batch
            .add_call::<_, Option<RpcBlockHeader>>("eth_getBlockByNumber", &(BlockNumberOrTag::Latest, false))
            .map_err(RpcError::from)?;
        let safe_waiter = batch
            .add_call::<_, Option<RpcBlockHeader>>("eth_getBlockByNumber", &(BlockNumberOrTag::Safe, false))
            .map_err(RpcError::from)?;
        let finalized_waiter = batch
            .add_call::<_, Option<RpcBlockHeader>>("eth_getBlockByNumber", &(BlockNumberOrTag::Finalized, false))
            .map_err(RpcError::from)?;

        batch.send().await.map_err(RpcError::from)?;

        let latest = latest_waiter
            .await
            .map_err(RpcError::from)?
            .ok_or_else(|| RpcError::Other("latest block is not available".to_string()))?;

        let safe = match safe_waiter.await {
            Ok(header) => header.map(|header| header.number.to::<u64>()),
            Err(error) => {
                debug!(%error, tag = "safe", "failed to read the chain tip block tag");
                None
            }
        };
        let finalized = match finalized_waiter.await {
            Ok(header) => header.map(|header| header.number.to::<u64>()),
            Err(error) => {
                debug!(%error, tag = "finalized", "failed to read the chain tip block tag");
                None
            }
        };

        Ok(TipObservation {
            latest: latest.number.to::<u64>(),
            latest_timestamp: Some(latest.timestamp.to::<u64>()),
            safe,
            finalized,
        })
    }

    async fn block_hash(&self, block: u64) -> Result<B256> {
        let mut batch = BatchRequest::new(self.provider.client());

        let waiter = batch
            .add_call::<_, Option<RpcBlockHeader>>("eth_getBlockByNumber", &(BlockNumberOrTag::Number(block), false))
            .map_err(RpcError::from)?;

        batch.send().await.map_err(RpcError::from)?;

        waiter
            .await
            .map_err(RpcError::from)?
            .and_then(|header| header.hash)
            .ok_or_else(|| RpcError::Other(format!("block {block} is not available")))
    }
}

pub(crate) struct TrackerRuntime {
    source: Arc<dyn TipSource>,
    confirmation: ConfirmationConfig,
    head: HeadSourceConfig,
    subscriber: Option<Arc<dyn HeadSubscriber>>,
    tip_tx: watch::Sender<Option<ChainTip>>,
    last_success: Mutex<Option<Instant>>,
    header_timestamps: Mutex<HeaderTimestamps>,
    refresh_lock: AsyncMutex<()>,
    failure_streak: Mutex<u32>,
    invalid_streak: Mutex<u32>,
    active_source: Mutex<HeadSource>,
    ws_reconnects: AtomicU64,
}

fn record_failure_streak(streak: &Mutex<u32>) -> bool {
    match streak.lock() {
        Ok(mut streak) => {
            let first = *streak == 0;
            *streak = streak.saturating_add(1);
            first
        }
        Err(_) => true,
    }
}

fn reset_failure_streak(streak: &Mutex<u32>) -> bool {
    match streak.lock() {
        Ok(mut streak) => {
            let had_failures = *streak > 0;
            *streak = 0;
            had_failures
        }
        Err(_) => false,
    }
}

impl fmt::Debug for TrackerRuntime {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_struct("TrackerRuntime")
            .field("confirmation", &self.confirmation)
            .field("head_poll_interval", &self.head.poll_interval)
            .field("expected_block_time", &self.head.expected_block_time)
            .field("head_ws_url", &self.head.ws_url.as_ref().map(redact_ws_url))
            .finish()
    }
}

impl TrackerRuntime {
    pub(crate) fn tip_source(&self) -> Arc<dyn TipSource> {
        self.source.clone()
    }

    pub(crate) fn active_source(&self) -> HeadSource {
        self.active_source
            .lock()
            .map(|source| *source)
            .unwrap_or(HeadSource::Polling)
    }

    pub(crate) fn set_active_source(&self, source: HeadSource) {
        if let Ok(mut active_source) = self.active_source.lock() {
            *active_source = source;
        }
    }

    fn current(&self) -> Option<ChainTip> {
        *self.tip_tx.borrow()
    }

    /// Returns the confirmed block of a tip, requiring the tag selected by the mode.
    fn confirmed_from_tip(&self, tip: ChainTip) -> Result<u64> {
        let missing_tag = match self.confirmation.mode {
            ConfirmationMode::Safe if tip.safe.is_none() => Some("safe"),
            ConfirmationMode::Finalized if tip.finalized.is_none() => Some("finalized"),
            _ => None,
        };

        match missing_tag {
            Some(tag) => Err(RpcError::Other(format!(
                "the endpoint reports no {tag} block, confirmation mode {} cannot confirm blocks",
                self.confirmation.mode
            ))),
            None => Ok(tip.confirmed),
        }
    }

    fn header_timestamp(&self, block: u64) -> Option<u64> {
        self.header_timestamps
            .lock()
            .ok()
            .and_then(|timestamps| timestamps.get(block))
    }

    fn record_source_failure(&self, error: &RpcError) {
        if record_failure_streak(&self.failure_streak) {
            warn!(%error, "failed to observe chain tip, keeping the previous tip");
        } else {
            debug!(%error, "failed to observe chain tip, keeping the previous tip");
        }
    }

    fn record_invalid_observation(&self, error: &TipError) {
        if record_failure_streak(&self.invalid_streak) {
            warn!(%error, "invalid chain tip observation, keeping the previous tip");
        } else {
            debug!(%error, "invalid chain tip observation, keeping the previous tip");
        }
    }

    fn record_success(&self) {
        let had_source_failure = reset_failure_streak(&self.failure_streak);
        let had_invalid_observation = reset_failure_streak(&self.invalid_streak);

        if had_source_failure || had_invalid_observation {
            info!("chain tip observations succeeded again");
        }
    }

    pub(crate) async fn refresh_from(&self, source: HeadSource) -> Result<ChainTip> {
        let _refresh_guard = self.refresh_lock.lock().await;

        let observation = match self.source.observe().await {
            Ok(observation) => observation,
            Err(error) => {
                self.record_source_failure(&error);
                return Err(error);
            }
        };

        if let Some(timestamp) = observation.latest_timestamp {
            if let Ok(mut timestamps) = self.header_timestamps.lock() {
                timestamps.insert(observation.latest, timestamp);
            }
        }

        let previous = self.current();
        let update = match next_tip(previous.as_ref(), &observation, &self.confirmation, source) {
            Ok(update) => update,
            Err(error) => {
                self.record_invalid_observation(&error);
                return Err(error.into());
            }
        };

        self.record_success();
        self.record_update(&update, previous.as_ref());

        if let Ok(mut last_success) = self.last_success.lock() {
            *last_success = Some(Instant::now());
        }

        if update.publish {
            self.tip_tx.send_replace(Some(update.tip));
        }
        Ok(update.tip)
    }

    pub(crate) fn record_ws_reconnect(&self) {
        self.ws_reconnects.fetch_add(1, Ordering::SeqCst);
        record_ws_reconnect_metric();
    }

    fn record_update(&self, update: &TipUpdate, previous: Option<&ChainTip>) {
        record_tip_metrics(update);

        if previous.is_none() {
            info!(
                mode = %self.confirmation.mode,
                source = %update.tip.source,
                fcr = %update.tip.fcr,
                "chain tip tracking started"
            );

            if matches!(
                self.confirmation.mode,
                ConfirmationMode::SafeOrDepth | ConfirmationMode::Safe
            ) && update.tip.fcr == FcrState::Unavailable
            {
                warn!(
                    mode = %self.confirmation.mode,
                    "safe block is unavailable, confirmation mode cannot use FCR"
                );
            }
            return;
        }

        if let Some(previous) = previous {
            if previous.fcr == FcrState::Healthy && update.tip.fcr != FcrState::Healthy {
                warn!(
                    previous = %previous.fcr,
                    current = %update.tip.fcr,
                    "FCR is no longer healthy"
                );
            } else if previous.fcr != FcrState::Healthy && update.tip.fcr == FcrState::Healthy {
                info!(previous = %previous.fcr, "FCR is healthy again");
            }
        }
    }
}

/// Tracks the chain tip and publishes the confirmed block on a watch channel.
pub struct ChainTipTracker {
    runtime: Arc<TrackerRuntime>,
    task: Mutex<Option<JoinHandle<()>>>,
    started: AtomicBool,
}

impl fmt::Debug for ChainTipTracker {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_struct("ChainTipTracker")
            .field("confirmation", &self.runtime.confirmation)
            .field("head_poll_interval", &self.runtime.head.poll_interval)
            .field("current", &self.current())
            .finish()
    }
}

impl ChainTipTracker {
    /// Creates a tracker around the given tip source.
    pub fn new(
        source: Arc<dyn TipSource>,
        confirmation: ConfirmationConfig,
        head_poll_interval: Duration,
        expected_block_time: Duration,
    ) -> Self {
        Self::with_head_source(
            source,
            confirmation,
            HeadSourceConfig {
                chain_id: 0,
                poll_interval: head_poll_interval,
                expected_block_time,
                ws_url: None,
                ws_url_derived: false,
            },
            None,
        )
    }

    /// Creates a tracker around the given tip source and optional WebSocket head subscriber.
    pub fn with_head_source(
        source: Arc<dyn TipSource>,
        confirmation: ConfirmationConfig,
        head: HeadSourceConfig,
        subscriber: Option<Arc<dyn HeadSubscriber>>,
    ) -> Self {
        let (tip_tx, _) = watch::channel(None);

        Self {
            runtime: Arc::new(TrackerRuntime {
                source,
                confirmation,
                head,
                subscriber,
                tip_tx,
                last_success: Mutex::new(None),
                header_timestamps: Mutex::new(HeaderTimestamps::default()),
                refresh_lock: AsyncMutex::new(()),
                failure_streak: Mutex::new(0),
                invalid_streak: Mutex::new(0),
                active_source: Mutex::new(HeadSource::Polling),
                ws_reconnects: AtomicU64::new(0),
            }),
            task: Mutex::new(None),
            started: AtomicBool::new(false),
        }
    }

    /// Subscribes to confirmed tip updates.
    pub fn subscribe(&self) -> watch::Receiver<Option<ChainTip>> {
        self.runtime.tip_tx.subscribe()
    }

    /// Returns the most recently published tip.
    pub fn current(&self) -> Option<ChainTip> {
        self.runtime.current()
    }

    /// Returns the remembered header timestamp for the given block, if it was observed recently.
    pub fn header_timestamp(&self, block: u64) -> Option<u64> {
        self.runtime.header_timestamp(block)
    }

    /// Returns the number of successful WebSocket reconnects since the tracker was created.
    pub fn ws_reconnect_count(&self) -> u64 {
        self.runtime.ws_reconnects.load(Ordering::SeqCst)
    }

    /// Reads the chain tip once from the active head source and publishes the resulting confirmed tip.
    pub async fn refresh(&self) -> Result<ChainTip> {
        let source = self.runtime.active_source();
        self.runtime.refresh_from(source).await
    }

    /// Starts the background refresh task if it is not running yet.
    ///
    /// The task holds only a weak reference to the tracker runtime, so it stops once the tracker is
    /// dropped.
    pub fn ensure_started(&self) {
        if self.started.swap(true, Ordering::SeqCst) {
            return;
        }

        let runtime = Arc::downgrade(&self.runtime);
        let head = self.runtime.head.clone();
        let subscriber = self.runtime.subscriber.clone();
        let handle = tokio::spawn(run_head_driver(runtime, head, subscriber));

        if let Ok(mut task) = self.task.lock() {
            *task = Some(handle);
        }
    }

    /// Returns the confirmed block.
    ///
    /// Before the background task is started this refreshes on every call, matching a direct RPC
    /// read. Once [`ChainTipTracker::ensure_started`] has been called, a cached tip younger than the
    /// staleness limit is served directly.
    ///
    /// In safe and finalized mode the confirmed block is reported only while the tag required by
    /// the mode is present. An endpoint that reports `latest` but no such tag therefore surfaces as
    /// an error instead of a confirmed block of zero.
    pub async fn confirmed_block(&self) -> Result<u64> {
        if self.started.load(Ordering::SeqCst) {
            let max_age = self
                .runtime
                .head
                .expected_block_time
                .max(self.runtime.head.poll_interval)
                .saturating_mul(3);
            let is_fresh = self
                .runtime
                .last_success
                .lock()
                .ok()
                .and_then(|last_success| *last_success)
                .is_some_and(|last_success| last_success.elapsed() < max_age);

            if is_fresh {
                if let Some(tip) = self.current() {
                    return self.runtime.confirmed_from_tip(tip);
                }
            }
        }

        let tip = self.refresh().await?;
        self.runtime.confirmed_from_tip(tip)
    }
}

impl Drop for ChainTipTracker {
    fn drop(&mut self) {
        if let Ok(mut task) = self.task.lock() {
            if let Some(handle) = task.take() {
                handle.abort();
            }
        }
    }
}

#[cfg(all(feature = "telemetry", not(test)))]
fn fcr_state_value(state: FcrState) -> f64 {
    match state {
        FcrState::Unavailable => 0.0,
        FcrState::Degraded => 1.0,
        FcrState::Healthy => 2.0,
    }
}

#[cfg(all(feature = "telemetry", not(test)))]
fn head_source_value(source: HeadSource) -> f64 {
    match source {
        HeadSource::Polling => 0.0,
        HeadSource::WebSocket => 1.0,
    }
}

#[cfg(all(feature = "telemetry", not(test)))]
lazy_static::lazy_static! {
    static ref METRIC_CHAIN_TIP_BLOCK: Option<MultiGauge> =
        MultiGauge::new(
            "blokli_chain_tip_block",
            "Chain tip block number by tag",
            &["tag"],
        ).inspect_err(|error| error!(%error, metric = "blokli_chain_tip_block", "failed to register telemetry metric")).ok();
    static ref METRIC_SAFE_GAP_BLOCKS: Option<SimpleGauge> =
        SimpleGauge::new(
            "blokli_safe_gap_blocks",
            "Number of blocks between the latest and safe blocks",
        ).inspect_err(|error| error!(%error, metric = "blokli_safe_gap_blocks", "failed to register telemetry metric")).ok();
    static ref METRIC_FCR_STATE: Option<SimpleGauge> =
        SimpleGauge::new(
            "blokli_fcr_state",
            "Fast Confirmation Rule state (0 unavailable, 1 degraded, 2 healthy)",
        ).inspect_err(|error| error!(%error, metric = "blokli_fcr_state", "failed to register telemetry metric")).ok();
    static ref METRIC_FCR_FALLBACK_TOTAL: Option<SimpleCounter> =
        SimpleCounter::new(
            "blokli_fcr_fallback_total",
            "Transitions into the depth fallback of safe_or_depth because FCR was not healthy",
        ).inspect_err(|error| error!(%error, metric = "blokli_fcr_fallback_total", "failed to register telemetry metric")).ok();
    static ref METRIC_SAFE_REGRESSIONS_TOTAL: Option<SimpleCounter> =
        SimpleCounter::new(
            "blokli_safe_regressions_total",
            "Number of observations where the safe block moved backwards",
        ).inspect_err(|error| error!(%error, metric = "blokli_safe_regressions_total", "failed to register telemetry metric")).ok();
    static ref METRIC_HEAD_SOURCE: Option<SimpleGauge> =
        SimpleGauge::new(
            "blokli_head_source",
            "Current chain head source (0 polling, 1 websocket)",
        ).inspect_err(|error| error!(%error, metric = "blokli_head_source", "failed to register telemetry metric")).ok();
}

fn record_tip_metrics(update: &TipUpdate) {
    #[cfg(all(feature = "telemetry", not(test)))]
    {
        if let Some(metric) = METRIC_CHAIN_TIP_BLOCK.as_ref() {
            metric.set(&["latest"], update.tip.latest as f64);
            if let Some(safe) = update.tip.safe {
                metric.set(&["safe"], safe as f64);
            }
            if let Some(finalized) = update.tip.finalized {
                metric.set(&["finalized"], finalized as f64);
            }
            metric.set(&["confirmed"], update.tip.confirmed as f64);
        }

        if let Some(metric) = METRIC_SAFE_GAP_BLOCKS.as_ref() {
            if let Some(safe) = update.tip.safe {
                metric.set(update.tip.latest.saturating_sub(safe) as f64);
            }
        }

        if let Some(metric) = METRIC_FCR_STATE.as_ref() {
            metric.set(fcr_state_value(update.tip.fcr));
        }

        if let Some(metric) = METRIC_HEAD_SOURCE.as_ref() {
            metric.set(head_source_value(update.tip.source));
        }

        if update.entered_fallback {
            if let Some(metric) = METRIC_FCR_FALLBACK_TOTAL.as_ref() {
                metric.increment();
            }
        }

        if update.safe_regressed {
            if let Some(metric) = METRIC_SAFE_REGRESSIONS_TOTAL.as_ref() {
                metric.increment();
            }
        }
    }

    #[cfg(not(all(feature = "telemetry", not(test))))]
    let _ = update;
}

#[cfg(test)]
mod tests {
    use std::{
        collections::VecDeque,
        sync::{
            Arc, Mutex,
            atomic::{AtomicUsize, Ordering},
        },
        time::Duration,
    };

    use async_trait::async_trait;
    use tokio::time::sleep;
    use url::Url;

    use super::{
        ChainTip, ChainTipTracker, ConfirmationConfig, ConfirmationMode, FcrState, HeadSource, HeaderTimestamps,
        TipObservation, TipSource, next_tip,
    };
    use crate::errors::{Result, RpcError};

    const SOURCE: HeadSource = HeadSource::Polling;

    fn observation(latest: u64, safe: Option<u64>, finalized: Option<u64>) -> TipObservation {
        TipObservation {
            latest,
            latest_timestamp: Some(1_700_000_000 + latest),
            safe,
            finalized,
        }
    }

    fn config(mode: ConfirmationMode) -> ConfirmationConfig {
        ConfirmationConfig {
            mode,
            depth: 3,
            max_safe_gap: 32,
        }
    }

    fn update(previous: Option<&ChainTip>, observation: &TipObservation, cfg: &ConfirmationConfig) -> super::TipUpdate {
        next_tip(previous, observation, cfg, SOURCE).expect("observation should be valid")
    }

    #[test]
    fn test_next_tip_depth_saturates_below_zero() {
        let tip = update(None, &observation(2, None, None), &config(ConfirmationMode::Depth)).tip;
        assert_eq!(tip.confirmed, 0);
        assert_eq!(tip.fcr, FcrState::Unavailable);
    }

    #[test]
    fn test_next_tip_depth_uses_configured_depth() {
        let tip = update(
            None,
            &observation(100, Some(99), Some(80)),
            &config(ConfirmationMode::Depth),
        )
        .tip;
        assert_eq!(tip.confirmed, 97);
        assert_eq!(tip.fcr, FcrState::Healthy);
    }

    #[test]
    fn test_next_tip_fcr_boundary_at_max_safe_gap() {
        let cfg = config(ConfirmationMode::SafeOrDepth);

        let healthy = update(None, &observation(100, Some(69), None), &cfg);
        assert_eq!(healthy.tip.fcr, FcrState::Healthy);
        assert_eq!(healthy.tip.confirmed, 97);
        assert!(!healthy.entered_fallback);

        let degraded = update(None, &observation(100, Some(68), None), &cfg);
        assert_eq!(degraded.tip.fcr, FcrState::Degraded);
        assert_eq!(degraded.tip.confirmed, 97);
        assert!(degraded.entered_fallback);
    }

    #[test]
    fn test_entered_fallback_only_on_transition() {
        let cfg = config(ConfirmationMode::SafeOrDepth);

        let healthy = update(None, &observation(100, Some(99), None), &cfg);
        assert!(!healthy.entered_fallback);

        let degraded = update(Some(&healthy.tip), &observation(101, Some(60), None), &cfg);
        assert!(degraded.entered_fallback);

        let still_degraded = update(Some(&degraded.tip), &observation(102, Some(60), None), &cfg);
        assert!(!still_degraded.entered_fallback);

        let healthy_again = update(Some(&still_degraded.tip), &observation(103, Some(102), None), &cfg);
        assert!(!healthy_again.entered_fallback);

        let unavailable = update(Some(&healthy_again.tip), &observation(104, None, None), &cfg);
        assert!(unavailable.entered_fallback);
    }

    #[test]
    fn test_entered_fallback_is_never_set_in_other_modes() {
        for mode in [
            ConfirmationMode::Depth,
            ConfirmationMode::Safe,
            ConfirmationMode::Finalized,
        ] {
            let update = update(None, &observation(100, None, None), &config(mode));
            assert!(!update.entered_fallback, "{mode} must never enter the FCR fallback");
        }
    }

    #[test]
    fn test_next_tip_safe_or_depth_uses_safe_when_healthy() {
        let update = update(
            None,
            &observation(100, Some(99), Some(80)),
            &config(ConfirmationMode::SafeOrDepth),
        );

        assert_eq!(update.tip.confirmed, 99);
        assert!(!update.entered_fallback);
    }

    #[test]
    fn test_header_timestamps_lookup_and_eviction() {
        let mut timestamps = HeaderTimestamps::new(3);

        timestamps.insert(10, 1_000);
        timestamps.insert(11, 1_001);
        timestamps.insert(12, 1_002);

        assert_eq!(timestamps.get(10), Some(1_000));
        assert_eq!(timestamps.get(11), Some(1_001));
        assert_eq!(timestamps.get(12), Some(1_002));
        assert_eq!(timestamps.get(9), None);

        timestamps.insert(13, 1_003);

        assert_eq!(timestamps.get(10), None, "the oldest entry should be evicted");
        assert_eq!(timestamps.get(11), Some(1_001));
        assert_eq!(timestamps.get(13), Some(1_003));
    }

    #[test]
    fn test_header_timestamps_capacity_is_bounded() {
        let mut timestamps = HeaderTimestamps::default();

        for block in 0..=super::HEADER_TIMESTAMP_CAPACITY as u64 {
            timestamps.insert(block, block + 1_000);
        }

        assert_eq!(timestamps.entries.len(), super::HEADER_TIMESTAMP_CAPACITY);
        assert_eq!(timestamps.get(0), None);
        assert_eq!(timestamps.get(1), Some(1_001));
    }

    #[test]
    fn test_next_tip_safe_mode_keeps_previous_confirmed_when_safe_is_missing() {
        let first = update(
            None,
            &observation(100, Some(90), Some(80)),
            &config(ConfirmationMode::Safe),
        )
        .tip;
        assert_eq!(first.confirmed, 90);

        let next = update(
            Some(&first),
            &observation(120, None, Some(100)),
            &config(ConfirmationMode::Safe),
        )
        .tip;

        assert_eq!(next.confirmed, 90);
        assert_eq!(next.fcr, FcrState::Unavailable);
    }

    #[test]
    fn test_next_tip_safe_mode_without_the_tag_publishes_nothing() {
        let update = update(None, &observation(100, None, None), &config(ConfirmationMode::Safe));
        assert_eq!(update.tip.confirmed, 0);
        assert!(!update.publish, "a missing safe tag must not be published as height 0");
    }

    #[test]
    fn test_next_tip_finalized_mode_without_the_tag_publishes_nothing() {
        let update = update(
            None,
            &observation(100, Some(99), None),
            &config(ConfirmationMode::Finalized),
        );
        assert_eq!(update.tip.confirmed, 0);
        assert!(
            !update.publish,
            "a missing finalized tag must not be published as height 0"
        );
    }

    #[test]
    fn test_next_tip_safe_mode_publishes_a_valid_tag_at_genesis() {
        let update = update(None, &observation(0, Some(0), Some(0)), &config(ConfirmationMode::Safe));
        assert_eq!(update.tip.confirmed, 0);
        assert_eq!(update.tip.safe, Some(0));
        assert!(
            update.publish,
            "a tag at block 0 is a valid observation, not a missing tag"
        );
    }

    #[test]
    fn test_next_tip_keeps_publishing_after_the_required_tag_disappears() {
        let cfg = config(ConfirmationMode::Safe);
        let first = update(None, &observation(100, Some(95), Some(80)), &cfg);
        assert!(first.publish);

        let second = update(Some(&first.tip), &observation(110, None, Some(80)), &cfg);
        assert!(second.publish);
        assert_eq!(second.tip.confirmed, 95);
        assert_eq!(second.tip.safe, None);
    }

    #[test]
    fn test_next_tip_finalized_mode_keeps_previous_confirmed_when_finalized_is_missing() {
        let first = update(
            None,
            &observation(100, Some(90), Some(80)),
            &config(ConfirmationMode::Finalized),
        )
        .tip;
        assert_eq!(first.confirmed, 80);

        let next = update(
            Some(&first),
            &observation(120, Some(110), None),
            &config(ConfirmationMode::Finalized),
        )
        .tip;

        assert_eq!(next.confirmed, 80);
    }

    #[test]
    fn test_next_tip_never_moves_confirmed_backwards() {
        let cfg = config(ConfirmationMode::Depth);
        let first = update(None, &observation(100, None, None), &cfg).tip;
        assert_eq!(first.confirmed, 97);

        let next = update(Some(&first), &observation(98, None, None), &cfg).tip;
        assert_eq!(next.confirmed, 97);
        assert_eq!(next.latest, 98);
    }

    #[test]
    fn test_next_tip_reports_safe_regression() {
        let cfg = config(ConfirmationMode::Safe);
        let first = update(None, &observation(100, Some(95), Some(80)), &cfg).tip;
        let next = update(Some(&first), &observation(100, Some(90), Some(80)), &cfg);

        assert!(next.safe_regressed);
        assert_eq!(next.tip.confirmed, 95);
    }

    #[test]
    fn test_next_tip_rejects_safe_above_latest() {
        let error = next_tip(
            None,
            &observation(100, Some(101), None),
            &config(ConfirmationMode::Safe),
            SOURCE,
        )
        .expect_err("safe above latest must be rejected");

        assert!(error.to_string().contains("safe block"));
    }

    #[test]
    fn test_next_tip_rejects_finalized_above_safe() {
        let error = next_tip(
            None,
            &observation(100, Some(90), Some(91)),
            &config(ConfirmationMode::Finalized),
            SOURCE,
        )
        .expect_err("finalized above safe must be rejected");

        assert!(error.to_string().contains("finalized block"));
    }

    #[test]
    fn test_next_tip_rejects_finalized_above_latest() {
        let error = next_tip(
            None,
            &observation(100, None, Some(101)),
            &config(ConfirmationMode::Finalized),
            SOURCE,
        )
        .expect_err("finalized above latest must be rejected");

        assert!(error.to_string().contains("finalized block"));
    }

    #[test]
    fn test_confirmation_mode_strings_round_trip() {
        for mode in [
            ConfirmationMode::Depth,
            ConfirmationMode::SafeOrDepth,
            ConfirmationMode::Safe,
            ConfirmationMode::Finalized,
        ] {
            assert_eq!(mode.to_string().parse::<ConfirmationMode>(), Ok(mode));
        }
    }

    #[test]
    fn test_serde_values_match_display_values() {
        for mode in [
            ConfirmationMode::Depth,
            ConfirmationMode::SafeOrDepth,
            ConfirmationMode::Safe,
            ConfirmationMode::Finalized,
        ] {
            let serialized = serde_json::to_string(&mode).expect("mode should serialize");
            assert_eq!(serialized, format!("\"{mode}\""));
        }

        for state in [FcrState::Unavailable, FcrState::Degraded, FcrState::Healthy] {
            let serialized = serde_json::to_string(&state).expect("state should serialize");
            assert_eq!(serialized, format!("\"{state}\""));
        }

        for source in [HeadSource::Polling, HeadSource::WebSocket] {
            let serialized = serde_json::to_string(&source).expect("source should serialize");
            assert_eq!(serialized, format!("\"{source}\""));
        }
    }

    #[test]
    fn test_fcr_and_head_source_strings_round_trip() {
        for state in [FcrState::Unavailable, FcrState::Degraded, FcrState::Healthy] {
            assert_eq!(state.to_string().parse::<FcrState>(), Ok(state));
        }
        for source in [HeadSource::Polling, HeadSource::WebSocket] {
            assert_eq!(source.to_string().parse::<HeadSource>(), Ok(source));
        }
    }

    #[derive(Clone)]
    struct MockTipSource {
        observations: Arc<Mutex<VecDeque<Result<TipObservation>>>>,
    }

    impl MockTipSource {
        fn new(observations: Vec<Result<TipObservation>>) -> Self {
            Self {
                observations: Arc::new(Mutex::new(observations.into())),
            }
        }
    }

    #[async_trait]
    impl TipSource for MockTipSource {
        async fn observe(&self) -> Result<TipObservation> {
            self.observations
                .lock()
                .expect("observations lock should not be poisoned")
                .pop_front()
                .unwrap_or_else(|| Err(RpcError::Other("no observation left".to_string())))
        }
    }

    fn tracker_with(source: MockTipSource, poll_interval: Duration, expected_block_time: Duration) -> ChainTipTracker {
        ChainTipTracker::new(
            Arc::new(source),
            config(ConfirmationMode::Depth),
            poll_interval,
            expected_block_time,
        )
    }

    #[tokio::test]
    async fn test_tracker_publishes_on_refresh() {
        let source = MockTipSource::new(vec![Ok(observation(100, Some(99), Some(80)))]);
        let tracker = tracker_with(source, Duration::from_secs(1), Duration::from_secs(1));
        let mut receiver = tracker.subscribe();

        let published = tracker.refresh().await.expect("refresh should succeed");

        receiver.changed().await.expect("a tip should be published");
        assert_eq!(receiver.borrow().as_ref(), Some(&published));
        assert_eq!(tracker.current(), Some(published));
    }

    #[tokio::test]
    async fn test_confirmed_block_waits_for_the_safe_tag() {
        let source = MockTipSource::new(vec![
            Ok(observation(100, None, Some(80))),
            Ok(observation(100, Some(95), Some(80))),
        ]);
        let tracker = ChainTipTracker::new(
            Arc::new(source),
            config(ConfirmationMode::Safe),
            Duration::from_secs(1),
            Duration::from_secs(1),
        );

        let error = tracker
            .confirmed_block()
            .await
            .expect_err("a missing safe tag must surface as an error");
        assert!(error.to_string().contains("safe"), "{error}");
        assert_eq!(tracker.current(), None, "no tip is published without the safe tag");

        assert_eq!(
            tracker
                .confirmed_block()
                .await
                .expect("the safe tag should confirm a block"),
            95
        );
        assert!(tracker.current().is_some());
    }

    #[tokio::test]
    async fn test_confirmed_block_errors_when_the_safe_tag_disappears() {
        let source = MockTipSource::new(vec![
            Ok(observation(100, Some(95), Some(80))),
            Ok(observation(110, None, Some(80))),
        ]);
        let tracker = ChainTipTracker::new(
            Arc::new(source),
            config(ConfirmationMode::Safe),
            Duration::from_secs(1),
            Duration::from_secs(1),
        );

        assert_eq!(
            tracker.confirmed_block().await.expect("the safe tag should confirm"),
            95
        );
        let error = tracker
            .confirmed_block()
            .await
            .expect_err("a disappeared safe tag must surface as an error");
        assert!(error.to_string().contains("safe"), "{error}");
        assert_eq!(
            tracker.current().map(|tip| tip.confirmed),
            Some(95),
            "the last height stays published for diagnostics"
        );
    }

    #[tokio::test]
    async fn test_confirmed_block_accepts_a_safe_tag_at_genesis() {
        let source = MockTipSource::new(vec![Ok(observation(0, Some(0), Some(0)))]);
        let tracker = ChainTipTracker::new(
            Arc::new(source),
            config(ConfirmationMode::Safe),
            Duration::from_secs(1),
            Duration::from_secs(1),
        );

        assert_eq!(tracker.confirmed_block().await.expect("a genesis safe tag is valid"), 0);
        assert_eq!(tracker.current().and_then(|tip| tip.safe), Some(0));
    }

    #[tokio::test]
    async fn test_tracker_debug_redacts_the_websocket_url() {
        let tracker = ChainTipTracker::with_head_source(
            Arc::new(MockTipSource::new(vec![])),
            config(ConfirmationMode::Depth),
            crate::head::HeadSourceConfig {
                chain_id: 100,
                poll_interval: Duration::from_secs(1),
                expected_block_time: Duration::from_secs(5),
                ws_url: Some(
                    Url::parse("wss://user:pass@rpc.example.com:8443/secret?token=xyz").expect("URL should parse"),
                ),
                ws_url_derived: true,
            },
            None,
        );

        let debug = format!("{:?}", tracker.runtime);
        assert!(debug.contains("wss://rpc.example.com:8443"), "{debug}");
        for secret in ["user", "pass", "secret", "xyz"] {
            assert!(!debug.contains(secret), "{debug}");
        }
    }

    #[tokio::test]
    async fn test_tracker_keeps_previous_tip_on_invalid_observation() {
        let valid = observation(100, Some(99), Some(80));
        let source = MockTipSource::new(vec![Ok(valid), Ok(observation(90, Some(91), Some(80)))]);
        let tracker = tracker_with(source, Duration::from_secs(1), Duration::from_secs(1));

        let first = tracker.refresh().await.expect("first refresh should succeed");
        let error = tracker.refresh().await.expect_err("invalid observation should fail");

        assert!(error.to_string().contains("safe block"));
        assert_eq!(tracker.current(), Some(first));
    }

    #[tokio::test]
    async fn test_tracker_keeps_previous_tip_on_source_error() {
        let valid = observation(100, Some(99), Some(80));
        let source = MockTipSource::new(vec![Ok(valid), Err(RpcError::Other("rpc failed".to_string()))]);
        let tracker = tracker_with(source, Duration::from_secs(1), Duration::from_secs(1));

        let first = tracker.refresh().await.expect("first refresh should succeed");
        let error = tracker.refresh().await.expect_err("source error should surface");

        assert!(error.to_string().contains("rpc failed"));
        assert_eq!(tracker.current(), Some(first));
    }

    #[tokio::test]
    async fn test_confirmed_block_refreshes_on_every_call_without_background() {
        let source = MockTipSource::new(vec![
            Ok(observation(100, Some(99), Some(80))),
            Ok(observation(110, Some(109), Some(90))),
        ]);
        let tracker = tracker_with(source, Duration::from_secs(1), Duration::from_secs(60));

        assert_eq!(tracker.confirmed_block().await.expect("first read should succeed"), 97);
        assert_eq!(
            tracker.confirmed_block().await.expect("second read should refresh"),
            107
        );
    }

    #[tokio::test]
    async fn test_confirmed_block_uses_cached_tip_after_background_started() {
        let count = Arc::new(AtomicUsize::new(0));
        let source = CountingSource {
            count: count.clone(),
            observation: observation(100, Some(99), Some(80)),
        };
        let tracker = ChainTipTracker::new(
            Arc::new(source),
            config(ConfirmationMode::Depth),
            Duration::from_secs(3600),
            Duration::from_secs(60),
        );

        tracker.ensure_started();

        for _ in 0..100 {
            if tracker.current().is_some() {
                break;
            }
            sleep(Duration::from_millis(5)).await;
        }
        assert!(tracker.current().is_some(), "background refresh should publish a tip");

        let before = count.load(Ordering::SeqCst);
        assert_eq!(tracker.confirmed_block().await.expect("cached read should succeed"), 97);
        assert_eq!(
            count.load(Ordering::SeqCst),
            before,
            "a fresh cached tip should be served after the background task starts"
        );
    }

    #[derive(Clone)]
    struct SuccessThenErrorSource {
        calls: Arc<AtomicUsize>,
    }

    #[async_trait]
    impl TipSource for SuccessThenErrorSource {
        async fn observe(&self) -> Result<TipObservation> {
            if self.calls.fetch_add(1, Ordering::SeqCst) == 0 {
                Ok(observation(100, Some(99), Some(80)))
            } else {
                Err(RpcError::Other("rpc failed".to_string()))
            }
        }
    }

    #[tokio::test]
    async fn test_confirmed_block_refreshes_when_stale_and_returns_source_error() {
        let tracker = ChainTipTracker::new(
            Arc::new(SuccessThenErrorSource {
                calls: Arc::new(AtomicUsize::new(0)),
            }),
            config(ConfirmationMode::Depth),
            Duration::from_millis(10),
            Duration::from_millis(20),
        );

        tracker.ensure_started();

        for _ in 0..100 {
            if tracker.current().is_some() {
                break;
            }
            sleep(Duration::from_millis(5)).await;
        }
        assert!(tracker.current().is_some(), "background refresh should publish a tip");

        sleep(Duration::from_millis(100)).await;

        let error = tracker
            .confirmed_block()
            .await
            .expect_err("a stale tip must be refreshed and surface the source error");

        assert!(error.to_string().contains("rpc failed"));
    }

    #[derive(Clone)]
    struct SlowFirstSource {
        calls: Arc<AtomicUsize>,
    }

    #[async_trait]
    impl TipSource for SlowFirstSource {
        async fn observe(&self) -> Result<TipObservation> {
            if self.calls.fetch_add(1, Ordering::SeqCst) == 0 {
                sleep(Duration::from_millis(100)).await;
                Ok(observation(100, None, None))
            } else {
                Ok(observation(120, None, None))
            }
        }
    }

    #[tokio::test]
    async fn test_concurrent_refreshes_never_move_confirmed_backwards() {
        let tracker = ChainTipTracker::new(
            Arc::new(SlowFirstSource {
                calls: Arc::new(AtomicUsize::new(0)),
            }),
            config(ConfirmationMode::Depth),
            Duration::from_secs(1),
            Duration::from_secs(1),
        );

        let (first, second) = tokio::join!(tracker.refresh(), tracker.refresh());
        first.expect("first refresh should succeed");
        second.expect("second refresh should succeed");

        let current = tracker.current().expect("a tip should be published");
        assert_eq!(current.latest, 120);
        assert_eq!(
            current.confirmed, 117,
            "the newer confirmed tip must not be overwritten"
        );
    }

    #[derive(Clone)]
    struct CountingSource {
        count: Arc<AtomicUsize>,
        observation: TipObservation,
    }

    #[async_trait]
    impl TipSource for CountingSource {
        async fn observe(&self) -> Result<TipObservation> {
            self.count.fetch_add(1, Ordering::SeqCst);
            Ok(self.observation)
        }
    }

    #[tokio::test]
    async fn test_background_task_stops_on_drop() {
        let count = Arc::new(AtomicUsize::new(0));
        let source = CountingSource {
            count: count.clone(),
            observation: observation(100, Some(99), Some(80)),
        };
        let tracker = ChainTipTracker::new(
            Arc::new(source),
            config(ConfirmationMode::Depth),
            Duration::from_millis(10),
            Duration::from_millis(10),
        );

        tracker.ensure_started();

        for _ in 0..100 {
            if count.load(Ordering::SeqCst) > 0 {
                break;
            }
            sleep(Duration::from_millis(5)).await;
        }
        assert!(count.load(Ordering::SeqCst) > 0, "background refresh should run");

        drop(tracker);

        sleep(Duration::from_millis(50)).await;
        let after_drop = count.load(Ordering::SeqCst);
        sleep(Duration::from_millis(50)).await;
        assert_eq!(
            count.load(Ordering::SeqCst),
            after_drop,
            "background task should stop after the tracker is dropped"
        );
    }
}
