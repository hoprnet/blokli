//! Prometheus metrics for raw transaction execution.
//!
//! The following metric is exported (when the `telemetry` feature is enabled):
//!
//! - `blokli_transaction_status_total`: Counter of terminal outcomes for raw-transaction attempts processed by the raw
//!   transaction executor, labelled by `status`.

/// All terminal transaction outcome label values for `blokli_transaction_status_total`.
///
/// # Examples
///
/// ```
/// use blokli_chain_api::metrics::{STATUS_CONFIRMED, record_transaction_status};
///
/// record_transaction_status(STATUS_CONFIRMED);
/// ```
pub const STATUS_CONFIRMED: &str = "confirmed";
pub const STATUS_REVERTED: &str = "reverted";
pub const STATUS_TIMEOUT: &str = "timeout";
pub const STATUS_VALIDATION_FAILED: &str = "validation_failed";
pub const STATUS_SUBMISSION_FAILED: &str = "submission_failed";

#[cfg(all(feature = "telemetry", not(test)))]
use hopr_types::telemetry as hopr_metrics;

#[cfg(all(feature = "telemetry", not(test)))]
lazy_static::lazy_static! {
    /// Terminal transaction outcome counter, by `status`: confirmed | reverted | timeout |
    /// validation_failed | submission_failed.
    static ref METRIC_BLOKLI_TRANSACTION_STATUS_TOTAL: hopr_metrics::MultiCounter =
        hopr_metrics::MultiCounter::new(
            "blokli_transaction_status_total",
            "Total number of raw-transaction attempts processed by the blokli API, by terminal outcome",
            &["status"],
        )
        .unwrap();
}

/// Increment the `blokli_transaction_status_total` counter for the given terminal outcome.
///
/// # Examples
///
/// ```
/// use blokli_chain_api::metrics::{STATUS_SUBMISSION_FAILED, record_transaction_status};
///
/// record_transaction_status(STATUS_SUBMISSION_FAILED);
/// ```
#[allow(unused_variables)]
pub fn record_transaction_status(status: &str) {
    #[cfg(all(feature = "telemetry", not(test)))]
    METRIC_BLOKLI_TRANSACTION_STATUS_TOTAL.increment(&[status]);
}
