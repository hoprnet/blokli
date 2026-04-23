# Bloklid - HOPR Chain Indexer Daemon

Bloklid is a daemon for indexing HOPR on-chain events and executing HOPR-related on-chain transactions.

## Features

- **Chain Event Indexing**: Continuously monitors and indexes blockchain events from HOPR smart contracts
- **Database Storage**: Stores indexed data in SQLite databases for efficient querying
- **Signal Handling**: Supports SIGHUP for configuration reload and SIGINT/SIGTERM for graceful shutdown
- **Fast Sync**: Supports fast synchronization through downloadable `hopr_logs.sql` logs snapshots
- **OTLP Telemetry Export**: Pushes selected metrics, traces, and/or logs to an OpenTelemetry collector when `telemetry.otlp_endpoint` is
  configured
- **Operational Endpoints**: Exposes embedded API health and readiness endpoints when the API server is enabled

## Usage

```bash
# Run with default configuration
bloklid

# Run with custom configuration file
bloklid -c config.toml

# Increase verbosity
bloklid -v    # debug level
bloklid -vv   # trace level

# Export a logs snapshot archive from the current database
bloklid export-logs-snapshot logs-snapshot.tar.xz
```

## Configuration

See `example-config.toml` for a complete configuration example. Key settings include:

- `api.bind_address`: API server bind address (default: "0.0.0.0:8080")
- `database_path`: Path to SQLite database file
- `private_key`: Ethereum private key for chain operations
- `rpc_url`: Ethereum JSON-RPC endpoint
- `telemetry`: OTLP metrics export configuration
  - `otlp_endpoint`: Collector endpoint for OTLP telemetry; if unset, OTLP export is disabled
  - `otlp_signals`: Comma-separated OTLP signals to export (`metrics`, `traces`, `logs`)
  - `metric_export_interval`: Push interval for OTLP metrics when `metrics` export is enabled

For the complete OTLP setup guide, including environment overrides and endpoint examples, see [OTLP.md](../OTLP.md).

- `indexer`: Indexer-specific configuration
  - `start_block_number`: Block to start indexing from
  - `enable_safe_indexing`: Enable Safe contract event indexing for discovered Safes
  - `fast_sync`: Enable fast synchronization
  - `enable_logs_snapshot`: Enable snapshot download for faster initial sync
  - `logs_snapshot_url`: URL of a tar.xz archive containing `hopr_logs.sql`

When `fast_sync = true`, `enable_logs_snapshot = true`, and the local logs database is empty, `bloklid` downloads the configured archive,
validates that it contains a usable `hopr_logs.sql`, imports the raw logs tables, rebuilds derived state from those logs, and then catches
up from the snapshot end to the current chain head. If this configured snapshot restore fails, startup fails instead of silently falling
back to a full historical RPC sync.

## Architecture

The daemon integrates several components:

1. **RPC Client**: Connects to an Ethereum node for blockchain data
2. **Indexer**: Processes blockchain logs and extracts HOPR-specific events
3. **Database**: Stores indexed data in SQLite for persistence
4. **Event Handlers**: Processes contract events and updates database state

## Signal Handling

- **SIGHUP**: Reloads configuration (note: indexer continues with original settings)
- **SIGINT/SIGTERM**: Gracefully shuts down the daemon
