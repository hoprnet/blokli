# Blokli Unhealthy

- Rule name: `blokli-unhealthy`
- Severity: Critical
- Responsible: DevOps Engineer

This alert fires when `blokli_health{status="ok"}` reports `0` for over `monitoring.prometheusRule.rules.unhealthy.for` (default 5m). The
`blokli_health` gauge mirrors the same readiness logic that gates the `/readyz` HTTP endpoint (`api/src/readiness.rs`), with one of four
mutually exclusive statuses: `ok`, `timeout`, `unsynched`, or `corrupted`.

- `timeout` — the most recent RPC call used for the readiness check failed.
- `unsynched` — no `chain_info` row exists yet, or the indexer lag exceeds `config.api.health.maxIndexerLag` (adjusted for chain finality,
  see `design/architecture.md`, "Finality Handling in Readiness Checks").
- `corrupted` — a database query error occurred during the health check (see [Blokli State Corrupted](blokli-state-corrupted.md), which
  fires specifically for this case).

## Impact

`/readyz` reports not-ready, so any Kubernetes readiness probe or load balancer will stop routing traffic to this pod. If all replicas are
unhealthy, the GraphQL API becomes fully unavailable to clients.

## Diagnosis

- Check which status is active by querying `blokli_health` directly (all four series are exported; the one at value `1` is current).
- If `unsynched`:
  - Check the indexer lag — see [Blokli Indexer Lag High](blokli-indexer-lag-high.md) and
    [Blokli Indexer Stalled](blokli-indexer-stalled.md).
  - Confirm `config.api.health.maxIndexerLag` is set to a reasonable value for the deployment's chain finality.
- If `timeout`:
  - Check RPC provider reachability and latency (`config.rpcUrl`).
  - Check `blokli_rpc_call_count{result="failure"}` and `blokli_rpc_call_time_sec` for the health-check RPC call specifically.
  - Confirm `config.api.health.timeout` (default 5s) isn't too tight for the current RPC provider's latency.
- If `corrupted`, follow [Blokli State Corrupted](blokli-state-corrupted.md) instead.
- Check pod readiness probe status:
  - `kubectl -n blokli describe pod <pod-name>` (look at the `Readiness` probe events)
- Check application logs around the time of the transition:
  - `kubectl -n blokli logs deployment/blokli-<networkName>`
