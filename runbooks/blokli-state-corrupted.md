# Blokli State Corrupted

- Rule name: `blokli-state-corrupted`
- Severity: Critical
- Responsible: DevOps Engineer

This alert fires when `blokli_health{status="corrupted"}` reports `1` for over `monitoring.prometheusRule.rules.stateCorrupted.for` (default
1m). Per `api/src/readiness.rs`, this status is set specifically when a database query performed as part of the readiness check returns an
error — as opposed to `unsynched` (lag) or `timeout` (RPC), which are usually transient and recoverable on their own.

The short default `for` window (1m, vs. 5m for the general [Blokli Unhealthy](blokli-unhealthy.md) alert) reflects that a DB query error is
a stronger, more specific signal than a generic health drop and should be investigated immediately.

## Impact

The database backing this Blokli instance may be unreachable, have a schema mismatch, or contain corrupted data. Both indexing and the
GraphQL API are at risk of serving incorrect data or failing outright.

## Diagnosis

- Check database connectivity from the pod.
- Check recent application logs for the specific query error:
  - `kubectl -n blokli logs deployment/blokli-<networkName> | grep -i -E "error|corrupt|sql|database"`
- Confirm the database schema matches what this `bloklid` version expects — check for pending or failed migrations (`db/migration/`).
- Check PostgreSQL server health directly: connection limits (`database.maxConnections`).
- If a recent deployment changed the `bloklid` image version, check whether a migration was expected but not applied, or whether the new
  version is incompatible with the current schema.
- If corruption is confirmed at the storage layer, this may require restoring from a backup or, for fast-sync-capable deployments,
  re-indexing from a logs snapshot (`config.indexer.enableLogsSnapshot`).
- Escalate to the development team with logs and the exact error if the cause is not immediately clear from the query error message.
