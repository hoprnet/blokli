# Blokli

This repository contains `Blokli`: On-chain Indexer of HOPR smart contracts and on-chain operations provider.

## Development

This project uses [just](https://github.com/casey/just) as a command runner and [Nix Flake](https://nix.dev/manual/nix/2.30/command-ref/new-cli/nix3-flake.html#description) as the build system.

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

## Documentation

- **[TESTING.md](TESTING.md)** - Comprehensive testing guide
- **[design/](design/)** - Design documents and architecture

## Configuration

Blokli can be configured via a configuration file (TOML) or environment variables. The precedence order is:

1. Environment Variables (Specific `BLOKLI_` vars or canonical `DATABASE_` vars)
2. Configuration File
3. Default Values

To generate a template configuration file:

```bash
bloklid generate-config config.toml
```

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

| Config Key                   | Environment Variable                |
| :--------------------------- | :---------------------------------- |
| `api.enabled`                | `BLOKLI_API_ENABLED`                |
| `api.bind_address`           | `BLOKLI_API_BIND_ADDRESS`           |
| `api.playground_enabled`     | `BLOKLI_API_PLAYGROUND_ENABLED`     |
| `api.health.max_indexer_lag` | `BLOKLI_API_HEALTH_MAX_INDEXER_LAG` |
| `api.health.timeout`         | `BLOKLI_API_HEALTH_TIMEOUT`         |
