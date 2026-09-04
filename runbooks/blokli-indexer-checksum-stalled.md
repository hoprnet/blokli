# Blokli Indexer Checksum Stalled

- Rule name: `blokli-indexer-checksum-stalled`
- Severity: Warning
- Responsible: DevOps Engineer

This alert fires when `blokli_indexer_checksum` has not changed over the last
`monitoring.prometheusRule.rules.checksumStalled.windowMinutes` (default 10m) while `blokli_indexer_block_number` has advanced in the same
window, sustained for over `monitoring.prometheusRule.rules.checksumStalled.for` (default 5m).

Unlike [Blokli Indexer Stalled](blokli-indexer-stalled.md), which fires on a full stall (block number frozen while the chain head keeps
advancing), this alert covers the case where the indexer keeps moving through blocks but its running checksum stops mutating — e.g. logs or
events being silently dropped or miscounted during processing. `blokli-indexer-lag-high` and `blokli-indexer-stalled` can both stay green
while this condition is present, since block number progress alone looks healthy.

## Impact

The indexer's on-disk/served state may be silently diverging from the chain (missed events, incomplete channel/ticket updates) even though
indexing otherwise looks like it's keeping up. Data served by the GraphQL API can be incomplete or stale in ways that lag/stall metrics
won't surface.

## Diagnosis

- Confirm the condition on the "Indexer checksum activity" dashboard panel — a sustained 0 while `blokli_indexer_block_number` keeps
  climbing is exactly this scenario.
- Check recent logs for errors during log/event decoding or handling that might be swallowed rather than surfaced:
  - `kubectl -n blokli logs deployment/blokli-<networkName> -f`
- Check whether the blocks being processed in the affected window are legitimately empty of relevant events (e.g. low on-chain activity for
  the contracts Blokli tracks) — if so this may be a false positive and the alert thresholds may need tuning (`windowMinutes` too short) or
  the checksum computation may need to also churn on activity-less blocks.
- Check `blokli_rpc_call_count{result="failure"}` for partial RPC failures (e.g. `eth_getLogs` returning truncated results) that would let
  block processing continue while missing events.
- If the checksum computation itself looks buggy (e.g. not incorporating a code path that should mutate it), escalate to the development
  team with logs and the affected block range — this points at a code fix in `chain/indexer/`.
