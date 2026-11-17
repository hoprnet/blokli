# Blokli Fast Confirmation

- Scope: confirmation modes, chain head source and Fast Confirmation Rule health
- Responsible: DevOps Engineer

This runbook covers the confirmation rule that decides which chain block the indexer treats as confirmed, the head source that feeds it, and
the post-finality checks that detect a false confirmation. The rule is selected in `[indexer.confirmation]` and the head source in
`[indexer.head_tracking]`.

## Confirmation Modes

Blokli supports four modes:

- `depth`: confirmed block is latest minus the configured depth. There is no formal guarantee, only a fixed wait.
- `safe_or_depth`: confirmed block is `safe` while the Fast Confirmation Rule (FCR) is healthy, otherwise latest minus the depth. Lag never
  exceeds the depth rule. This is the default on Gnosis networks.
- `safe`: confirmed block is `safe` only. The guarantee is strongest, but lag grows to about one epoch when FCR stalls or when the endpoint
  does not provide a safe block.
- `finalized`: confirmed block is `finalized` only. Use it when the deployment cannot tolerate any reorganization.

`depth` is the fallback distance used by `depth` and `safe_or_depth`. It defaults to the network value. `max_safe_gap` is the latest-to-safe
gap at or above which FCR counts as degraded.

## Switch the Mode

1. Edit `[indexer.confirmation]` in the config file.
2. Set `mode` to `depth`, `safe_or_depth`, `safe` or `finalized`.
3. Set `depth` when the default fallback distance does not match the network.
4. Restart the daemon. The mode is read at startup.

Before enabling `safe_or_depth` on a new deployment, run observe-only first:

1. Set `mode = "depth"`.
2. Keep metrics enabled.
3. Watch `blokli_fcr_state`, `blokli_safe_gap_blocks` and `blokli_fcr_fallback_total` for at least a few days.
4. Check that `blokli_safe_regressions_total` stays low and that `blokli_fcr_state` is `2` for most of the time.
5. Switch to `safe_or_depth` and restart.
6. Watch `blokli_indexer_head_lag_seconds` and `blokli_fcr_fallback_total` after the switch.

`safe_or_depth` behaves like `depth` whenever FCR is not healthy, so a stall adds lag but does not weaken the confirmation guarantee below
the depth rule.

## Endpoint Trust

`safe` is whatever the configured RPC endpoint's consensus client reports. A misconfigured or malicious endpoint can report `safe = latest`,
and the tracker accepts it because `safe <= latest` and `finalized <= safe` still hold.

The sanity checks reject impossible orderings only. Post-finality verification re-reads the indexed ranges once they become final and
compares block hash, transaction hash and log index against the canonical chain. It detects a false confirmation after the fact, reports it,
and halts the indexer. It does not repair the stored data.

Production deployments must use our own nodes with FCR enabled. Enable FCR on non-validating beacon nodes only, as the consensus clients
recommend. Do not rely on a public endpoint for the safe tag.

## Hard Forks

FCR depends on the consensus client's fork choice, so it can stall or misbehave around a hard fork.

1. Before a Gnosis hard fork, set `mode` to `depth` or `finalized`.
2. Restart the daemon.
3. Upgrade the nodes and wait until the fork is stable and `blokli_fcr_state` is `2` again.
4. Set `mode` back to the previous value.
5. Restart the daemon.

Use `finalized` when a temporary lag is acceptable. Use `depth` when indexing must keep up and a shallow reorganization is tolerable.

## False Confirmation

The alert condition is `blokli_fcr_false_confirmations_total > 0` or `blokli_health{status="verification_failed"}` equal to `1`.

This means that a block range indexed above finality no longer matches the canonical logs once it became final. Either an indexed log was
reorged out or a canonical log was missed.

1. Check `/readyz`. It reports not ready and the indexer stops advancing.
2. Find the verified range and the example mismatches in the logs. The error line names the range and up to ten entries with block hash,
   transaction hash and log index.
3. Compare the same range against an independent node with the same filter set.
4. Check whether the endpoint that served `safe` had FCR enabled and whether it switched clients or nodes.
5. Record the affected range before restarting. A restart clears the halt, and the verification cursor starts at the finalized block at that
   moment, so a range below it is not checked again.
6. Repair or re-index the affected range, then restart the daemon.

Setting `verify_finalized = false` is the last resort. It disables the post-finality comparison, so false confirmations are no longer
detected at all and the halt never fires. Use it only with `depth` or `finalized` mode, or together with external verification.

## WebSocket Head Source

The tracker uses a `newHeads` subscription over WebSocket by default. Polling is the fallback, not an error, so an endpoint without
WebSocket support needs no change.

- `blokli_head_source` is `1` when the subscription is active and `0` when the tracker polls.
- `blokli_head_ws_reconnects_total` counts successful reconnects. A rising value means an unstable socket.
- `rpc_ws_url` overrides the WebSocket URL. Set it when the provider serves WebSocket on a different path or port from `rpc_url`.
- `head_subscription = false` forces polling and disables the WebSocket code path.

If `blokli_head_source` is `0` and the subscription is wanted:

1. Find the one startup warning that names the redacted WebSocket URL and the reason.
2. Check that `rpc_url` or `rpc_ws_url` points at an endpoint that accepts `eth_subscribe`.
3. Check that both URLs serve the same chain. A chain ID or block hash mismatch keeps the tracker on polling.
4. Set `rpc_ws_url` when the provider uses a dedicated WebSocket path or port.
5. Restart the daemon and confirm `blokli_head_source` is `1`.

If reconnects keep failing, the tracker stays on polling and keeps indexing. Restore the socket to recover the lower latency.

## Metrics to Watch

| Metric                                 | What it tells you                                                          |
| -------------------------------------- | -------------------------------------------------------------------------- |
| `blokli_chain_tip_block`               | latest, safe, finalized and confirmed block numbers by tag                 |
| `blokli_safe_gap_blocks`               | latest minus safe                                                          |
| `blokli_fcr_state`                     | FCR health: 0 unavailable, 1 degraded, 2 healthy                           |
| `blokli_fcr_fallback_total`            | transitions into the depth fallback of `safe_or_depth`                     |
| `blokli_safe_regressions_total`        | observations where the safe block moved backwards                          |
| `blokli_fcr_false_confirmations_total` | verified ranges whose stored logs differ from the canonical logs           |
| `blokli_head_source`                   | head source: 0 polling, 1 websocket                                        |
| `blokli_head_ws_reconnects_total`      | successful WebSocket reconnects                                            |
| `blokli_indexer_head_lag_seconds`      | time between the confirmed block header timestamp and the end of a round   |
| `blokli_health`                        | health status, including `verification_failed` after a failed verification |
