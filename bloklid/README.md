# Bloklid - HOPR Chain Indexer Daemon

Bloklid is a daemon for indexing HOPR on-chain events and executing HOPR-related on-chain transactions.

## Features

- **Chain Event Indexing**: Continuously monitors and indexes blockchain events from HOPR smart contracts
- **Database Storage**: Stores indexed data in SQLite databases for efficient querying
- **Signal Handling**: Supports SIGHUP for configuration reload and SIGINT/SIGTERM for graceful shutdown
- **Fast Sync**: Supports fast synchronization through pre-built logs database snapshots

## Usage

```bash
# Run with default configuration
bloklid

# Run with custom configuration file
bloklid -c config.yaml

# Increase verbosity
bloklid -v    # debug level
bloklid -vv   # trace level
```

## Configuration

See `example-config.yaml` for a complete configuration example. Key settings include:

- `host`: API server bind address (default: "0.0.0.0:3064")
- `database_path`: Path to SQLite database file
- `private_key`: Ethereum private key for chain operations
- `rpc_url`: Ethereum JSON-RPC endpoint
- `indexer`: Indexer-specific configuration
  - `start_block_number`: Block to start indexing from
  - `fast_sync`: Enable fast synchronization
  - `enable_logs_snapshot`: Enable snapshot download for faster initial sync

## Architecture

The daemon integrates several components:

1. **RPC Client**: Connects to an Ethereum node for blockchain data
2. **Indexer**: Processes blockchain logs and extracts HOPR-specific events
3. **Database**: Stores indexed data in SQLite for persistence
4. **Event Handlers**: Processes contract events and updates database state

## Signal Handling

- **SIGHUP**: Reloads configuration (note: indexer continues with original settings)
- **SIGINT/SIGTERM**: Gracefully shuts down the daemon
