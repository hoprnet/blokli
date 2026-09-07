# Blokli Transaction Validation Failed

- Rule name: `blokli-transaction-validation-failed`
- Severity: Warning
- Responsible: DevOps Engineer

This alert fires when more than `monitoring.prometheusRule.rules.transactionValidationFailed.minFailures` (default 5) transactions submitted
through the Blokli raw transaction API failed local validation (`validation_failed`) within the last
`monitoring.prometheusRule.rules.transactionValidationFailed.windowMinutes` (default 15m), sustained for over
`monitoring.prometheusRule.rules.transactionValidationFailed.for` (default 5m).

Validation happens in `chain/api/src/transaction_validator.rs` before the transaction is ever submitted to the RPC provider — e.g. the
transaction targets a contract or function not on the allowlist. Because this rejection happens client-side and before broadcast, it almost
always indicates a caller/integration issue (wrong contract address, outdated allowlist, a bug in the transaction-building code) rather than
an on-chain or RPC problem.

`minFailures` is a plain floor, not a failure-ratio threshold like [Blokli Transaction Failed](blokli-transaction-failed.md) uses:
`validation_failed` is deterministic (a request either matches the allowlist or it doesn't), not RPC-driven, so there's no legitimate
nonzero "normal" rate to compare against. The floor exists purely to tolerate a handful of one-off bad requests — a stale client, a
temporary bug, or a few scanner/malicious probes — without paging on the very first one, while still catching a sustained pattern.

## Impact

The intended on-chain operation was never attempted. Lower severity than [Blokli Transaction Failed](blokli-transaction-failed.md) because
no gas was spent and no partial state change occurred, but the caller's operation still did not happen.

## Diagnosis

- Query the Blokli GraphQL API or container logs for the specific validation error (`ContractNotAllowed`/`FunctionNotAllowed`, see
  `api/src/mutation.rs`) and the `to` address/selector involved:
  - `kubectl -n blokli logs deployment/blokli-<networkName> | grep -i "validation"`
- Confirm whether the target contract/function is expected to be allowed for this deployment, and whether the allowlist configuration is
  stale relative to a recent contract redeploy or upgrade.
- Identify the calling service from the request pattern/timing and check whether it recently changed the transactions it submits (e.g. a new
  funding flow, a new target contract).
- If the target should be allowed, update the transaction validator's allowlist configuration and redeploy.
- If the target should not be allowed, the fix is on the caller's side — coordinate with the owning team to correct the transaction being
  built.
