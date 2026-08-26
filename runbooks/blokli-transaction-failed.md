# Blokli Transaction Failed

- Rule name: `blokli-transaction-failed`
- Severity: Warning
- Responsible: DevOps Engineer

Fires when transaction failures (`reverted`, `timeout`, or `submission_failed` — a well-formed transaction that Blokli genuinely attempted
to submit, e.g. a channel-funding tx) spike above their recent baseline, per `monitoring.prometheusRule.rules.transactionFailed.*`.

It's spike-based rather than "any failure fires". Instead it requires the recent rate (`windowMinutes`, default 15m) to be
`spikeMultiplier`x (default 3x) the rate over `baselineWindowHours` (default 6h), with an absolute floor (`minFailures`, default 3) so 1-2
failures against a near-zero baseline don't count as a "spike" on their own.

## Impact

Whatever on-chain operation the failed transactions represented didn't take effect — if it was funding, the target channel/wallet balance
wasn't topped up as intended.

## Diagnosis

- Break down by outcome to see what's driving the spike:
  `sum by (status) (increase(blokli_transaction_status_total{status=~"reverted|timeout|submission_failed"}[15m]))`
- Query the Blokli GraphQL API for the specific failing transaction(s) and their `errorMessage`/`safeExecution.revertReason`.
- `reverted`: fetch the revert reason (`debug_traceTransaction`, see `verify_rpc_capabilities` in `chain/api/src/lib.rs`) — e.g.
  insufficient Safe allowance/balance, a disallowed module call.
- `timeout`: see [RPC Call Latency High](blokli-rpc-call-latency-high.md) / [RPC Failure Rate High](blokli-rpc-failure-rate-high.md).
- `submission_failed`: check container logs for the RPC rejection reason (`grep -i "submission\|rpc error"`) — common causes: stale nonce
  from the calling service, insufficient gas balance, RPC rate-limiting.
