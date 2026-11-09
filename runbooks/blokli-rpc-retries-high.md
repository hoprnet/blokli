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

**How the metric is recorded**: `InstrumentedRetryBackoffLayer` (`chain/rpc/src/client.rs`, wired in `chain/api/src/lib.rs`) observes
`blokli_retries_per_rpc_call` once per completed JSON-RPC request, labelled by method, with the number of retries that request took. For a
batch, every method in the batch is observed with the retry count of the batch as a whole. The layer allows at most 2 retries per request
(`chain/api/src/lib.rs`), so the p95 cannot exceed the default `thresholdRetries` of 3: lower the threshold below 2 for the alert to be able
to fire.

**Known gap — fixed backoff**: `TODO(#7140)` (`chain/api/src/lib.rs`, `chain/rpc/src/client.rs`) is still open: alloy's
`RetryPolicy::backoff_hint` doesn't receive the current retry count, so retries use a fixed delay instead of the configured exponential
backoff. This does not affect the metric, which counts retries in the layer itself.

## Impact

No immediate user-facing impact if retries are still succeeding, but indexing throughput is degraded and the provider is close to tipping
into outright failures.

## Diagnosis

- Break down retries by RPC method:
  - `histogram_quantile(0.95, sum by (le, call) (rate(blokli_retries_per_rpc_call_bucket[5m])))`
- Check `config.maxRpcRequestsPerSec` — retries often correlate with the provider rate-limiting Blokli; lowering this value can reduce retry
  pressure.
- Check [Tenderly's status page](https://tenderly.co/status) for known rate-limit or capacity issues.
- Cross-check with [Blokli RPC Failure Rate High](blokli-rpc-failure-rate-high.md) and
  [Blokli RPC Call Latency High](blokli-rpc-call-latency-high.md) — retries frequently precede or accompany both.
- If retries are consistently high for a specific method, check whether request parameters (e.g. block range size) can be reduced to fit
  within the provider's limits.
