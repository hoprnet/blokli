# Blokli Indexer Lag High

- Rule name: `blokli-indexer-lag-high`
- Severity: Warning
- Responsible: DevOps Engineer

This alert fires when `blokli_chain_head_block_number - blokli_indexer_block_number` exceeds
`monitoring.prometheusRule.rules.indexerLagHigh.thresholdBlocks` (default 15 blocks) for over
`monitoring.prometheusRule.rules.indexerLagHigh.for` (default 5m).

This is an early-warning signal, distinct from [Blokli Unhealthy](blokli-unhealthy.md). Readiness only fails once the lag exceeds
`config.api.health.maxIndexerLag` (default 10 blocks) plus the configured chain finality (see `design/architecture.md`, "Finality Handling
in Readiness Checks"). This alert's default threshold (15) is intentionally set close to, but still above, that readiness threshold so it
flags a growing lag shortly before it becomes a readiness failure, without being so tight it fires on normal jitter.

## Impact

The GraphQL API continues to serve data, but it reflects a chain state that is increasingly stale. Consumers relying on near-real-time data
(e.g. channel balances, account state) may act on outdated information.

## Diagnosis

- Check the current lag and trend via the `blokli_chain_head_block_number` and `blokli_indexer_block_number` metrics/Grafana panels.
- Check `blokli_indexer_sync_progress{phase=...}` to see which indexing phase is active (`HistoricalDiscovery`, `HistoricalSafeBackfill`,
  `fast_sync`, `Continuous`) — a lag during a historical backfill phase is expected and self-resolving; a lag during `Continuous` phase is
  not.
- Check `blokli_rpc_call_time_sec` and `blokli_rpc_call_count{result="failure"}` for signs the RPC provider is slow or failing (see
  [RPC Call Latency High](blokli-rpc-call-latency-high.md) and [RPC Failure Rate High](blokli-rpc-failure-rate-high.md)).
- Check pod CPU/memory pressure and database latency — a saturated PostgreSQL instance can slow down block processing.
- Check container logs for repeated RPC retries or DB write errors:
  - `kubectl -n blokli logs deployment/blokli-<networkName> -f | grep -i -E "retry|error|lag"`
- If the lag keeps growing and the indexer is not stalled outright, consider whether `config.maxRpcRequestsPerSec` is throttling too
  aggressively, or whether the RPC provider needs to be swapped/scaled.
- If the lag is not recovering, escalate — see [Blokli Indexer Stalled](blokli-indexer-stalled.md) for the case where the indexer has
  stopped making progress entirely.
