# Blokli

[![codecov](https://codecov.io/gh/hoprnet/blokli/branch/main/graph/badge.svg)](https://codecov.io/gh/hoprnet/blokli)

This repository contains `Blokli`: On-chain Indexer of HOPR smart contracts and on-chain operations provider.

## Components

- `bloklid`: Daemon that indexes on-chain events and submits transactions
- `blokli-api`: GraphQL server for querying indexed data and streaming updates over SSE with keep-alive support
- `db`: Database abstractions, entities, and migrations

## Development

This project uses [just](https://github.com/casey/just) as a command runner and
[Nix Flake](https://nix.dev/manual/nix/2.30/command-ref/new-cli/nix3-flake.html#description) as the build system.

### Quick Start

Enter the Nix development environment:

```bash
nix develop
```

Build the project:

```bash
just build
```

Run tests:

```bash
just test
```

Format, lint, and check compilation (recommended after changes):

```bash
just quick
```

### Run Locally

Run the indexer daemon:

```bash
just run
```

`bloklid` embeds the GraphQL API server. The `blokli-api` binary is only used to export the schema; for example:

```bash
just export-schema-sqlite
```

## Docker Images

### Blokli + Anvil (single container)

This image runs `anvil` with a 1s block time, deploys contracts, and starts `bloklid` against the local chain. Only the GraphQL API port is
exposed.

```bash
# Build the image
just docker-build-anvil

# Run the container (default: RUST_LOG=info)
just docker-run-anvil

# Run with debug logging
just docker-run-anvil debug

# Run with trace logging
just docker-run-anvil trace
```

**Environment Variables:**

| Variable                              | Default                   | Description                                              |
| ------------------------------------- | ------------------------- | -------------------------------------------------------- |
| `RUST_LOG`                            | `info`                    | Tracing filter, such as `info` or `bloklid=debug`        |
| `BLOKLI_LOG_FORMAT`                   | text                      | Set to `json` for structured JSON logs                   |
| `ANVIL_HOST`                          | `127.0.0.1`               | Anvil listen address                                     |
| `ANVIL_PORT`                          | `8545`                    | Anvil RPC port                                           |
| `ANVIL_BLOCK_TIME`                    | `1`                       | Block time in seconds                                    |
| `ANVIL_ACCOUNTS`                      | `10`                      | Number of accounts to create                             |
| `ANVIL_BALANCE`                       | `10000`                   | Initial balance per account in ETH                       |
| `ANVIL_RPC_URL`                       | derived from `ANVIL_PORT` | URL used for readiness checks, deployment, and `bloklid` |
| `ANVIL_DEPLOYER_PRIVATE_KEY`          | Anvil's first account     | Private key for HOPR contract deployment                 |
| `ANVIL_COMMON_DEPLOYER_PRIVATE_KEY`   | Anvil's second account    | Private key for common contract deployment               |
| `BLOKLI_DEPLOYER_TICKET_PRICE`        | `100 wei wxHOPR`          | Initial ticket price oracle value                        |
| `BLOKLI_DEPLOYER_WINNING_PROBABILITY` | `0.000125`                | Initial winning probability oracle value                 |
| `BLOKLI_DATA_DIRECTORY`               | `/data`                   | Data directory for SQLite databases                      |
| `BLOKLI_CONFIG_PATH`                  | `/config.toml`            | Generated daemon configuration path                      |

Once running, access the GraphQL playground at: <http://localhost:8080/graphql>

Prometheus metrics are available at: <http://localhost:8080/metrics>

To push daemon telemetry to an OpenTelemetry collector, configure the `[telemetry]` section in `bloklid/example-config.toml`. See
[OTLP.md](OTLP.md) for transport rules, signal selection, environment overrides, and example configurations.

## Testing

Blokli has comprehensive test coverage for temporal queries, blockchain reorganization handling, subscriptions, and edge cases.

### Quick Test Commands

```bash
# Run all tests
just test

# Run specific package tests
just test-package blokli-db             # Database and temporal query tests
just test-package blokli-chain-indexer  # Indexer and reorg handling tests

# Run tests with debug output (single-threaded, shows println!)
just test-debug
```

### Smoke Tests

Smoke tests verify that `bloklid` can start and connect to external dependencies. Logs are automatically saved to local files for inspection
after each test run.

```bash
# Test with local Anvil (fast, 30s timeout, no external deps)
just smoke-test

# Test with Gnosis Chain RPC (allows high lag, 30s timeout)
just smoke-test-gnosis

# Test with Gnosis Chain RPC (requires full sync within 10 blocks, 600s timeout)
just smoke-test-gnosis-full-sync

# Run all smoke tests (builds Docker image first)
just smoke-test-full
```

You can also run them manually:

```bash
# Test with local Anvil (fast, 30s timeout, no external deps)
cd tests/smoke && ./run-smoke-test.sh

# Test with Gnosis Chain RPC (allows high lag, 30s timeout)
SMOKE_CONFIG=config-smoke-gnosis.toml ./run-smoke-test.sh

# Test with Gnosis Chain RPC (requires full sync within 10 blocks, 600s timeout)
SMOKE_CONFIG=config-smoke-gnosis-full-sync.toml ./run-smoke-test.sh
```

**Log Files**: After each test run, logs are saved as `blokli-smoke-{config}-{timestamp}.log` in the `tests/smoke/` directory for debugging
failed tests.

### Testing Guide

See **[TESTING.md](TESTING.md)** for the complete testing guide.

### Running Specific Tests

```bash
# Database tests
cargo test -p blokli-db -F runtime-tokio

# Reorg handling tests
cargo test -p blokli-chain-indexer -F runtime-tokio

# Subscription tests
cargo test -p blokli-api --test safe_subscription_test -F runtime-tokio

# Integration tests
cargo test -p bloklid --test indexer_startup_test -F runtime-tokio

# Transaction integration tests
cargo test -p blokli-chain-api --test transaction_integration_test -F runtime-tokio -- --test-threads=1

# Run a specific test by name
cargo test -p blokli-db test_block_position_ordering -F runtime-tokio -- --nocapture
```

## Architecture

Blokli implements a temporal database system for tracking blockchain state changes with full audit trail preservation.

### Key Features

- **Temporal Queries**: Query state at any point in blockchain history using `(block, tx_index, log_index)` positions
- **Reorg Handling**: Preserves audit trail during blockchain reorganizations with corrective states
- **Never-Delete Principle**: Historical data never deleted, only corrective states added
- **Position Ordering**: Lexicographic ordering ensures correct temporal queries
- **Performance**: Efficient point-in-time queries using database indexes

## Repository Layout

- `bloklid/`: Indexer daemon and chain operations
- `api/`: GraphQL API server
- `db/`: Database abstractions, entities, and migrations
- `design/`: Architecture and target schema references
- `tests/`: Integration and smoke tests

## Documentation

- **[TESTING.md](TESTING.md)** - Comprehensive testing guide
- **[docs/guide-internal-tx-debugging.md](docs/guide-internal-tx-debugging.md)** - Debugging Safe internal transactions with `cast`
- **`design/architecture.md`** - System architecture and data flows
- **`design/target-api-schema.graphql`** - Target GraphQL schema reference
- **`design/target-db-schema.mmd`** - Target database schema reference

## Configuration

Blokli can be configured via a configuration file (TOML) and the explicitly mapped environment variables below. The precedence order is:

1. Environment Variables (Specific `BLOKLI_` vars or canonical `DATABASE_` vars)
2. Configuration File
3. Default Values

The path to the configuration file can be specified via the `-c` flag or the `BLOKLI_CONFIG_PATH` environment variable (`BLOKLI_CONFIG_PATH`
takes priority). If neither is set, the daemon will try `/etc/bloklid/bloklid.toml` if it exists; otherwise it starts using only environment
variables and built-in defaults.

To generate a template configuration file:

```bash
bloklid generate-config config.toml
```

For fast-sync bootstrap, configure `indexer.fast_sync = true`, `indexer.enable_logs_snapshot = true`, and `indexer.logs_snapshot_url` to a
`.tar.xz` archive that contains `hopr_logs.sql`. On an empty node, `bloklid` imports that file into the raw logs tables, rebuilds derived
state locally, and then resumes normal RPC catch-up from the snapshot end. If the configured snapshot restore fails, startup fails.

For a complete example with comments, see [`bloklid/example-config.toml`](bloklid/example-config.toml). Duration values use human-readable
syntax such as `500ms`, `15s`, and `1m`.

### Configuration Reference

Only the mappings listed below are supported. A dash means the setting is available only in TOML. `BLOKLI_DATABASE_*` variables take
priority over their canonical database aliases when both are set. Boolean environment values accept `true`/`false` and `1`/`0`.

#### Process Configuration

| Setting            | Default   | Environment Variable | Description                                                    |
| :----------------- | :-------- | :------------------- | :------------------------------------------------------------- |
| Configuration file | automatic | `BLOKLI_CONFIG_PATH` | Overrides `-c`; otherwise `/etc/bloklid/bloklid.toml` is tried |
| Log filter         | `info`    | `RUST_LOG`           | Standard `tracing-subscriber` filter syntax                    |
| Log format         | text      | `BLOKLI_LOG_FORMAT`  | Set to `json` for structured JSON output                       |

#### Root Configuration

| Config Key                 | Default                 | Environment Variable              | Description                                                               |
| :------------------------- | :---------------------- | :-------------------------------- | :------------------------------------------------------------------------ |
| `data_directory`           | `data`                  | `BLOKLI_DATA_DIRECTORY`           | Directory for daemon data                                                 |
| `network`                  | `jura-dev`              | `BLOKLI_NETWORK`                  | `jura-dev`, `jura-prod`, or `anvil-localhost` (`localhost` is an alias)   |
| `rpc_url`                  | `http://localhost:8545` | `BLOKLI_RPC_URL`                  | Chain JSON-RPC endpoint                                                   |
| `max_rpc_requests_per_sec` | `100`                   | `BLOKLI_MAX_RPC_REQUESTS_PER_SEC` | Maximum request rate; `0` means unlimited                                 |
| `max_block_range`          | `10000`                 | `BLOKLI_MAX_BLOCK_RANGE`          | Ceiling for adaptive `eth_getLogs` ranges; `0` auto-discovers up to 10000 |

The daemon also requires a `[database]` table or equivalent database environment variables.

#### Database Configuration

`database.type` is required and accepts `postgresql`, `sqlite`, or `in-memory`. PostgreSQL accepts either `url` or the individual connection
fields. SQLite uses separate index and raw-log databases.

| Config Key                 | Default                        | Primary Env Var                   | Canonical Env Vars                |
| :------------------------- | :----------------------------- | :-------------------------------- | :-------------------------------- |
| `database.type`            | required                       | `BLOKLI_DATABASE_TYPE`            | —                                 |
| `database.url`             | unset                          | `BLOKLI_DATABASE_URL`             | `DATABASE_URL`                    |
| `database.host`            | `localhost` when URL is absent | `BLOKLI_DATABASE_HOST`            | `PGHOST`, `POSTGRES_HOST`         |
| `database.port`            | `5432` when URL is absent      | `BLOKLI_DATABASE_PORT`            | `PGPORT`, `POSTGRES_PORT`         |
| `database.username`        | empty when URL is absent       | `BLOKLI_DATABASE_USERNAME`        | `PGUSER`, `POSTGRES_USER`         |
| `database.password`        | empty when URL is absent       | `BLOKLI_DATABASE_PASSWORD`        | `PGPASSWORD`, `POSTGRES_PASSWORD` |
| `database.database`        | empty when URL is absent       | `BLOKLI_DATABASE_DATABASE`        | `PGDATABASE`, `POSTGRES_DB`       |
| `database.max_connections` | `10`                           | `BLOKLI_DATABASE_MAX_CONNECTIONS` | —                                 |
| `database.index_path`      | `data/bloklid-index.db`        | `BLOKLI_DATABASE_INDEX_PATH`      | —                                 |
| `database.logs_path`       | `data/bloklid-logs.db`         | `BLOKLI_DATABASE_LOGS_PATH`       | —                                 |

#### Indexer Configuration

| Config Key                                      | Default | Environment Variable                                   | Description                                           |
| :---------------------------------------------- | :------ | :----------------------------------------------------- | :---------------------------------------------------- |
| `indexer.fast_sync`                             | `true`  | `BLOKLI_INDEXER_FAST_SYNC`                             | Enables the fast initial synchronization path         |
| `indexer.enable_logs_snapshot`                  | `false` | `BLOKLI_INDEXER_ENABLE_LOGS_SNAPSHOT`                  | Restores raw logs from a snapshot before catch-up     |
| `indexer.enable_safe_indexing`                  | `false` | `BLOKLI_INDEXER_ENABLE_SAFE_INDEXING`                  | Indexes Safe events after Safe discovery              |
| `indexer.logs_snapshot_url`                     | unset   | `BLOKLI_INDEXER_LOGS_SNAPSHOT_URL`                     | URL of a `.tar.xz` archive containing `hopr_logs.sql` |
| `indexer.subscription.event_bus_capacity`       | `1000`  | `BLOKLI_INDEXER_SUBSCRIPTION_EVENT_BUS_CAPACITY`       | Channel-event bus capacity                            |
| `indexer.subscription.shutdown_signal_capacity` | `10`    | `BLOKLI_INDEXER_SUBSCRIPTION_SHUTDOWN_SIGNAL_CAPACITY` | Shutdown signal buffer capacity                       |
| `indexer.subscription.batch_size`               | `100`   | `BLOKLI_INDEXER_SUBSCRIPTION_BATCH_SIZE`               | Historical subscription query batch size              |

#### API Configuration

| Config Key                            | Default          | Environment Variable                         | Description                                   |
| ------------------------------------- | ---------------- | -------------------------------------------- | --------------------------------------------- |
| `api.enabled`                         | `true`           | `BLOKLI_API_ENABLED`                         | Enables the embedded GraphQL server           |
| `api.bind_address`                    | `127.0.0.1:8080` | `BLOKLI_API_BIND_ADDRESS`                    | API listen address                            |
| `api.playground_enabled`              | `false`          | `BLOKLI_API_PLAYGROUND_ENABLED`              | Enables GraphQL Playground                    |
| `api.gas_multiplier`                  | `1.0`            | `BLOKLI_API_GAS_MULTIPLIER`                  | Finite EIP-1559 fee multiplier, minimum `1.0` |
| `api.max_query_depth`                 | `8`              | —                                            | Maximum GraphQL nesting depth                 |
| `api.max_query_complexity`            | `500`            | —                                            | Maximum GraphQL complexity budget             |
| `api.sse_keepalive.enabled`           | `true`           | `BLOKLI_API_SSE_KEEPALIVE_ENABLED`           | Enables SSE keep-alive events                 |
| `api.sse_keepalive.interval`          | `15s`            | `BLOKLI_API_SSE_KEEPALIVE_INTERVAL`          | Keep-alive interval                           |
| `api.sse_keepalive.text`              | `keep-alive`     | `BLOKLI_API_SSE_KEEPALIVE_TEXT`              | Keep-alive payload                            |
| `api.health.max_indexer_lag`          | `10`             | `BLOKLI_API_HEALTH_MAX_INDEXER_LAG`          | Maximum finality-adjusted lag for readiness   |
| `api.health.timeout`                  | `5s`             | `BLOKLI_API_HEALTH_TIMEOUT`                  | Readiness query timeout                       |
| `api.health.readiness_check_interval` | `60s`            | `BLOKLI_API_HEALTH_READINESS_CHECK_INTERVAL` | Cached readiness refresh interval             |

GraphQL subscriptions stream over SSE and send periodic keep-alive events to prevent idle connection timeouts. Keep-alive is enabled by
default with a 15s interval and `keep-alive` payload, and can be customized via the `api.sse_keepalive.*` settings. `api.gas_multiplier`
(default `1.0`, minimum `1.0`) scales `chainInfo.maxFeePerGas` and `chainInfo.maxPriorityFeePerGas` (rounded up to whole wei).
`chainInfo.gasPrice` is not scaled.

#### Telemetry Configuration

| Config Key                         | Default   | Environment Variable            | Description                                                     |
| ---------------------------------- | --------- | ------------------------------- | --------------------------------------------------------------- |
| `telemetry.otlp_endpoint`          | unset     | `BLOKLI_OTLP_ENDPOINT`          | OTLP collector base URL; unset disables OTLP export             |
| `telemetry.metric_export_interval` | `15s`     | `BLOKLI_METRIC_EXPORT_INTERVAL` | Metrics export interval; must be greater than zero when enabled |
| `telemetry.otlp_signals`           | `metrics` | `BLOKLI_OTLP_SIGNALS`           | Comma-separated `metrics`, `traces`, and/or `logs`              |

See [OTLP.md](OTLP.md) for endpoint schemes, per-signal paths, and collector examples.

### Contract Address Overrides

You can override contract addresses via TOML only; no contract-address environment variables are defined. By default, addresses are resolved
from `hopr-bindings` for the selected network. If `[contracts]` is present it replaces the complete resolved set: all fields below except
`xhopr_token` are required. `xhopr_token` defaults to the zero address when omitted. Values are quoted `0x`-prefixed, 20-byte hex strings.

```toml
[contracts]
token = "0x0000000000000000000000000000000000000000"
channels = "0x0000000000000000000000000000000000000000"
announcements = "0x0000000000000000000000000000000000000000"
module_implementation = "0x0000000000000000000000000000000000000000"
node_safe_migration = "0x0000000000000000000000000000000000000000"
node_safe_registry = "0x0000000000000000000000000000000000000000"
ticket_price_oracle = "0x0000000000000000000000000000000000000000"
winning_probability_oracle = "0x0000000000000000000000000000000000000000"
node_stake_factory = "0x0000000000000000000000000000000000000000"
xhopr_token = "0x0000000000000000000000000000000000000000"
```

#### Contract Deployer Environment Variables

These variables configure `blokli-contract-deployer` when it is run directly. Command-line options take precedence.

| Environment Variable                  | Default                    | Description                       |
| ------------------------------------- | -------------------------- | --------------------------------- |
| `BLOKLI_DEPLOYER_RPC_URL`             | `http://127.0.0.1:8545`    | Deployment RPC endpoint           |
| `ANVIL_DEPLOYER_PRIVATE_KEY`          | Anvil's first account key  | HOPR contract deployer key        |
| `ANVIL_COMMON_DEPLOYER_PRIVATE_KEY`   | Anvil's second account key | Common contract deployer key      |
| `BLOKLI_DEPLOYER_TICKET_PRICE`        | `100 wei wxHOPR`           | Initial ticket price oracle value |
| `BLOKLI_DEPLOYER_WINNING_PROBABILITY` | `0.000125`                 | Initial winning probability       |
