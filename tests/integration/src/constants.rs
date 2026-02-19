use std::time::Duration as StdDuration;

use futures_time::time::Duration;
use hopr_primitive_types::prelude::HoprBalance;

const INITIAL_SAFE_BALANCE: &str = "0.5 wxHOPR";
const SUBSCRIPTION_TIMEOUT_SECS: u64 = 60;

pub const EPSILON: f64 = 1e-5;
pub const STACK_STARTUP_WAIT: StdDuration = StdDuration::from_secs(8);

pub fn subscription_timeout() -> Duration {
    Duration::from_secs(SUBSCRIPTION_TIMEOUT_SECS)
}

pub fn parsed_safe_balance() -> HoprBalance {
    INITIAL_SAFE_BALANCE
        .parse()
        .expect("failed to parse initial safe balance")
}
