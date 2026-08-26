# Blokli Indexer Stalled

- Rule name: `blokli-indexer-stalled`
- Severity: Critical
- Responsible: DevOps Engineer

This alert fires when `blokli_indexer_block_number` has not increased over the last
`monitoring.prometheusRule.rules.indexerStalled.windowMinutes` (default 5m) while `blokli_chain_head_block_number` has increased in the same
window, sustained for over `monitoring.prometheusRule.rules.indexerStalled.for` (default 2m).

Unlike [Blokli Indexer Lag High](blokli-indexer-lag-high.md), which tracks a growing gap, this alert specifically detects the indexer making
_zero_ progress while the chain keeps moving — i.e. it is stuck, not just behind. Per `design/architecture.md` ("Error Handling Strategy"),
transient issues (RPC retry, DB retry, rate-limit backoff) are expected to self-heal; a full stall for several minutes with the chain head
still advancing indicates a non-recoverable condition (bad config, schema mismatch, corrupted state, deadlock, or panic loop) rather than a
transient blip. The defaults are tuned deliberately tight (5m look-back, 2m sustained — ~7 minutes total) since zero indexer progress while
the chain head keeps moving is already an unambiguous signal, not something that needs a long window to rule out noise.

## Impact

Indexing has completely stopped. The GraphQL API will serve an increasingly stale, frozen view of chain state, and `blokli_health` will
eventually flip to `unsynched` once the lag exceeds `config.api.health.maxIndexerLag`.

## Diagnosis

- Check pod status and restart count — a crash loop would explain a stall:
  - `kubectl -n blokli get pods`
- Check for panics, deadlocks, or fatal errors in recent logs:
  - `kubectl -n blokli logs deployment/blokli-<networkName> -f`
  - `kubectl -n blokli logs deployment/blokli-<networkName> --previous` if it restarted
- Check `blokli_indexer_checksum` for repeated identical values, confirming no new blocks are being processed.
- Check `blokli_indexer_data_source{source=...}` to see whether the indexer is stuck attempting `fast-sync` (e.g.
  `config.indexer.logsSnapshotUrl` unreachable) rather than falling back to `rpc`.
- Check RPC connectivity and error rate — a fully unresponsive RPC provider can stall indexing entirely:
  - Inspect `blokli_rpc_call_count{result="failure"}` and `blokli_retries_per_rpc_call`.
  - Confirm `config.rpcUrl` is reachable from the cluster.
- Check the database — a stuck write transaction or exhausted connection pool (`database.maxConnections`) can block block processing:
  - Check active PostgreSQL connections/locks against the `bloklid` role.
- If logs show a panic or repeated identical error, restart the pod to clear transient state:
  - `kubectl -n blokli rollout restart deployment blokli-<networkName>`
- If the stall persists after restart, escalate to the development team with logs — this may indicate a bug in log/event handling
  (`chain/indexer/`) requiring a code fix.
