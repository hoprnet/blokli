//! Chain tip tracking and confirmation mode handling.
//!
//! The tracker reads the chain head through a [`TipSource`]. It derives a confirmed block number
//! with the configured [`ConfirmationMode`]. The tracker publishes each successful update on a
//! `tokio::sync::watch` channel. Consumers can wait for the confirmed tip on that channel, and they
//! do not have to poll the RPC endpoint.

use std::{
    collections::VecDeque,
    fmt::{self, Formatter},
    result::Result as StdResult,
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
use thiserror::Error;
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

/// Rule that derives the confirmed block from the observed chain tip.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, Display, EnumString)]
#[serde(rename_all = "snake_case")]
#[strum(serialize_all = "snake_case")]
pub enum ConfirmationMode {
    /// Use a fixed number of blocks behind the chain head.
    #[default]
    Depth,
    /// Use the safe tip if it is healthy. If not, use the configured depth.
    SafeOrDepth,
    /// Use the safe tip only.
    Safe,
    /// Use the finalized tip only.
    Finalized,
}

/// Health of the Fast Confirmation Rule, as the safe block tag shows it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Display, EnumString)]
#[serde(rename_all = "snake_case")]
#[strum(serialize_all = "snake_case")]
pub enum FcrState {
    /// The endpoint does not expose a safe block.
    Unavailable,
    /// The safe block is too far behind the latest block.
    Degraded,
    /// The safe block is near enough to the latest block to be useful.
    Healthy,
}

/// Source of the head updates that the tracker uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Display, EnumString)]
#[serde(rename_all = "snake_case")]
#[strum(serialize_all = "snake_case")]
pub enum HeadSource {
    /// The tracker polls the head over HTTP.
    Polling,
    /// A WebSocket subscription delivers the head.
    #[serde(rename = "websocket")]
    #[strum(serialize = "websocket")]
    WebSocket,
}

/// A chain tip with the confirmed block derived from it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChainTip {
    /// Latest block that the endpoint reports.
    pub latest: u64,
    /// Safe block that the endpoint reports, if available.
    pub safe: Option<u64>,
    /// Finalized block that the endpoint reports, if available.
    pub finalized: Option<u64>,
    /// Block that the configured confirmation mode treats as confirmed.
    ///
    /// This value is monotonic. It is the indexing watermark and never moves backwards.
    pub confirmed: u64,
    /// Source of the observation.
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
    /// Safe block number, if the endpoint reports one.
    pub safe: Option<u64>,
    /// Timestamp of the safe block header in seconds since the Unix epoch.
    pub safe_timestamp: Option<u64>,
    /// Finalized block number, if the endpoint reports one.
    pub finalized: Option<u64>,
    /// Timestamp of the finalized block header in seconds since the Unix epoch.
    pub finalized_timestamp: Option<u64>,
}

/// Number of recent header timestamps that the tracker keeps.
const HEADER_TIMESTAMP_CAPACITY: usize = 256;

/// Number of intervals after which a cached tip is stale.
///
/// The interval is the larger value of the expected block time and the head poll interval. The log
/// stream of the indexer also waits for a tip update for at most this many intervals.
pub(crate) const TIP_STALENESS_INTERVALS: u32 = 3;

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
    /// Number of blocks behind the latest block for the depth-based modes.
    pub depth: u64,
    /// Smallest gap from the latest block to the safe block that makes FCR degraded.
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

/// Result of one observation that the tracker applied.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TipUpdate {
    /// New tip to publish.
    pub tip: ChainTip,
    /// `true` if the tracker can publish the tip on the watch channel.
    ///
    /// A mode that needs the safe or finalized tag publishes nothing until the tracker observes that
    /// tag at least once. Thus a missing tag never becomes a confirmation height of zero. When a tip
    /// exists, later observations without the tag keep the last height.
    pub publish: bool,
    /// `true` if the safe block moved backwards compared with the previous tip.
    pub safe_regressed: bool,
    /// `true` if this update entered the safe-or-depth fallback after an update without the fallback.
    pub entered_fallback: bool,
}

/// Errors from deriving or tracking the confirmed tip.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum TipError {
    /// The observation violates the ordering between latest, safe and finalized.
    #[error("invalid chain tip observation: {0}")]
    InvalidObservation(String),
}

/// Computes the next tip from a previous tip and a raw observation.
///
/// # Errors
///
/// Returns [`TipError::InvalidObservation`] if the safe or finalized block is above the latest block,
/// or if the finalized block is above the safe block.
pub fn next_tip(
    previous: Option<&ChainTip>,
    observation: &TipObservation,
    cfg: &ConfirmationConfig,
    source: HeadSource,
) -> StdResult<TipUpdate, TipError> {
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
    /// Reads the current chain tip from the endpoint.
    ///
    /// # Errors
    ///
    /// Returns an [`RpcError`] if the endpoint cannot supply the latest block.
    async fn observe(&self) -> Result<TipObservation>;

    /// Reads the block hash for the given block number from the endpoint.
    ///
    /// The default implementation tells that the lookup is not available. Thus a source that
    /// implements only [`TipSource::observe`] continues to work outside the WebSocket startup probe.
    ///
    /// # Errors
    ///
    /// Returns an [`RpcError`] if the lookup fails or if the endpoint does not know the block.
    async fn block_hash(&self, _block: u64) -> Result<B256> {
        Err(RpcError::Other("block hash lookup is not supported".to_string()))
    }
}

#[derive(Debug, Clone, Deserialize)]
struct RpcBlockHeader {
    number: U64,
    timestamp: U64,
}

/// Production [`TipSource`] that reads the latest, safe and finalized headers in one batched RPC
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

        let (safe, safe_timestamp) = match safe_waiter.await {
            Ok(Some(header)) => (Some(header.number.to::<u64>()), Some(header.timestamp.to::<u64>())),
            Ok(None) => (None, None),
            Err(error) => {
                debug!(%error, tag = "safe", "failed to read the chain tip block tag");
                (None, None)
            }
        };
        let (finalized, finalized_timestamp) = match finalized_waiter.await {
            Ok(Some(header)) => (Some(header.number.to::<u64>()), Some(header.timestamp.to::<u64>())),
            Ok(None) => (None, None),
            Err(error) => {
                debug!(%error, tag = "finalized", "failed to read the chain tip block tag");
                (None, None)
            }
        };

        Ok(TipObservation {
            latest: latest.number.to::<u64>(),
            latest_timestamp: Some(latest.timestamp.to::<u64>()),
            safe,
            safe_timestamp,
            finalized,
            finalized_timestamp,
        })
    }

    async fn block_hash(&self, block: u64) -> Result<B256> {
        self.provider
            .get_block_by_number(BlockNumberOrTag::Number(block))
            .await
            .map_err(RpcError::from)?
            .map(|block| block.header.hash)
            .ok_or_else(|| RpcError::Other(format!("block {block} is not available")))
    }
}

struct TrackerState {
    last_success: Option<Instant>,
    header_timestamps: HeaderTimestamps,
    failure_streak: u32,
    invalid_streak: u32,
    active_source: HeadSource,
}

impl Default for TrackerState {
    fn default() -> Self {
        Self {
            last_success: None,
            header_timestamps: HeaderTimestamps::default(),
            failure_streak: 0,
            invalid_streak: 0,
            active_source: HeadSource::Polling,
        }
    }
}

pub(crate) struct TrackerRuntime {
    source: Arc<dyn TipSource>,
    confirmation: ConfirmationConfig,
    head: HeadSourceConfig,
    subscriber: Option<Arc<dyn HeadSubscriber>>,
    tip_tx: watch::Sender<Option<ChainTip>>,
    state: Mutex<TrackerState>,
    refresh_lock: AsyncMutex<()>,
    ws_reconnects: AtomicU64,
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
        self.state
            .lock()
            .map(|state| state.active_source)
            .unwrap_or(HeadSource::Polling)
    }

    pub(crate) fn set_active_source(&self, source: HeadSource) {
        if let Ok(mut state) = self.state.lock() {
            state.active_source = source;
        }
    }

    fn current(&self) -> Option<ChainTip> {
        *self.tip_tx.borrow()
    }

    fn is_fresh(&self, max_age: Duration) -> bool {
        self.state
            .lock()
            .ok()
            .and_then(|state| state.last_success)
            .is_some_and(|last_success| last_success.elapsed() < max_age)
    }

    /// Returns the confirmed block of a tip. The tip must have the tag that the mode selects.
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
        self.state
            .lock()
            .ok()
            .and_then(|state| state.header_timestamps.get(block))
    }

    fn record_source_failure(&self, error: &RpcError) {
        let first = self.state.lock().map_or(true, |mut state| {
            let first = state.failure_streak == 0;
            state.failure_streak = state.failure_streak.saturating_add(1);
            first
        });
        if first {
            warn!(%error, "failed to observe chain tip, keeping the previous tip");
        } else {
            debug!(%error, "failed to observe chain tip, keeping the previous tip");
        }
    }

    fn record_invalid_observation(&self, error: &TipError) {
        let first = self.state.lock().map_or(true, |mut state| {
            let first = state.invalid_streak == 0;
            state.invalid_streak = state.invalid_streak.saturating_add(1);
            first
        });
        if first {
            warn!(%error, "invalid chain tip observation, keeping the previous tip");
        } else {
            debug!(%error, "invalid chain tip observation, keeping the previous tip");
        }
    }

    fn record_success(&self) {
        if let Ok(mut state) = self.state.lock() {
            let recovered = state.failure_streak > 0 || state.invalid_streak > 0;
            state.failure_streak = 0;
            state.invalid_streak = 0;
            state.last_success = Some(Instant::now());
            if recovered {
                info!("chain tip observations succeeded again");
            }
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

        if let Ok(mut state) = self.state.lock() {
            for (block, timestamp) in [
                (Some(observation.latest), observation.latest_timestamp),
                (observation.safe, observation.safe_timestamp),
                (observation.finalized, observation.finalized_timestamp),
            ] {
                if let (Some(block), Some(timestamp)) = (block, timestamp) {
                    state.header_timestamps.insert(block, timestamp);
                }
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

        let Some(previous) = previous else {
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
        };

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
    /// Creates a tracker for the given tip source.
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

    /// Creates a tracker for the given tip source and an optional WebSocket head subscriber.
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
                state: Mutex::new(TrackerState::default()),
                refresh_lock: AsyncMutex::new(()),
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

    /// Returns the last published tip.
    pub fn current(&self) -> Option<ChainTip> {
        self.runtime.current()
    }

    /// Returns the confirmation mode configured for this tracker.
    pub fn confirmation_mode(&self) -> ConfirmationMode {
        self.runtime.confirmation.mode
    }

    /// Returns the cached header timestamp for the given block, if the tracker observed it recently.
    pub fn header_timestamp(&self, block: u64) -> Option<u64> {
        self.runtime.header_timestamp(block)
    }

    /// Returns the number of successful WebSocket reconnects since the creation of the tracker.
    pub fn ws_reconnect_count(&self) -> u64 {
        self.runtime.ws_reconnects.load(Ordering::SeqCst)
    }

    /// Reads the chain tip once from the active head source and publishes the confirmed tip.
    ///
    /// # Errors
    ///
    /// Returns an [`RpcError`] if the tip source fails or if the observation is not valid. The
    /// tracker then keeps the previous tip.
    pub async fn refresh(&self) -> Result<ChainTip> {
        let source = self.runtime.active_source();
        self.runtime.refresh_from(source).await
    }

    /// Starts the background refresh task if it is not running.
    ///
    /// The task holds only a weak reference to the tracker runtime. Thus the task stops when the
    /// tracker is dropped.
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
    /// Before the background task starts, each call refreshes the tip, the same as a direct RPC read.
    /// After a call to [`ChainTipTracker::ensure_started`], the function returns a cached tip if that
    /// tip is younger than the staleness limit.
    ///
    /// In safe and finalized mode, the function returns the confirmed block only while the tag for
    /// the mode is present. If an endpoint reports `latest` but not that tag, the result is an error,
    /// not a confirmed block of zero.
    ///
    /// # Errors
    ///
    /// Returns an [`RpcError`] if a refresh fails, or if the tag that the mode needs is missing.
    pub async fn confirmed_block(&self) -> Result<u64> {
        if self.started.load(Ordering::SeqCst) {
            let max_age = self
                .runtime
                .head
                .expected_block_time
                .max(self.runtime.head.poll_interval)
                .saturating_mul(TIP_STALENESS_INTERVALS);

            if self.runtime.is_fresh(max_age) {
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
        ChainTip, ChainTipTracker, ConfirmationConfig, ConfirmationMode, FcrState, HEADER_TIMESTAMP_CAPACITY,
        HeadSource, HeaderTimestamps, StdResult, TipObservation, TipSource, TipUpdate, next_tip,
    };
    use crate::{
        errors::{Result, RpcError},
        head::HeadSourceConfig,
    };

    const SOURCE: HeadSource = HeadSource::Polling;

    fn observation(latest: u64, safe: Option<u64>, finalized: Option<u64>) -> TipObservation {
        TipObservation {
            latest,
            latest_timestamp: Some(1_700_000_000 + latest),
            safe,
            safe_timestamp: safe.map(|block| 1_700_000_000 + block),
            finalized,
            finalized_timestamp: finalized.map(|block| 1_700_000_000 + block),
        }
    }

    fn config(mode: ConfirmationMode) -> ConfirmationConfig {
        ConfirmationConfig {
            mode,
            depth: 3,
            max_safe_gap: 32,
        }
    }

    fn update(previous: Option<&ChainTip>, observation: &TipObservation, cfg: &ConfirmationConfig) -> TipUpdate {
        next_tip(previous, observation, cfg, SOURCE).expect("observation should be valid")
    }

    #[test]
    fn test_next_tip_modes_boundaries_and_monotonicity() {
        // Depth mode saturates at zero and uses the configured depth.
        let t0 = update(None, &observation(2, None, None), &config(ConfirmationMode::Depth)).tip;
        assert_eq!((t0.confirmed, t0.fcr), (0, FcrState::Unavailable));
        let t1 = update(
            None,
            &observation(100, Some(99), Some(80)),
            &config(ConfirmationMode::Depth),
        )
        .tip;
        assert_eq!((t1.confirmed, t1.fcr), (97, FcrState::Healthy));

        // SafeOrDepth uses safe while FCR is healthy. It falls back at gap == max_safe_gap (32). entered_fallback is
        // set only on the transition.
        let sod = config(ConfirmationMode::SafeOrDepth);
        let u1 = update(None, &observation(100, Some(99), Some(80)), &sod);
        assert_eq!(
            (u1.tip.confirmed, u1.tip.fcr, u1.entered_fallback),
            (99, FcrState::Healthy, false)
        );
        let u2 = update(None, &observation(100, Some(69), None), &sod);
        assert_eq!(
            (u2.tip.confirmed, u2.tip.fcr, u2.entered_fallback),
            (97, FcrState::Healthy, false)
        );
        let u3 = update(Some(&u1.tip), &observation(101, Some(69), None), &sod);
        assert_eq!(
            (u3.tip.confirmed, u3.tip.fcr, u3.entered_fallback),
            (99, FcrState::Degraded, true)
        );
        let u4 = update(Some(&u3.tip), &observation(102, Some(60), None), &sod);
        assert!(!u4.entered_fallback);
        let u5 = update(Some(&u4.tip), &observation(103, Some(102), None), &sod);
        assert_eq!(
            (u5.tip.confirmed, u5.tip.fcr, u5.entered_fallback),
            (102, FcrState::Healthy, false)
        );
        let u6 = update(Some(&u5.tip), &observation(104, None, None), &sod);
        assert!(u6.entered_fallback);

        for mode in [
            ConfirmationMode::Depth,
            ConfirmationMode::Safe,
            ConfirmationMode::Finalized,
        ] {
            assert!(!update(None, &observation(100, None, None), &config(mode)).entered_fallback);
        }

        // Safe and Finalized publish nothing while the tag is missing at the start. Genesis 0 publishes. A tag that
        // goes missing later keeps the previous confirmed block.
        assert!(!update(None, &observation(100, None, None), &config(ConfirmationMode::Safe)).publish);
        assert!(
            !update(
                None,
                &observation(100, Some(99), None),
                &config(ConfirmationMode::Finalized)
            )
            .publish
        );
        let genesis = update(None, &observation(0, Some(0), Some(0)), &config(ConfirmationMode::Safe));
        assert!(genesis.publish && genesis.tip.confirmed == 0 && genesis.tip.safe == Some(0));

        let safe_first = update(
            None,
            &observation(100, Some(95), Some(80)),
            &config(ConfirmationMode::Safe),
        )
        .tip;
        let safe_missing = update(
            Some(&safe_first),
            &observation(120, None, Some(100)),
            &config(ConfirmationMode::Safe),
        );
        assert!(
            safe_missing.publish && safe_missing.tip.confirmed == 95 && safe_missing.tip.fcr == FcrState::Unavailable
        );

        let fin_first = update(
            None,
            &observation(100, Some(90), Some(80)),
            &config(ConfirmationMode::Finalized),
        )
        .tip;
        let fin_missing = update(
            Some(&fin_first),
            &observation(120, Some(110), None),
            &config(ConfirmationMode::Finalized),
        );
        assert!(fin_missing.publish && fin_missing.tip.confirmed == 80);

        // Monotonicity and safe regression flag.
        let reg = update(
            Some(&safe_first),
            &observation(100, Some(90), Some(80)),
            &config(ConfirmationMode::Safe),
        );
        assert!(reg.safe_regressed && reg.tip.confirmed == 95);
        let back = update(
            Some(&t1),
            &observation(98, None, None),
            &config(ConfirmationMode::Depth),
        )
        .tip;
        assert_eq!((back.confirmed, back.latest), (97, 98));

        // The function rejects orderings that are not valid.
        for (obs, needle) in [
            (observation(100, Some(101), None), "safe block"),
            (observation(100, Some(90), Some(91)), "finalized block"),
            (observation(100, None, Some(101)), "finalized block"),
        ] {
            let err = next_tip(None, &obs, &config(ConfirmationMode::Safe), SOURCE).unwrap_err();
            assert!(err.to_string().contains(needle), "{err}");
        }
    }

    #[test]
    fn test_header_timestamps_eviction_and_enum_serialization() {
        let mut timestamps = HeaderTimestamps::new(3);
        for (b, t) in [(10, 1_000), (11, 1_001), (12, 1_002), (13, 1_003)] {
            timestamps.insert(b, t);
        }
        assert_eq!(timestamps.get(10), None);
        assert_eq!(timestamps.get(11), Some(1_001));
        assert_eq!(timestamps.get(13), Some(1_003));

        let mut bounded = HeaderTimestamps::default();
        for block in 0..=HEADER_TIMESTAMP_CAPACITY as u64 {
            bounded.insert(block, block + 1_000);
        }
        assert_eq!(bounded.entries.len(), HEADER_TIMESTAMP_CAPACITY);
        assert_eq!(bounded.get(0), None);

        for mode in [
            ConfirmationMode::Depth,
            ConfirmationMode::SafeOrDepth,
            ConfirmationMode::Safe,
            ConfirmationMode::Finalized,
        ] {
            assert_eq!(mode.to_string().parse::<ConfirmationMode>(), Ok(mode));
            assert_eq!(serde_json::to_string(&mode).unwrap(), format!("\"{mode}\""));
        }
        assert_eq!(serde_json::to_string(&HeadSource::WebSocket).unwrap(), "\"websocket\"");
    }

    #[derive(Clone)]
    struct MockTipSource {
        script: Arc<Mutex<VecDeque<Result<TipObservation>>>>,
        fallback: Option<StdResult<TipObservation, String>>,
        calls: Arc<AtomicUsize>,
        first_call_delay: Duration,
    }

    impl MockTipSource {
        fn scripted(observations: Vec<Result<TipObservation>>) -> Self {
            Self {
                script: Arc::new(Mutex::new(observations.into())),
                fallback: None,
                calls: Arc::new(AtomicUsize::new(0)),
                first_call_delay: Duration::ZERO,
            }
        }

        fn repeating(obs: TipObservation) -> Self {
            Self {
                script: Arc::new(Mutex::new(VecDeque::new())),
                fallback: Some(Ok(obs)),
                calls: Arc::new(AtomicUsize::new(0)),
                first_call_delay: Duration::ZERO,
            }
        }

        fn calls(&self) -> usize {
            self.calls.load(Ordering::SeqCst)
        }
    }

    #[async_trait]
    impl TipSource for MockTipSource {
        async fn observe(&self) -> Result<TipObservation> {
            let call = self.calls.fetch_add(1, Ordering::SeqCst);
            if call == 0 && !self.first_call_delay.is_zero() {
                sleep(self.first_call_delay).await;
            }
            if let Some(next) = self.script.lock().unwrap().pop_front() {
                return next;
            }
            match &self.fallback {
                Some(Ok(obs)) => Ok(*obs),
                Some(Err(msg)) => Err(RpcError::Other(msg.clone())),
                None => Err(RpcError::Other("no observation left".to_string())),
            }
        }
    }

    fn tracker_with(source: MockTipSource, mode: ConfirmationMode) -> ChainTipTracker {
        ChainTipTracker::new(
            Arc::new(source),
            config(mode),
            Duration::from_secs(1),
            Duration::from_secs(1),
        )
    }

    #[tokio::test]
    async fn test_tracker_refresh_safe_tag_errors_and_debug_redaction() {
        // A refresh publishes the tip. An invalid observation or a source error keeps the previous tip.
        let source = MockTipSource::scripted(vec![
            Ok(observation(100, Some(99), Some(80))),
            Ok(observation(90, Some(91), Some(80))),
            Err(RpcError::Other("rpc failed".to_string())),
        ]);
        let tracker = tracker_with(source, ConfirmationMode::Depth);
        let mut rx = tracker.subscribe();
        let first = tracker.refresh().await.unwrap();
        rx.changed().await.unwrap();
        assert_eq!(rx.borrow().as_ref(), Some(&first));
        assert_eq!(tracker.confirmation_mode(), ConfirmationMode::Depth);
        assert_eq!(tracker.header_timestamp(100), Some(1_700_000_100));
        assert_eq!(tracker.header_timestamp(99), Some(1_700_000_099));
        assert_eq!(tracker.header_timestamp(80), Some(1_700_000_080));
        assert!(tracker.refresh().await.unwrap_err().to_string().contains("safe block"));
        assert!(tracker.refresh().await.unwrap_err().to_string().contains("rpc failed"));
        assert_eq!(tracker.current(), Some(first));

        // Safe mode waits for the safe tag and accepts genesis 0. It returns an error if safe disappears later.
        let safe_tracker = tracker_with(
            MockTipSource::scripted(vec![
                Ok(observation(100, None, Some(80))),
                Ok(observation(100, Some(95), Some(80))),
                Ok(observation(110, None, Some(80))),
            ]),
            ConfirmationMode::Safe,
        );
        assert!(
            safe_tracker
                .confirmed_block()
                .await
                .unwrap_err()
                .to_string()
                .contains("safe")
        );
        assert_eq!(safe_tracker.current(), None);
        assert_eq!(safe_tracker.confirmed_block().await.unwrap(), 95);
        assert!(
            safe_tracker
                .confirmed_block()
                .await
                .unwrap_err()
                .to_string()
                .contains("safe")
        );
        assert_eq!(safe_tracker.current().map(|t| t.confirmed), Some(95));

        let genesis_tracker = tracker_with(
            MockTipSource::scripted(vec![Ok(observation(0, Some(0), Some(0)))]),
            ConfirmationMode::Safe,
        );
        assert_eq!(genesis_tracker.confirmed_block().await.unwrap(), 0);

        // Debug output removes the credentials, path and query of the WebSocket URL.
        let redacted = ChainTipTracker::with_head_source(
            Arc::new(MockTipSource::scripted(vec![])),
            config(ConfirmationMode::Depth),
            HeadSourceConfig {
                chain_id: 100,
                poll_interval: Duration::from_secs(1),
                expected_block_time: Duration::from_secs(5),
                ws_url: Some(Url::parse("wss://user:pass@rpc.example.com:8443/secret?token=xyz").unwrap()),
                ws_url_derived: true,
            },
            None,
        );
        let debug = format!("{:?}", redacted.runtime);
        assert!(debug.contains("wss://rpc.example.com:8443"), "{debug}");
        for secret in ["user", "pass", "secret", "xyz"] {
            assert!(!debug.contains(secret), "{debug}");
        }
    }

    #[tokio::test]
    async fn test_tracker_caching_staleness_concurrency_and_drop() {
        // Without the background task, each confirmed_block call refreshes.
        let tracker = tracker_with(
            MockTipSource::scripted(vec![
                Ok(observation(100, Some(99), Some(80))),
                Ok(observation(110, Some(109), Some(90))),
            ]),
            ConfirmationMode::Depth,
        );
        assert_eq!(tracker.confirmed_block().await.unwrap(), 97);
        assert_eq!(tracker.confirmed_block().await.unwrap(), 107);

        // After ensure_started, a fresh cached tip comes back without an extra observe() call.
        let source = MockTipSource::repeating(observation(100, Some(99), Some(80)));
        let tracker = ChainTipTracker::new(
            Arc::new(source.clone()),
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
        let before = source.calls();
        assert_eq!(tracker.confirmed_block().await.unwrap(), 97);
        assert_eq!(source.calls(), before);

        // A stale cached tip causes a refresh, which returns the source error.
        let mut stale_src = MockTipSource::scripted(vec![Ok(observation(100, Some(99), Some(80)))]);
        stale_src.fallback = Some(Err("rpc failed".to_string()));
        let stale_tracker = ChainTipTracker::new(
            Arc::new(stale_src),
            config(ConfirmationMode::Depth),
            Duration::from_millis(10),
            Duration::from_millis(20),
        );
        stale_tracker.ensure_started();
        for _ in 0..100 {
            if stale_tracker.current().is_some() {
                break;
            }
            sleep(Duration::from_millis(5)).await;
        }
        sleep(Duration::from_millis(100)).await;
        assert!(
            stale_tracker
                .confirmed_block()
                .await
                .unwrap_err()
                .to_string()
                .contains("rpc failed")
        );

        // Concurrent refreshes never move confirmed backwards.
        let mut slow_src =
            MockTipSource::scripted(vec![Ok(observation(100, None, None)), Ok(observation(120, None, None))]);
        slow_src.first_call_delay = Duration::from_millis(50);
        let conc_tracker = tracker_with(slow_src, ConfirmationMode::Depth);
        let (a, b) = tokio::join!(conc_tracker.refresh(), conc_tracker.refresh());
        a.unwrap();
        b.unwrap();
        assert_eq!(conc_tracker.current().unwrap().confirmed, 117);

        // The background task stops when the tracker is dropped.
        let drop_src = MockTipSource::repeating(observation(100, Some(99), Some(80)));
        let drop_tracker = ChainTipTracker::new(
            Arc::new(drop_src.clone()),
            config(ConfirmationMode::Depth),
            Duration::from_millis(10),
            Duration::from_millis(10),
        );
        drop_tracker.ensure_started();
        for _ in 0..100 {
            if drop_src.calls() > 0 {
                break;
            }
            sleep(Duration::from_millis(5)).await;
        }
        drop(drop_tracker);
        sleep(Duration::from_millis(40)).await;
        let after_drop = drop_src.calls();
        sleep(Duration::from_millis(40)).await;
        assert_eq!(drop_src.calls(), after_drop);
    }
}
