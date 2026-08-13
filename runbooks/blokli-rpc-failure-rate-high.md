# Blokli RPC Failure Rate High

- Rule name: `blokli-rpc-failure-rate-high`
- Severity: Warning
- Responsible: DevOps Engineer

This alert fires when the ratio of `blokli_rpc_call_count{result="failure"}` to total `blokli_rpc_call_count` over a 5m rate window exceeds
`monitoring.prometheusRule.rules.rpcFailureRateHigh.thresholdRatio` (default 0.1, i.e. 10%), sustained for over
`monitoring.prometheusRule.rules.rpcFailureRateHigh.for` (default 10m).

`blokli_rpc_call_count` is labeled by `call` (the JSON-RPC method) and `result` (`success`/`failure`), recorded in
`chain/rpc/src/client.rs`. A sustained elevated failure rate points at the upstream RPC provider, not at Blokli itself.

## Impact

Indexing can slow down or stall (see [Blokli Indexer Lag High](blokli-indexer-lag-high.md) and
[Blokli Indexer Stalled](blokli-indexer-stalled.md)), and any on-chain operation requests routed through Blokli's RPC connector may fail.

## Diagnosis

- Break down failures by RPC method to identify a specific hot spot:
  - `sum by (call) (rate(blokli_rpc_call_count{result="failure"}[5m]))`
- Check the RPC provider's own status page/dashboard for incidents or rate-limit changes.
- Check `config.maxRpcRequestsPerSec` — if unlimited (`0`) or set too high, Blokli may be self-inflicting rate-limit failures.
- Check `blokli_retries_per_rpc_call` for elevated retries on the same calls (see [Blokli RPC Retries High](blokli-rpc-retries-high.md))
  this often accompanies a failure-rate spike.
- Check container logs for the specific RPC error messages:
  - `kubectl -n blokli logs deployment/blokli-<networkName> | grep -i rpc`
- If the provider is confirmed degraded, consider failing over to a backup RPC endpoint (`config.rpcUrl`) if one is available for the
  network.
- If failures are isolated to one method (e.g. a specific `eth_getLogs` range), check whether the provider has a response-size or
  block-range limit that Blokli's request pattern is hitting.
