//! Prometheus metrics for the blokli API server.
//!
//! This module provides metric definitions and update functions for monitoring
//! the health and performance of the blokli service.
//!
//! The following metrics are exported (when the `prometheus` feature is enabled):
//!
//! - `hopr_blokli_health`: Gauge (0 or 1) with a `status` label indicating the
//!   current health state (`ok`, `timeout`, `unsynched`, `corrupted`).
//! - `hopr_blokli_errors_total`: Counter of GraphQL errors, labelled by `type`.
//! - `hopr_blokli_request_count`: Counter of GraphQL requests, labelled by `type`
//!   (query, mutation, subscription).
//! - `hopr_blokli_request_duration_seconds`: Histogram of GraphQL request durations
//!   in seconds, labelled by `type`.

/// All possible health status label values for `hopr_blokli_health`.
pub const HEALTH_STATUS_OK: &str = "ok";
pub const HEALTH_STATUS_TIMEOUT: &str = "timeout";
pub const HEALTH_STATUS_UNSYNCHED: &str = "unsynched";
pub const HEALTH_STATUS_CORRUPTED: &str = "corrupted";

/// All possible health status values, used to reset metrics on state transition.
#[cfg(all(feature = "prometheus", not(test)))]
const ALL_HEALTH_STATUSES: &[&str] = &[
    HEALTH_STATUS_OK,
    HEALTH_STATUS_TIMEOUT,
    HEALTH_STATUS_UNSYNCHED,
    HEALTH_STATUS_CORRUPTED,
];

#[cfg(all(feature = "prometheus", not(test)))]
lazy_static::lazy_static! {
    /// Health status gauge. One label value is set to 1.0, others to 0.0.
    /// `status` label: ok | timeout | unsynched | corrupted
    static ref METRIC_BLOKLI_HEALTH: hopr_metrics::MultiGauge =
        hopr_metrics::MultiGauge::new(
            "hopr_blokli_health",
            "Health status of the blokli service (1 = active status, 0 = inactive). \
             Label 'status' indicates current state: ok|timeout|unsynched|corrupted",
            &["status"],
        )
        .unwrap();

    /// Total number of GraphQL errors returned, by error type.
    static ref METRIC_BLOKLI_ERRORS_TOTAL: hopr_metrics::MultiCounter =
        hopr_metrics::MultiCounter::new(
            "hopr_blokli_errors_total",
            "Total number of errors returned by the blokli GraphQL API",
            &["type"],
        )
        .unwrap();

    /// Total number of GraphQL requests received, by request type.
    static ref METRIC_BLOKLI_REQUEST_COUNT: hopr_metrics::MultiCounter =
        hopr_metrics::MultiCounter::new(
            "hopr_blokli_request_count",
            "Total number of GraphQL requests received by the blokli API",
            &["type"],
        )
        .unwrap();

    /// Histogram of GraphQL request durations in seconds, by request type.
    ///
    /// Note: subscription duration is not tracked here because SSE subscriptions are
    /// long-lived connections whose "duration" is not meaningful as a request metric.
    static ref METRIC_BLOKLI_REQUEST_DURATION_SECONDS: hopr_metrics::MultiHistogram =
        hopr_metrics::MultiHistogram::new(
            "hopr_blokli_request_duration_seconds",
            "Duration of GraphQL query and mutation requests handled by the blokli API in seconds",
            vec![0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0],
            &["type"],
        )
        .unwrap();
}

/// Update the `hopr_blokli_health` gauge to reflect a new health status.
///
/// Sets the gauge for `active_status` to 1.0 and all other status labels to 0.0,
/// so that exactly one label value is active at any time.
#[allow(unused_variables)]
pub fn set_health_status(active_status: &str) {
    #[cfg(all(feature = "prometheus", not(test)))]
    {
        for status in ALL_HEALTH_STATUSES {
            let value = if *status == active_status { 1.0 } else { 0.0 };
            METRIC_BLOKLI_HEALTH.set(&[status], value);
        }
    }
}

/// Increment the `hopr_blokli_errors_total` counter for the given error type.
#[allow(unused_variables)]
pub fn increment_errors(error_type: &str) {
    #[cfg(all(feature = "prometheus", not(test)))]
    METRIC_BLOKLI_ERRORS_TOTAL.increment(&[error_type]);
}

/// Increment the `hopr_blokli_request_count` counter for the given request type.
#[allow(unused_variables)]
pub fn increment_request_count(request_type: &str) {
    #[cfg(all(feature = "prometheus", not(test)))]
    METRIC_BLOKLI_REQUEST_COUNT.increment(&[request_type]);
}

/// Record a request duration for the `hopr_blokli_request_duration_seconds` histogram.
///
/// Only call this for queries and mutations. Subscriptions are long-lived SSE connections
/// and their setup time is tracked separately via `hopr_blokli_request_count`.
#[allow(unused_variables)]
pub fn observe_request_duration(request_type: &str, duration_secs: f64) {
    #[cfg(all(feature = "prometheus", not(test)))]
    METRIC_BLOKLI_REQUEST_DURATION_SECONDS.observe(&[request_type], duration_secs);
}

/// Gather all registered Prometheus metrics and encode them in text format.
///
/// Returns `None` when the `prometheus` feature is not enabled.
#[cfg(all(feature = "prometheus", not(test)))]
pub fn gather_metrics() -> Option<String> {
    hopr_metrics::gather_all_metrics().ok()
}

/// Gather all registered Prometheus metrics and encode them in text format.
///
/// Returns `None` when the `prometheus` feature is not enabled.
#[cfg(not(all(feature = "prometheus", not(test))))]
pub fn gather_metrics() -> Option<String> {
    None
}
