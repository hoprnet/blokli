# Blokli Fast Confirmation

- Scope: confirmation modes, chain head source and Fast Confirmation Rule health
- Responsible: DevOps Engineer

This runbook covers the confirmation rule that decides which chain block the indexer treats as confirmed, the head source that feeds it, and
the post-finality checks that detect a false confirmation. The rule is selected in `[indexer.confirmation]` and the head source in
`[indexer.head_tracking]`.

## Confirmation Modes

Blokli supports four modes:

- `depth` (default on all networks): confirmed block is latest minus the configured depth. There is no formal guarantee, only a fixed wait.
- `safe_or_depth`: while FCR is healthy the confirmed block is the higher of the safe block and (latest - depth). While FCR is degraded or
  unavailable it is latest - depth. The lag behind latest never exceeds the depth.
- `safe`: confirmed block is the `safe` block only. No depth fallback. If the endpoint has never reported a safe block, the tracker
  publishes no tip: the indexer does not advance and readiness reports not ready. If the safe tag disappears later, the confirmed block
  stays at its last value and readiness reports not ready. A slow safe block makes the lag grow, up to about one epoch. `safe` is weaker
  than `finalized`: it is the fast-confirmed block.
- `finalized`: confirmed block is the `finalized` block only. Same handling of a missing tag as `safe`: the tracker publishes no tip if the
  endpoint never reported the tag. If the tag disappears later, the confirmed block stays at its last value and readiness reports not ready.
  `finalized` is the strongest rule (no reorganization).

`depth` is the fallback distance used by `depth` and `safe_or_depth`. It defaults to the network value. `max_safe_gap` is the latest-to-safe
gap at or above which FCR counts as degraded.

The indexer mode does not change the block count that `sendTransactionSync` waits for.

## Switch the Mode

1. Edit `[indexer.confirmation]` in the config file.
2. Set `mode` to `depth`, `safe_or_depth`, `safe` or `finalized`.
3. Set `depth` when the default fallback distance does not match the network.
4. Restart the daemon. The mode is read at startup.

Before enabling `safe_or_depth` on a new deployment, run observe-only first:

1. Set `mode = "depth"`.
2. Keep metrics enabled.
3. Watch `blokli_fcr_state`, `blokli_safe_gap_blocks` and `blokli_safe_regressions_total` for at least a few days. In `depth` mode
   `blokli_fcr_fallback_total` never changes.
4. Check that `blokli_safe_regressions_total` stays low and that `blokli_fcr_state` is `2` for most of the time.
5. Switch to `safe_or_depth` and restart.
6. Watch `blokli_indexer_head_lag_seconds` and `blokli_fcr_fallback_total` after the switch.

`safe_or_depth` behaves like `depth` whenever FCR is not healthy, so a stall adds lag but does not weaken the confirmation guarantee below
the depth rule.

## Endpoint Trust

`safe` is whatever the configured RPC endpoint's consensus client reports. A misconfigured or malicious endpoint can report `safe = latest`,
and the tracker accepts it because `safe <= latest` and `finalized <= safe` still hold.

The sanity checks reject impossible orderings only. Post-finality verification re-fetches the logs of a finalized range through the same
configured RPC endpoint that indexing uses. It compares block number, block hash, transaction hash and log index with the stored logs. It
detects a result that changed between indexing and finality. It cannot detect logs that the endpoint returns consistently wrong. It needs an
advancing `finalized` tag: with no finalized block it stays idle even when `verify_finalized` is true. A range that differs in two
comparisons in a row is reported as a false confirmation and halts the indexer. Verification does not repair the stored data.

The endpoint must therefore be an operator-controlled node. Production deployments must use our own nodes with FCR enabled. Enable FCR on
non-validating beacon nodes only, as the consensus clients recommend. Do not rely on a public endpoint for the safe tag.

## Hard Forks

FCR depends on the consensus client's fork choice, so it can stall or misbehave around a hard fork.

1. Before a Gnosis hard fork, set `mode` to `depth` or `finalized`.
2. Restart the daemon.
3. Upgrade the nodes and wait until the fork is stable and `blokli_fcr_state` is `2` again.
4. Set `mode` back to the previous value.
5. Restart the daemon.

Use `finalized` when a temporary lag is acceptable. Use `depth` when indexing must keep up and a shallow reorganization is tolerable.

## False Confirmation

The alert rule `blokli-fcr-false-confirmation` fires when
`increase(blokli_fcr_false_confirmations_total[windowMinutes]) > 0 or blokli_health{status="verification_failed"} == 1`, held for `for`.
Defaults are `windowMinutes` 10 and `for` 1m. The counter branch only sees false confirmations recorded inside the window. The health branch
stays `1` while the index has an unresolved verification failure.

This means that a block range indexed above finality no longer matches the canonical logs once it became final. Either an indexed log was
reorged out or a canonical log was missed.

1. Check `/readyz`. It reports not ready and the indexer stops advancing.
2. Find the verified range and the example mismatches in the logs. The error line names the range and up to ten entries with block hash,
   transaction hash and log index.
3. Compare the same range against an independent node with the same filter set.
4. Check whether the endpoint that served `safe` had FCR enabled and whether it switched clients or nodes.
5. Record the affected range. The index database retains a corroborated failure across restarts. Disabling verification does not clear it.
6. Use the supported recovery: there is no in-place repair of indexed data. Reverting one log does not undo the state derived from it
   (channels, accounts, Safes). Restore the database from a backup taken before the affected range, or start from an empty index database
   against a trusted endpoint. Use only a trusted logs snapshot from before the affected range. See
   [Blokli State Corrupted](blokli-state-corrupted.md). Restart the daemon after the recovery.

Setting `verify_finalized = false` is the last resort. It disables the post-finality comparison, so false confirmations are no longer
detected by that comparison. It does not bypass a recorded halt. Even when `verify_finalized` is true, verification stays idle when the
endpoint reports no finalized block. Use it only with `depth` or `finalized` mode, or together with external verification.

### Which logs are checked

Three kinds of difference are reported: a stored log that the canonical chain does not have, a stored log whose block hash or transaction
hash changed, and a canonical log that is missing from the store. The first two are checked for every contract address (including Safe
contracts). The last one (`missing_from_store`) is checked only for the static contracts configured at startup, because Safe contract
addresses are discovered dynamically while indexing.

## WebSocket Head Source

The tracker uses a `newHeads` subscription over WebSocket by default. Polling is the fallback, not an error, so an endpoint without
WebSocket support needs no change.

- `blokli_head_source` is `1` when the subscription is active and `0` when the tracker polls.
- `blokli_head_ws_reconnects_total` counts successful reconnects. A rising value means an unstable socket.
- `rpc_ws_url` overrides the WebSocket URL. Set it when the provider serves WebSocket on a different path or port from `rpc_url`.
- `head_subscription = false` forces polling and disables the WebSocket code path.

Startup and runtime fallback have different diagnostics. At startup a probe compares the chain id and the block hash of the first header
with the HTTP endpoint. It detects disagreement at the probed block; it does not prove both URLs are the same node. An unsupported
subscription or a chain id / block hash mismatch keeps the tracker on polling for the process lifetime. A connection failure is retried with
backoff. This includes a failure at startup and the case where the HTTP endpoint does not have the block of the first header yet. The
startup warning shows the redacted URL, a category (`unsupported` or `connection`) and the JSON-RPC error code, not the endpoint's error
text.

At runtime a closed or failed socket, or no header within the silence window, logs the warning `switching to polling`. The tracker
reconnects with backoff. A successful reconnect increments `blokli_head_ws_reconnects_total`.

If `blokli_head_source` is `0` and the subscription is wanted:

1. For a startup failure, read the startup warning for the redacted WebSocket URL, the category and the JSON-RPC error code.
2. Check that `rpc_url` or `rpc_ws_url` points at an endpoint that accepts `eth_subscribe`.
3. Check that both URLs serve the same chain. A chain ID or block hash mismatch keeps the tracker on polling.
4. Set `rpc_ws_url` when the provider uses a dedicated WebSocket path or port.
5. Restart the daemon and confirm `blokli_head_source` is `1`.

For a runtime fallback, look for the warning `switching to polling` and watch `blokli_head_ws_reconnects_total`.

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
| `blokli_indexer_head_lag_seconds`      | last near-tip lag sample with a cached block header timestamp              |
| `blokli_health`                        | health status, including `verification_failed` after a failed verification |
