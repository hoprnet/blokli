# Blokli GraphQL Error Rate High

- Rule name: `blokli-graphql-error-rate-high`
- Severity: Warning
- Responsible: DevOps Engineer

## Background: GraphQL operation types

GraphQL exposes a single HTTP endpoint (`/graphql`); the request body declares what kind of operation it is. blokli classifies every request
into one of three types by checking the first keyword in the query text (`api/src/server.rs:431-437`):

- `query` — read-only. Fetching indexed chain data (channel balances, accounts, etc). This is the default: if the request doesn't start with
  `mutation` or `subscription`, it's a `query`.
- `mutation` — a write. In blokli this is how raw signed transactions are submitted
  (`sendTransaction`/`sendTransactionAsync`/`sendTransactionSync` in `api/src/mutation.rs`) — e.g. a channel-funding transaction goes
  through as a mutation.
- `subscription` — a long-lived, push-based stream over Server-Sent Events. The client opens one connection and keeps receiving new results
  as they happen, instead of polling with repeated queries.

`invalid_request` is **not** a GraphQL operation type — it's blokli's own label for a request that never got far enough to be classified as
one of the three above: the HTTP body couldn't even be parsed into a valid GraphQL request shape (broken JSON, missing the `query` field,
etc.), which is checked _before_ the query/mutation/subscription classification (`api/src/server.rs:385-388`).

This matters for this alert specifically: `blokli_request_count` is only ever incremented for requests that passed that initial parse (so it
only has `query`/`mutation`/`subscription` values, never `invalid_request`). `blokli_errors_total`, on the other hand, _does_ get an
`invalid_request` entry. A plain `errors_total / request_count` ratio would therefore be silently skewed by `invalid_request` errors (they
inflate the numerator with no matching denominator). Subscriptions add a second asymmetry: a subscription is counted once in `request_count`
when the connection opens, but can generate many error increments over that one connection's lifetime as it keeps pushing messages — so a
ratio doesn't mean "errors per request" for subscriptions either.

## What this alert actually checks

Because of the above, this is really three independent conditions unioned together with PromQL's `or`, each tagged with its own `reason`
label so the firing alert tells you which one triggered (`charts/blokli/templates/prometheusrule.yaml`):

- `reason="query_mutation_error_ratio"` — the `query`+`mutation` error/request ratio over 5m exceeds
  `monitoring.prometheusRule.rules.graphqlErrorRateHigh.thresholdRatio` (default 0.05, i.e. 5%). This is the only one of the three that's a
  genuine "ratio", since query/mutation requests are 1-request-in, 1-response-out.
- `reason="invalid_request_count"` — at least `monitoring.prometheusRule.rules.graphqlErrorRateHigh.thresholdInvalidRequestCount`
  (default 5) malformed requests were received in 5m. An absolute count, not a ratio, since there's no meaningful request-count denominator
  for these.
- `reason="subscription_error_count"` — at least `monitoring.prometheusRule.rules.graphqlErrorRateHigh.thresholdSubscriptionErrorCount`
  (default 5) errored messages were pushed over subscription connections in 5m. Also an absolute count, for the reason explained above.

All three share the same `for` window (`monitoring.prometheusRule.rules.graphqlErrorRateHigh.for`, default 10m). If more than one condition
is breached at the same time, you'll see multiple separate firing alert instances (one per `reason`), not one alert that hides the others.

## Impact

Depends on which `reason` fired: query/mutation errors mean API clients are seeing failed reads/writes; invalid requests mean something is
sending blokli malformed traffic; subscription errors mean live/streamed data is unreliable for connected clients.

## Diagnosis

- Check the `reason` label on the firing alert first — it tells you which of the three conditions below to follow.

**`reason="query_mutation_error_ratio"`**

- Break down by type to see whether `query` or `mutation` dominates:
  - `sum by (type) (rate(blokli_errors_total{type=~"query|mutation"}[5m])) / sum by (type) (rate(blokli_request_count{type=~"query|mutation"}[5m]))`
- If `mutation` dominates, check whether the errors originate from on-chain operation failures (e.g. Safe module transaction rejects) rather
  than blokli itself — see [Blokli Transaction Failed](blokli-transaction-failed.md) and
  [Blokli Transaction Validation Failed](blokli-transaction-validation-failed.md).
- If `query` dominates, check whether it correlates with [Blokli Unhealthy](blokli-unhealthy.md) or
  [Blokli Indexer Lag High](blokli-indexer-lag-high.md) — resolvers reading indexer state can surface errors when the underlying data is
  stale or the DB is under pressure.
- Check application logs for the specific GraphQL error codes (see `api/src/errors.rs` for the centralized error code catalog):
  - `kubectl -n <namespace> logs <pod-name> --since=15m | grep -i graphql`
- Check database health and connection pool saturation (`database.maxConnections`) if errors correlate with traffic spikes.

**`reason="invalid_request_count"`**

- Check for a recent client-side deployment sending malformed queries, or a breaking schema change (compare against
  `design/target-api-schema.graphql` / `just export-schema-sqlite` output).
- Check container logs for the raw parse error message (`api/src/server.rs:393`):
  - `kubectl -n <namespace> logs <pod-name> --since=15m | grep -i "invalid graphql request"`
- Identify the source of the traffic (client IP/user-agent, if available at the ingress/load-balancer level) — this is sometimes automated
  scanning/health-check traffic rather than a real client bug.

**`reason="subscription_error_count"`**

- Identify which subscription is affected — subscription names are logged on connection establishment (`api/src/server.rs:487-491`, look for
  `"SSE connection established"`).
- Check whether the errors correlate with [Blokli Unhealthy](blokli-unhealthy.md) or [Blokli Indexer Lag High](blokli-indexer-lag-high.md) —
  long-lived subscriptions surface the same underlying resolver errors as queries do, just repeatedly over the connection's lifetime.
- Check for client-side reconnect storms: a client that repeatedly opens and drops subscriptions can inflate this count without there being
  a single "big" underlying failure.
