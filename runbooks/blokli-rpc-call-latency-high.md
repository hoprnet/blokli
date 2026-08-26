# Blokli RPC Call Latency High

- Rule name: `blokli-rpc-call-latency-high`
- Severity: Informative
- Responsible: DevOps Engineer

This alert fires when the p95 of `blokli_rpc_call_time_sec` (a histogram with buckets at 0.1/0.5/1/2/5/7/10 seconds, recorded in
`chain/rpc/src/client.rs`) exceeds `monitoring.prometheusRule.rules.rpcCallLatencyHigh.thresholdSeconds` (default 5s), sustained for over
`monitoring.prometheusRule.rules.rpcCallLatencyHigh.for` (default 10m).

It's deliberately `info`, not `warning`: elevated latency on its own is a leading/diagnostic signal, not something that should page anyone.
A genuine RPC outage cascades into [RPC Failure Rate High](blokli-rpc-failure-rate-high.md) and eventually
[Blokli Unhealthy](blokli-unhealthy.md), which are more specific and already carry `warning`/`critical` severity — this alert exists to give
those a head start / extra context, not to be the primary signal.

`blokli_rpc_call_time_sec` is only populated when the RPC client is built with `.layer(MetricsLayer)` (`chain/rpc/src/client.rs`). This is
wired into both production RPC clients — the combined indexer+API path (`chain/api/src/lib.rs`, used by `bloklid`, the only binary this Helm
chart deploys) and the standalone `blokli-api`-only mode (`api/src/lib.rs`) — so the metric is live in both. It is not wired into the
one-off CLI tools (`blokli-contract-deployer`, `blokli-api export-schema`), but those are never scraped by Prometheus anyway.

## Impact

On its own: none directly — it's a diagnostic breadcrumb. Sustained high latency can eventually slow indexing throughput and push
`blokli_indexer_block_number` further behind `blokli_chain_head_block_number`, which is what
[Blokli Indexer Lag High](blokli-indexer-lag-high.md) is for.

## Diagnosis

- Break down latency by RPC method to find the slow call:
  - `histogram_quantile(0.95, sum by (le, call) (rate(blokli_rpc_call_time_sec_bucket[5m])))`
- Check whether the slowdown correlates with a specific indexing phase (`blokli_indexer_sync_progress{phase=...}`) — historical backfill
  phases naturally issue larger/heavier RPC calls than continuous tailing.
- Check the Tenderly RPC provider's own latency dashboards/status page for a regional or global slowdown.
- Check `config.maxRpcRequestsPerSec` — if set very low, calls may be queuing client-side rather than the provider itself being slow;
  compare `blokli_rpc_call_time_sec` against provider-side metrics if available.
- If a specific method is consistently slow (e.g. large `eth_getLogs` ranges), consider whether batch sizes
  (`config.indexer.subscription.batchSize`) can be tuned down to reduce per-call latency.
- If latency is confirmed as an upstream provider issue and persists, consider switching `config.rpcUrl` to an alternate provider for the
  network.
