# Blokli Runbooks

This directory contains runbooks for alerts fired by the Prometheus alerting rules defined in `charts/blokli/templates/prometheusrule.yaml`.
Each runbook describes the alert, its impact, and the steps to diagnose and resolve it. Alert thresholds are parametrized in
`charts/blokli/values.yaml` under `monitoring.prometheusRule.rules.*`.

## Indexer Health

Alerts related to on-chain indexing progress and correctness.

| Alert                                                          | Severity | Responsible     | Description                                                                              |
| -------------------------------------------------------------- | -------- | --------------- | ---------------------------------------------------------------------------------------- |
| [Indexer Lag High](blokli-indexer-lag-high.md)                 | Warning  | DevOps Engineer | The indexer is falling behind the chain head by more than the configured block threshold |
| [Indexer Stalled](blokli-indexer-stalled.md)                   | Critical | DevOps Engineer | The indexer has made zero progress while the chain head keeps advancing                  |
| [Indexer Checksum Stalled](blokli-indexer-checksum-stalled.md) | Warning  | DevOps Engineer | The indexer's running checksum isn't mutating despite block-number progress              |

## Readiness

Alerts derived from the same readiness logic that gates the `/readyz` endpoint.

| Alert                                               | Severity | Responsible     | Description                                          |
| --------------------------------------------------- | -------- | --------------- | ---------------------------------------------------- |
| [Blokli Unhealthy](blokli-unhealthy.md)             | Critical | DevOps Engineer | The readiness check is not reporting the "ok" status |
| [Blokli State Corrupted](blokli-state-corrupted.md) | Critical | DevOps Engineer | The readiness check reported a database query error  |

## Transaction Execution

Alerts related to raw transactions submitted through the Blokli API (e.g. channel funding).

| Alert                                                                    | Severity | Responsible     | Description                                                                                           |
| ------------------------------------------------------------------------ | -------- | --------------- | ----------------------------------------------------------------------------------------------------- |
| [Transaction Failed (short)](blokli-transaction-failed.md)               | Warning  | DevOps Engineer | A burst of transaction failures (reverted/timeout/submission_failed) over a short window              |
| [Transaction Failed (long)](blokli-transaction-failed.md)                | Warning  | DevOps Engineer | A low but steady rate of transaction failures (reverted/timeout/submission_failed) over a long window |
| [Transaction Validation Failed](blokli-transaction-validation-failed.md) | Warning  | DevOps Engineer | A submitted transaction was rejected before broadcast (e.g. disallowed target contract/function)      |

## RPC / Upstream Provider

Alerts related to the health of the blockchain RPC provider Blokli depends on.

| Alert                                                    | Severity | Responsible     | Description                                                              |
| -------------------------------------------------------- | -------- | --------------- | ------------------------------------------------------------------------ |
| [RPC Failure Rate High](blokli-rpc-failure-rate-high.md) | Warning  | DevOps Engineer | The share of failed RPC calls is above the configured threshold          |
| [RPC Call Latency High](blokli-rpc-call-latency-high.md) | Info     | DevOps Engineer | The p95 RPC call latency is above the configured threshold               |
| [RPC Retries High](blokli-rpc-retries-high.md)           | Warning  | DevOps Engineer | The p95 number of retries per RPC call is above the configured threshold |

## GraphQL API

Alerts related to the GraphQL API's error rate and latency.

| Alert                                                        | Severity | Responsible     | Description                                                                           |
| ------------------------------------------------------------ | -------- | --------------- | ------------------------------------------------------------------------------------- |
| [GraphQL Error Rate High](blokli-graphql-error-rate-high.md) | Warning  | DevOps Engineer | The share of GraphQL requests resulting in an error is above the configured threshold |
| [GraphQL Latency High](blokli-graphql-latency-high.md)       | Warning  | DevOps Engineer | The p95 GraphQL request latency is above the configured threshold                     |
