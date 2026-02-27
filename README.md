# Blokli

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

Run the GraphQL API server on its own:

```bash
just run-api
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

| Variable                     | Default         | Description                                     |
| ---------------------------- | --------------- | ----------------------------------------------- |
| `RUST_LOG`                   | `info`          | Logging level (error, warn, info, debug, trace) |
| `ANVIL_BLOCK_TIME`           | `1`             | Block time in seconds                           |
| `ANVIL_ACCOUNTS`             | `10`            | Number of accounts to create                    |
| `ANVIL_BALANCE`              | `10000`         | Initial balance per account (ETH)               |
| `ANVIL_DEPLOYER_PRIVATE_KEY` | (first account) | Private key for contract deployment             |
| `BLOKLI_DATA_DIRECTORY`      | `/data`         | Data directory for SQLite databases             |

Once running, access the GraphQL playground at: <http://localhost:8080/graphql>

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

# Run integration tests
just test-indexer
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
# Temporal query tests
cargo test -p blokli-db-sql --lib state_queries -F runtime-tokio

# Reorg handling tests
cargo test -p blokli-chain-indexer --lib block::tests::test_handle_reorg -F runtime-tokio

# Subscription tests
cargo test -p blokli-db-sql --lib subscription -F runtime-tokio

# Integration tests
cargo test -p bloklid --test indexer_startup_test -F runtime-tokio

# Transaction integration tests
cargo test -p blokli-chain-api --test transaction_integration_test -F runtime-tokio -- --test-threads=1

# Run specific test by name
cargo test test_get_channel_state_at -F runtime-tokio -- --nocapture
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
- `blokli-api/`: GraphQL API server
- `db/`: Database abstractions, entities, and migrations
- `design/`: Architecture and target schema references
- `tests/`: Integration and smoke tests

## Documentation

- **[TESTING.md](TESTING.md)** - Comprehensive testing guide
- **`design/architecture.md`** - System architecture and data flows
- **`design/target-api-schema.graphql`** - Target GraphQL schema reference
- **`design/target-db-schema.mmd`** - Target database schema reference

## Configuration

Blokli can be configured via a configuration file (TOML) or environment variables. The precedence order is:

1. Environment Variables (Specific `BLOKLI_` vars or canonical `DATABASE_` vars)
2. Configuration File
3. Default Values

To generate a template configuration file:

```bash
bloklid generate-config config.toml
```

For a complete example with defaults and comments, see `bloklid/example-config.toml`.

### Environment Variables

You can override any configuration setting using environment variables.

#### Root Configuration

| Config Key                 | Environment Variable              |
| :------------------------- | :-------------------------------- |
| `host`                     | `BLOKLI_HOST`                     |
| `data_directory`           | `BLOKLI_DATA_DIRECTORY`           |
| `network`                  | `BLOKLI_NETWORK`                  |
| `rpc_url`                  | `BLOKLI_RPC_URL`                  |
| `max_rpc_requests_per_sec` | `BLOKLI_MAX_RPC_REQUESTS_PER_SEC` |

#### Database Configuration

| Config Key                 | Primary Env Var                   | Canonical Env Vars                |
| :------------------------- | :-------------------------------- | :-------------------------------- |
| `database.url`             | `BLOKLI_DATABASE_URL`             | `DATABASE_URL`                    |
| `database.host`            | `BLOKLI_DATABASE_HOST`            | `PGHOST`, `POSTGRES_HOST`         |
| `database.port`            | `BLOKLI_DATABASE_PORT`            | `PGPORT`, `POSTGRES_PORT`         |
| `database.username`        | `BLOKLI_DATABASE_USERNAME`        | `PGUSER`, `POSTGRES_USER`         |
| `database.password`        | `BLOKLI_DATABASE_PASSWORD`        | `PGPASSWORD`, `POSTGRES_PASSWORD` |
| `database.database`        | `BLOKLI_DATABASE_DATABASE`        | `PGDATABASE`, `POSTGRES_DB`       |
| `database.type`            | `BLOKLI_DATABASE_TYPE`            | -                                 |
| `database.max_connections` | `BLOKLI_DATABASE_MAX_CONNECTIONS` | -                                 |
| `database.index_path`      | `BLOKLI_DATABASE_INDEX_PATH`      | -                                 |
| `database.logs_path`       | `BLOKLI_DATABASE_LOGS_PATH`       | -                                 |

#### Indexer Configuration

| Config Key                                      | Environment Variable                                   |
| :---------------------------------------------- | :----------------------------------------------------- |
| `indexer.fast_sync`                             | `BLOKLI_INDEXER_FAST_SYNC`                             |
| `indexer.enable_logs_snapshot`                  | `BLOKLI_INDEXER_ENABLE_LOGS_SNAPSHOT`                  |
| `indexer.logs_snapshot_url`                     | `BLOKLI_INDEXER_LOGS_SNAPSHOT_URL`                     |
| `indexer.subscription.event_bus_capacity`       | `BLOKLI_INDEXER_SUBSCRIPTION_EVENT_BUS_CAPACITY`       |
| `indexer.subscription.shutdown_signal_capacity` | `BLOKLI_INDEXER_SUBSCRIPTION_SHUTDOWN_SIGNAL_CAPACITY` |
| `indexer.subscription.batch_size`               | `BLOKLI_INDEXER_SUBSCRIPTION_BATCH_SIZE`               |

#### API Configuration

| Config Key                            | Environment Variable                         |
| ------------------------------------- | -------------------------------------------- |
| `api.enabled`                         | `BLOKLI_API_ENABLED`                         |
| `api.bind_address`                    | `BLOKLI_API_BIND_ADDRESS`                    |
| `api.playground_enabled`              | `BLOKLI_API_PLAYGROUND_ENABLED`              |
| `api.gas_multiplier`                  | `BLOKLI_API_GAS_MULTIPLIER`                  |
| `api.sse_keepalive.enabled`           | `BLOKLI_API_SSE_KEEPALIVE_ENABLED`           |
| `api.sse_keepalive.interval`          | `BLOKLI_API_SSE_KEEPALIVE_INTERVAL`          |
| `api.sse_keepalive.text`              | `BLOKLI_API_SSE_KEEPALIVE_TEXT`              |
| `api.health.max_indexer_lag`          | `BLOKLI_API_HEALTH_MAX_INDEXER_LAG`          |
| `api.health.timeout`                  | `BLOKLI_API_HEALTH_TIMEOUT`                  |
| `api.health.readiness_check_interval` | `BLOKLI_API_HEALTH_READINESS_CHECK_INTERVAL` |

GraphQL subscriptions stream over SSE and send periodic keep-alive events to prevent idle connection timeouts. Keep-alive is enabled by
default with a 15s interval and `keep-alive` payload, and can be customized via the `api.sse_keepalive.*` settings. `api.gas_multiplier`
(default `1.0`, minimum `1.0`) scales `chainInfo.maxFeePerGas` and `chainInfo.maxPriorityFeePerGas` (rounded up to whole wei).
`chainInfo.gasPrice` is not scaled.

### Contract Address Overrides

You can override contract addresses via the configuration file. By default, addresses are resolved from hopr-bindings based on the selected
network.

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
```
