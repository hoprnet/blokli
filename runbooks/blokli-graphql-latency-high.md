# Blokli GraphQL Latency High

- Rule name: `blokli-graphql-latency-high`
- Severity: Warning
- Responsible: DevOps Engineer

This alert fires when the p95 of `blokli_request_duration_seconds` (a histogram labeled by `type`, recorded in `api/src/metrics.rs`) exceeds
`monitoring.prometheusRule.rules.graphqlLatencyHigh.thresholdSeconds` (default 2s), sustained for over
`monitoring.prometheusRule.rules.graphqlLatencyHigh.for` (default 10m).

The `type` label here only ever takes the values `query` or `mutation` — a GraphQL `subscription` is a long-lived connection, not a single
request/response, so its "duration" isn't meaningful and blokli intentionally never records it into this histogram
(`api/src/metrics.rs:66-73`). Malformed requests (`invalid_request`) are rejected before request timing even starts, so they're absent here
too. See [Blokli GraphQL Error Rate High](blokli-graphql-error-rate-high.md) for background on what `query`, `mutation`, `subscription`, and
`invalid_request` mean and how each is tracked.

## Impact

Clients querying the GraphQL API experience slow responses. Depending on client-side timeouts, this can also manifest as request failures
downstream even though Blokli itself is still serving requests.

## Diagnosis

- Break down latency by request type:
  - `histogram_quantile(0.95, sum by (le, type) (rate(blokli_request_duration_seconds_bucket[5m])))`
- Check whether latency correlates with database load — GraphQL resolvers query the configured database (`database.type`: PostgreSQL or
  SQLite) directly; check for missing indexes, N+1 query patterns, or connection pool exhaustion (`database.maxConnections`). For
  PostgreSQL, inspect `pg_stat_activity` for long-running queries or lock contention. SQLite deployments have no connection pool and are
  more sensitive to write contention under concurrent load.
- Check pod CPU/memory usage for resource starvation:
  - `kubectl -n blokli top pod <pod-name>`
- Check whether specific queries are unusually expensive (e.g. large pagination windows, deeply nested selections) — consider whether
  DataLoader batching is being bypassed for the slow resolver.
- Check whether the slowdown correlates with indexer activity (e.g. a historical backfill phase competing for DB/CPU resources with API
  request handling).
- If latency is broadly elevated across all types, consider if `resources` need to be scaled up for the current traffic level.
