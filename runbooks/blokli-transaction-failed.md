# Blokli Transaction Failed

- Rule names: `blokli-transaction-failed-short`, `blokli-transaction-failed-long`
- Severity: Warning
- Responsible: DevOps Engineer

Two alerts fire when transaction failures (`reverted`, `timeout`, or `submission_failed` — a well-formed transaction that Blokli genuinely
attempted to submit, e.g. a channel-funding tx) make up too large a share of on-chain submission attempts. The denominator for both is all
attempts except `validation_failed`, since those never reach the chain (see
[Transaction Validation Failed](blokli-transaction-validation-failed.md)). Neither fires on "any failure" — some timeouts/reverts are
expected background noise.

- `blokli-transaction-failed-short` (`monitoring.prometheusRule.rules.transactionFailedShort.*`): more than `ratio` (default 50%) of
  attempts failed over `windowMinutes` (default 15m), with at least `minFailures` (default 3) failures. Catches a sharp burst.
- `blokli-transaction-failed-long` (`monitoring.prometheusRule.rules.transactionFailedLong.*`): more than `ratio` (default 5%) of attempts
  failed over `windowHours` (default 6h), with at least `minFailures` (default 5) failures. Catches a lower failure rate that's nonetheless
  persistent/regular enough that the short alert would never trip on it (e.g. "1 in 20 timeouts sustained for hours" — rare-looking in any
  15m slice, but not actually rare).

Tune each alert's window and thresholds independently based on what "acceptable but rare" vs. "unacceptable, even at low volume" means for
your traffic — the defaults above are starting points, not derived from observed baseline traffic.

## Impact

Whatever on-chain operation the failed transactions represented didn't take effect — if it was funding, the target channel/wallet balance
wasn't topped up as intended.

## Diagnosis

- Check which alert fired to know whether this is a burst (`-short`) or a sustained low-grade failure rate (`-long`).
- Break down by outcome to see what's driving it:
  `sum by (status) (increase(blokli_transaction_status_total{status=~"reverted|timeout|submission_failed"}[15m]))`
- Query the Blokli GraphQL API for the specific failing transaction(s) and their `errorMessage`/`safeExecution.revertReason`.
- `reverted`: fetch the revert reason (`debug_traceTransaction`, see `verify_rpc_capabilities` in `chain/api/src/lib.rs`) — e.g.
  insufficient Safe allowance/balance, a disallowed module call.
- `timeout`: see [RPC Call Latency High](blokli-rpc-call-latency-high.md) / [RPC Failure Rate High](blokli-rpc-failure-rate-high.md).
- `submission_failed`: check container logs for the RPC rejection reason (`grep -i "submission\|rpc error"`) — common causes: stale nonce
  from the calling service, insufficient gas balance, RPC rate-limiting.
