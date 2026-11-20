# Blokli Fast Confirmation

- Scope: confirmation modes, chain head source and Fast Confirmation Rule health
- Responsible: DevOps Engineer

This runbook covers three parts of the indexer:

- The confirmation rule. It decides which chain block the indexer treats as confirmed.
- The head source. It supplies the chain head to the chain tip tracker.
- Post-finality verification. It detects a false confirmation.

The chain tip tracker (the tracker) is a background task. It reads the chain head and derives the confirmed tip, the highest confirmed
block. A false confirmation is an indexed block range whose logs changed after the range became final. You select the rule in
`[indexer.confirmation]` and the head source in `[indexer.head_tracking]`.

The Fast Confirmation Rule (FCR) is a feature of the consensus client. It reports the fast-confirmed block as the `safe` block. A
reorganization is a chain event that replaces recent blocks. Readiness is the state that the `/readyz` endpoint reports.

## Confirmation Modes

Blokli supports four modes. The default mode is `depth` on all networks.

- `depth`: The confirmed block is the latest block minus the configured depth. This mode gives no formal guarantee. It only waits a fixed
  number of blocks.
- `safe_or_depth`: If FCR is healthy, the confirmed block is the higher of the safe block and (latest - depth). If FCR is degraded or
  unavailable, the confirmed block is latest - depth. The lag behind the latest block never exceeds the depth.
- `safe`: The confirmed block is the `safe` block only. This mode has no depth fallback. If the endpoint never reported a safe block, the
  tracker publishes no tip. Then the indexer does not advance and readiness reports not ready. If the safe tag disappears later, the
  confirmed block stays at its last value and readiness reports not ready. If the safe block is slow, the lag grows, up to about one epoch.
  `safe` is weaker than `finalized`. The `safe` block is the fast-confirmed block.
- `finalized`: The confirmed block is the `finalized` block only. A missing tag has the same effect as in `safe` mode. If the endpoint never
  reported the tag, the tracker publishes no tip. If the tag disappears later, the confirmed block stays at its last value and readiness
  reports not ready. `finalized` is the strongest rule. A reorganization cannot replace a finalized block.

The `depth` key sets the fallback distance for the `depth` and `safe_or_depth` modes. Its default comes from the network. The `max_safe_gap`
key sets a limit for the gap from the latest block to the safe block. If the gap is at or above this limit, FCR counts as degraded.

The indexer mode does not change the number of blocks that `sendTransactionSync` waits for.

## Switch the Mode

1. Edit `[indexer.confirmation]` in the configuration file.
2. Set `mode` to `depth`, `safe_or_depth`, `safe` or `finalized`.
3. If the default fallback distance does not match the network, set `depth`.
4. Restart the daemon. The daemon reads the mode at startup.

Before you enable `safe_or_depth` on a new deployment, observe FCR in `depth` mode first:

1. Set `mode = "depth"`.
2. Keep metrics enabled.
3. Watch `blokli_fcr_state`, `blokli_safe_gap_blocks` and `blokli_safe_regressions_total` for at least a few days. In `depth` mode,
   `blokli_fcr_fallback_total` never changes.
4. Make sure that `blokli_safe_regressions_total` stays low.
5. Make sure that `blokli_fcr_state` is `2` for most of the time.
6. Set `mode = "safe_or_depth"`.
7. Restart the daemon.
8. After the restart, watch `blokli_indexer_head_lag_seconds` and `blokli_fcr_fallback_total`.

If FCR is not healthy, `safe_or_depth` operates like `depth`. Thus a stall adds lag, but the confirmation guarantee does not become weaker
than the depth rule.

## Endpoint Trust

The `safe` block is the value that the consensus client of the configured RPC endpoint reports. An endpoint with a bad configuration, or a
malicious endpoint, can report `safe = latest`. The tracker accepts this value because `safe <= latest` and `finalized <= safe` are still
true.

The sanity checks reject only impossible orderings. Post-finality verification fetches the logs of a finalized range again. It uses the same
configured RPC endpoint as indexing. It compares block number, block hash, transaction hash and log index with the stored logs. It detects a
result that changed between indexing and finality. It cannot detect wrong logs that the endpoint returns the same way each time.

Post-finality verification needs a `finalized` tag that advances. If the endpoint reports no finalized block, verification stays idle, even
when `verify_finalized` is true. If a range differs in two comparisons in a row, verification reports a false confirmation and halts the
indexer. Verification does not repair the stored data.

For these reasons, the endpoint must be a node that the operator controls. Production deployments must use our own nodes with FCR enabled.
Enable FCR only on beacon nodes that do not validate. The consensus clients recommend this. Do not trust a public endpoint for the safe tag.

## Hard Forks

FCR uses the fork choice of the consensus client. Thus FCR can stall or operate incorrectly near a hard fork.

Select a temporary mode before the fork. If a temporary lag is acceptable, use `finalized`. If indexing must keep up and a shallow
reorganization is acceptable, use `depth`.

1. Before a Gnosis hard fork, set `mode` to the temporary mode.
2. Restart the daemon.
3. Upgrade the nodes.
4. Wait until the fork is stable and `blokli_fcr_state` is `2` again.
5. Set `mode` back to the previous value.
6. Restart the daemon.

## False Confirmation

The alert rule `blokli-fcr-false-confirmation` fires when
`increase(blokli_fcr_false_confirmations_total[windowMinutes]) > 0 or blokli_health{status="verification_failed"} == 1` stays true for the
`for` duration. The defaults are 10 for `windowMinutes` and 1m for `for`. The counter part of the condition sees only false confirmations
that the daemon recorded inside the window. The health part stays `1` while the index has a verification failure that is not resolved.

The alert means that a block range no longer matches the canonical logs after it became final. The indexer indexed this range before it was
final. A reorganization removed an indexed log, or the indexer missed a canonical log.

Find the cause:

1. Query `/readyz`. It reports not ready, and the indexer stops advancing.
2. Find the verified range and the example mismatches in the logs. The error line names the range. It also gives up to ten entries with
   block hash, transaction hash and log index.
3. Compare the same range with an independent node that uses the same filter set.
4. Find out if the endpoint that served `safe` had FCR enabled.
5. Find out if that endpoint switched clients or nodes.
6. Record the affected range. The index database keeps a failure that two comparisons found across restarts. If you disable verification,
   the failure stays.

Do not try to repair indexed data in place. The supported recovery has no in-place repair. If you revert one log, the state derived from it
(channels, accounts, Safes) stays.

Recover the index:

1. Restore the database from a backup taken before the affected range. As an alternative, start from an empty index database against a
   trusted endpoint. For the procedure, see [Blokli State Corrupted](blokli-state-corrupted.md).
2. If you use a logs snapshot, use only a trusted snapshot from before the affected range.
3. Restart the daemon after the recovery.

Set `verify_finalized = false` only as the last resort. This setting disables the post-finality comparison. Then that comparison no longer
detects false confirmations. The setting does not bypass a recorded halt. Even when `verify_finalized` is true, verification stays idle if
the endpoint reports no finalized block. If you set `verify_finalized = false`, use it only with `depth` or `finalized` mode, or together
with external verification.

### Which logs are checked

Verification reports three kinds of difference:

- A stored log that the canonical chain does not have.
- A stored log whose block hash or transaction hash changed.
- A canonical log that is missing from the store (`missing_from_store`).

Verification looks for the first two kinds for every contract address, Safe contracts included. It looks for `missing_from_store` only for
the static contracts in the startup configuration. The reason is that the indexer discovers Safe contract addresses while it indexes.

## WebSocket Head Source

By default, the tracker uses a `newHeads` subscription over WebSocket. Polling is the fallback and not an error. Thus an endpoint without
WebSocket support needs no change.

- `blokli_head_source` is `1` when the subscription is active and `0` when the tracker polls.
- `blokli_head_ws_reconnects_total` counts successful reconnects. A value that increases shows an unstable socket.
- `rpc_ws_url` overrides the WebSocket URL. If the provider serves WebSocket on a different path or port from `rpc_url`, set it.
- `head_subscription = false` forces polling and disables the WebSocket code path.

A fallback at startup and a fallback at runtime give different diagnostics. At startup, a probe compares the chain ID and the block hash of
the first header with the HTTP endpoint. It detects a disagreement at the probed block. It does not prove that both URLs lead to the same
node.

If the subscription is not supported, or the chain ID or block hash does not match, the tracker polls for the lifetime of the process. The
tracker retries a connection failure with backoff. This includes a failure at startup. It also includes the case where the HTTP endpoint
does not have the block of the first header yet. The startup warning shows the redacted URL, a category (`unsupported` or `connection`) and
the JSON-RPC error code. It does not show the error text of the endpoint.

The silence window is the maximum time that the tracker waits for a new header. At runtime, the tracker logs the warning
`switching to polling` in three cases: the socket closes, the socket fails, or no header arrives within the silence window. The tracker
reconnects with backoff. A successful reconnect increments `blokli_head_ws_reconnects_total`.

If `blokli_head_source` is `0` and you want the subscription, do these steps:

1. If the failure occurred at startup, read the redacted WebSocket URL, the category and the JSON-RPC error code in the startup warning.
2. Make sure that `rpc_url` or `rpc_ws_url` points to an endpoint that accepts `eth_subscribe`.
3. Make sure that both URLs serve the same chain. A chain ID or block hash mismatch keeps the tracker on polling.
4. If the provider uses a dedicated WebSocket path or port, set `rpc_ws_url`.
5. Restart the daemon.
6. Make sure that `blokli_head_source` is `1`.

If the fallback occurred at runtime, find the warning `switching to polling` in the logs. Then watch `blokli_head_ws_reconnects_total`.

If reconnects continue to fail, the tracker continues to poll and to index. To get the lower latency back, repair the WebSocket connection.

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
| `blokli_indexer_head_lag_seconds`      | last lag sample near the tip, taken when the block timestamp is cached     |
| `blokli_health`                        | health status, including `verification_failed` after a failed verification |
