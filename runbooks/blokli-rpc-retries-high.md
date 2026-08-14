# Blokli RPC Retries High

- Rule name: `blokli-rpc-retries-high`
- Severity: Warning
- Responsible: DevOps Engineer

This alert fires when the p95 of `blokli_retries_per_rpc_call` (a histogram with integer buckets 0-10, recorded in
`chain/rpc/src/client.rs`) exceeds `monitoring.prometheusRule.rules.rpcRetriesHigh.thresholdRetries` (default 3 retries), sustained for over
`monitoring.prometheusRule.rules.rpcRetriesHigh.for` (default 10m).

This is an earlier warning signal than [Blokli RPC Failure Rate High](blokli-rpc-failure-rate-high.md): calls that eventually succeed after
several retries do not count as `result="failure"`, but a rising retry count still indicates the RPC provider is flaky or being
rate-limited, and is a leading indicator of an incoming failure-rate or latency spike.

**Known gap — currently never fires**: unlike `blokli_rpc_call_count`/`blokli_rpc_call_time_sec` (see
[Blokli RPC Call Latency High](blokli-rpc-call-latency-high.md)), `blokli_retries_per_rpc_call` has no code path that ever calls
`.observe()` on it anywhere in the codebase, in production or in tests — it isn't just unwired, it's unimplemented. The blocker is
`TODO(#7140)` (`chain/api/src/lib.rs`, `chain/rpc/src/client.rs`): alloy's `RetryPolicy::backoff_hint` doesn't receive the current retry
count, so there is no hook available yet to know how many retries a given call took. Until #7140 lands, treat this alert as a placeholder —
it will not fire regardless of the threshold. [Blokli RPC Failure Rate High](blokli-rpc-failure-rate-high.md) is the closest live signal in
the meantime.

## Impact

No immediate user-facing impact if retries are still succeeding, but indexing throughput is degraded and the provider is close to tipping
into outright failures.

## Diagnosis

- Break down retries by RPC method:
  - `histogram_quantile(0.95, sum by (le, call) (rate(blokli_retries_per_rpc_call_bucket[5m])))`
- Check `config.maxRpcRequestsPerSec` — retries often correlate with the provider rate-limiting Blokli; lowering this value can reduce retry
  pressure.
- Check the RPC provider's status page for known rate-limit or capacity issues.
- Cross-check with [Blokli RPC Failure Rate High](blokli-rpc-failure-rate-high.md) and
  [Blokli RPC Call Latency High](blokli-rpc-call-latency-high.md) — retries frequently precede or accompany both.
- If retries are consistently high for a specific method, check whether request parameters (e.g. block range size) can be reduced to fit
  within the provider's limits.
