# Debugging Safe Internal Transactions with `cast`

This guide explains how to manually inspect Gnosis Safe internal transactions
using Foundry's `cast` CLI. It covers the HOPR-specific module transaction flow
and how to determine whether an internal call succeeded or reverted.

## Background: Safe Module Transaction Flow

HOPR transactions go through a Safe module rather than calling the Safe directly.
The call chain looks like this:

```
EOA tx  ->  Module (execTransactionFromModule)  ->  Safe  ->  Target contract (e.g., HoprChannels)
```

The outer transaction targets the **module address**, not the Safe. The Safe emits
one of these events depending on the internal outcome:

| Internal Result | Event Emitted by Safe                                | Topic0 Hash (first 10 chars) |
|-----------------|------------------------------------------------------|------------------------------|
| Success         | `ExecutionFromModuleSuccess(address indexed module)`  | `0x6895c136...`              |
| Failure         | `ExecutionFromModuleFailure(address indexed module)`  | `0xacd2c870...`              |

The outer transaction **always succeeds** (status=1) even when the internal call
reverts. You must inspect the receipt logs to determine the internal status.

## Prerequisites

All commands below use `cast` from [Foundry](https://book.getfoundry.sh/). It is
available inside the Nix dev shell:

```bash
nix develop
```

Set your RPC URL once for the session:

```bash
# Local Anvil (integration tests)
export ETH_RPC_URL=http://localhost:8546

# Gnosis Chain (production)
# export ETH_RPC_URL=https://rpc.gnosischain.com
```

## Step 1: Get the Transaction Receipt

Start with the outer transaction hash:

```bash
cast receipt <TX_HASH>
```

Key fields in the output:

- **`status`** -- `1` means the outer tx succeeded (almost always 1 for module txs)
- **`to`** -- the module address (not the Safe)
- **`blockNumber`** -- needed for log queries

For JSON output (easier to pipe into `jq`):

```bash
cast receipt <TX_HASH> --json
```

To extract a single field:

```bash
cast receipt <TX_HASH> status
cast receipt <TX_HASH> blockNumber
```

## Step 2: Check the Logs for Safe Execution Events

The receipt contains all event logs emitted during the transaction. Look for Safe
execution events among them:

```bash
cast receipt <TX_HASH> --json | jq '.logs[] | {address, topics, data}'
```

Match `topics[0]` against the known event hashes (see the
[Quick Reference](#quick-reference-topic-hashes) at the bottom).

Alternatively, query logs for a specific event in a block range:

```bash
BLOCK=$(cast receipt <TX_HASH> blockNumber)

# Module success events from a specific Safe address
cast logs \
  --from-block $BLOCK \
  --to-block $BLOCK \
  --address <SAFE_ADDRESS> \
  "ExecutionFromModuleSuccess(address)"

# Module failure events
cast logs \
  --from-block $BLOCK \
  --to-block $BLOCK \
  --address <SAFE_ADDRESS> \
  "ExecutionFromModuleFailure(address)"
```

## Step 3: Resolve Module Address to Safe Address

If you only have the transaction hash (and therefore the module address from the
`to` field), you can find the Safe address by looking at which contract emitted
the execution event:

```bash
cast receipt <TX_HASH> --json \
  | jq '.logs[] | select(
      .topics[0] == "0x6895c13664aa4f67288b25d7a21d7aaa34916e355fb9b6fae0a139a9085becb8"
      or .topics[0] == "0xacd2c87028041289fdb0d2bb49f6d127dd0181c13fd45dbfe16de0930e2bd375"
    ) | {safe_address: .address, event_topic: .topics[0], module_topic: .topics[1]}'
```

In the output:

- **`safe_address`** (`address`) -- the Safe contract that emitted the event
- **`module_topic`** (`topics[1]`) -- the module address, left-padded to 32 bytes;
  the last 40 hex characters are the actual address

## Step 4: Decode the Module Address from the Event

The indexed `module` parameter is stored in `topics[1]` as a 32-byte
left-padded address. Extract the last 20 bytes:

```bash
# Get the raw topic
TOPIC=$(cast receipt <TX_HASH> --json \
  | jq -r '.logs[] | select(
      .topics[0] == "0x6895c13664aa4f67288b25d7a21d7aaa34916e355fb9b6fae0a139a9085becb8"
    ) | .topics[1]')

# The module address is the last 40 hex chars (20 bytes)
echo "Module address: 0x${TOPIC: -40}"
```

## Step 5: Full Execution Trace with `cast run`

For the deepest inspection, replay the transaction locally and see the full
internal call trace:

```bash
# Execution trace with internal call decoding
cast run <TX_HASH> --decode-internal -vvvv
```

This replays the transaction against a forked chain state and prints every
internal `CALL`, `DELEGATECALL`, `STATICCALL`, and `REVERT`. You will see:

- The call from the EOA to the module contract
- The module calling into the Safe via `execTransactionFromModule`
- The Safe calling the target contract (e.g., `HoprChannels.approve(...)`)
- The exact point where an internal revert happens (if any)
- The revert reason or custom error data

Verbosity levels:

| Flag      | Detail Level                                          |
|-----------|-------------------------------------------------------|
| `-vvv`    | Execution traces                                      |
| `-vvvv`   | Execution traces + setup traces                       |
| `-vvvvv`  | Everything including storage changes                  |

## Step 6: Decode a Revert Reason

If `cast run` shows a revert with hex error data, decode it:

```bash
# Auto-detect the error signature via openchain.xyz lookup
cast decode-error <HEX_DATA>

# Or specify the signature explicitly
cast decode-error <HEX_DATA> --sig "MyCustomError(uint256,address)"
```

## Step 7: Identify Unknown Event Topics

If you encounter an unknown `topics[0]` in the logs:

```bash
cast 4byte-event <TOPIC_0_HASH>
```

This queries [openchain.xyz](https://openchain.xyz) for the event signature
matching that topic hash.

You can also compute topic hashes yourself to verify:

```bash
cast keccak "ExecutionFromModuleSuccess(address)"
# 0x6895c13664aa4f67288b25d7a21d7aaa34916e355fb9b6fae0a139a9085becb8
```

## Complete Example

Investigating a failed Safe module transaction end to end:

```bash
TX=0xabc123...

# 1. Confirm the outer tx succeeded (status=1, always true for module txs)
cast receipt $TX status

# 2. Get the block number
BLOCK=$(cast receipt $TX blockNumber)

# 3. Check for failure events in the receipt
cast receipt $TX --json \
  | jq '.logs[] | select(
      .topics[0] == "0xacd2c87028041289fdb0d2bb49f6d127dd0181c13fd45dbfe16de0930e2bd375"
    ) | {safe: .address, module: .topics[1]}'
# If this returns a result, the internal execution failed.
# .address is the Safe, .topics[1] (last 40 hex chars) is the module.

# 4. Replay with full trace to find the revert
cast run $TX --decode-internal -vvvv

# 5. Decode the revert data from the trace output
cast decode-error 0x<revert_data_from_trace>
```

## Quick Reference: Topic Hashes

### Direct Safe Execution (execTransaction)

| Event                                     | Topic0                                                               |
|-------------------------------------------|----------------------------------------------------------------------|
| `ExecutionSuccess(bytes32,uint256)`        | `0x442e715f626346e8c54381002da614f62bee8d27386535b2521ec8540898556e`   |
| `ExecutionFailure(bytes32,uint256)`        | `0x23428b18acfb3ea64b08dc0c1d296ea9c09702c09083ca5272e64d115b687d23`   |

These events include a `bytes32 txHash` parameter (the Safe's internal tx hash)
and a `uint256 payment` parameter.

### Module Execution (execTransactionFromModule)

| Event                                              | Topic0                                                               |
|----------------------------------------------------|----------------------------------------------------------------------|
| `ExecutionFromModuleSuccess(address indexed module)` | `0x6895c13664aa4f67288b25d7a21d7aaa34916e355fb9b6fae0a139a9085becb8` |
| `ExecutionFromModuleFailure(address indexed module)` | `0xacd2c87028041289fdb0d2bb49f6d127dd0181c13fd45dbfe16de0930e2bd375` |

These events only contain the calling module address. There is no Safe `txHash`
parameter. This is the path used by HOPR via `SafePayloadGenerator`.

## Related Code

- `chain/api/src/safe_execution.rs` -- Event topic constants and log inspection logic
- `chain/api/src/transaction_monitor.rs` -- Transaction enrichment with Safe execution data
- `tests/integration/tests/blokli_transaction_client.rs` -- Integration tests for Safe module transactions
