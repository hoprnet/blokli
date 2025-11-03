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
just test-package blokli-db-sql         # Database and temporal query tests
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
- **[design/](design/)** - Design documents and architecture specifications
