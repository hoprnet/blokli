# Agent Guidelines for HOPR Blokli

Blokli is a Rust workspace project: an on-chain indexer of HOPR smart contracts and on-chain operations provider.

## Project Overview

- `bloklid` — Daemon for indexing HOPR on-chain events
- `blokli-api` — GraphQL API server for querying indexed data
- `db/` — Database modules (SeaORM entities, migrations, abstractions)

**Tech stack:** Rust (2021 edition), Tokio, Axum + async-graphql, PostgreSQL (SeaORM), Nix Flakes + `just`, Ethereum/Gnosis Chain (Alloy)

## Quick Reference

**After making code changes**, always run `just quick` (formats, lints, checks compilation).

## Documentation Map

- `README.md` — High-level overview and quickstart
- `TESTING.md` — Test strategy and commands
- `design/architecture.md` — System architecture (conceptual, no code snippets)
- `design/target-api-schema.graphql` — Target GraphQL schema
- `design/target-db-schema.mmd` — Target database schema

## Build Commands

All `just` commands must be run within `nix develop` shell. Run `just` with no arguments to see available commands.

The `runtime-tokio` feature flag is required and automatically included in `just` commands.

## Code Style

### Naming & Formatting

- snake_case for functions/variables, PascalCase for types/enums
- 4 spaces indentation
- `//!` for module docs, `///` for item docs
- Explicit type annotations on all public function signatures

### Imports

**All imports at module top level** — never inside functions or impl blocks.

Group in order: (1) `std::`, (2) external crates alphabetically, (3) local crates/modules alphabetically.

```rust
use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use tokio::sync::RwLock;

use crate::config::Config;
use crate::indexer::BlockIndexer;
```

- **No wildcard imports** (`use module::*`) — exception: migration files for `sea_orm`/`sea_query`
- **No inline fully-qualified paths** — use imports instead
- **Use workspace dependencies** from root `Cargo.toml`

### Error Handling

- `thiserror::Error` for custom error types
- `anyhow::Result` for application-level errors
- Always return `Result<T>` for fallible operations
- No `unwrap()`/`expect()` in production code

### GraphQL Error Management

All API error handling is centralized in `api/src/errors.rs`. Key rules:

1. **Always use error builder functions** — never construct errors inline
2. **Always use `errors::codes::*` constants** — never hardcode error code strings
3. **Use `crate::errors`** for everything — codes, builders, and message templates

See `api/src/errors.rs` for available builders and codes. When a pattern repeats, add a new builder there.

### Async Code

- Use `async/await` with `tokio::spawn` for concurrency
- `Arc<RwLock<T>>` for shared mutable state
- No blocking operations in async contexts

## HOPR Foundation Types

**Always prefer these over creating new types** for addresses, balances, channels, accounts, and crypto primitives:

| Crate | Key Types |
|-------|-----------|
| `hopr-primitive-types` | `Address`, `Balance<C>`, `HoprBalance`, `XDaiBalance`, `U256`, `SerializableLog`, `ToHex`, `IntoEndian` |
| `hopr-crypto-types` | `Hash`, `OffchainPublicKey`, `ChainKeypair`, `OffchainKeypair` |
| `hopr-internal-types` | `ChannelEntry`, `ChannelStatus`, `AccountEntry`, `AccountType`, `AcknowledgedTicket` |
| `hopr-bindings` | Smart contract bindings and event encoding/decoding |

Use prelude modules: `hopr_types::primitive::prelude`, `hopr_types::crypto::prelude`.
Implement `From`/`TryFrom` for DB model conversions (see `db/entity/src/conversions/`).
Run `cargo doc --package <crate-name> --open` to explore the full API.

## Design

### GraphQL API

- Target schema: `design/target-api-schema.graphql`
- Generate actual schema: `just export-schema-sqlite` → `schema.graphql`
- Use DataLoader pattern for N+1 prevention
- Subscriptions via SSE with keep-alive

**Context type safety:** All schema context data MUST use newtype wrappers — never raw primitives. Existing wrappers in `api/src/schema.rs`: `ChainId(u64)`, `NetworkName(String)`, `ExpectedBlockTime(u64)`, `Finality(u16)`.

### Database

- Target schema: `design/target-db-schema.mmd`
- Attribute names must match target schema
- Use SeaORM entities from `db/entity/src/codegen/`
- Use `*_current` database views for latest state (not manual `ORDER BY ... LIMIT 1`)
- Batch-fetch accounts with HashMap for O(1) lookup (avoid N+1 patterns)

### Real-Time Subscriptions

GraphQL subscriptions use an in-memory `async_broadcast` event bus via `IndexerState` (`chain/indexer/src/state.rs`). Subscriptions follow a 2-phase model: (1) atomically capture a watermark and subscribe to the event bus under a coordination lock, (2) stream matching events to the client. See `api/src/subscription.rs` for implementation.

To add a new subscription event: publish a new `IndexerEvent` variant from the indexer handlers, then filter for it in the subscription resolver.

### Configuration

Config files: `bloklid/src/config.rs` (structs) and `bloklid/example-config.toml` (documentation).

**These must stay in sync.** When modifying config code, update the example config to match — add/remove/rename fields, update defaults, document new sections. Exclude `#[serde(skip)]` fields and auto-generated sections like `protocols`.

### Architecture Documentation

Read `design/architecture.md` before significant changes. Update it when adding components, changing data flows, modifying schema, or altering deployment models. Keep it conceptual — no code, CLI commands, or config snippets.

### Transaction Store

`chain/api/src/transaction_store.rs` — in-memory only, by design. Lost on restart. Only tracks `async`/`sync` mode transactions. Use on-chain confirmation for permanent records.

## Testing

- Unit tests: `#[cfg(test)]` in same file, `#[tokio::test]` for async
- Test error paths and happy paths
- Mock external dependencies (RPC, database)

### Integration Tests

Full-stack tests against real blockchain (Anvil) + PostgreSQL in Docker.

```bash
nix build .#docker-blokli-dev   # Build Docker image first
just test-indexer                # Run integration tests
```

**When to use:** E2E transaction flows, GraphQL queries against indexed data, Safe/channel operations, bloklid indexing verification.

**When NOT to use:** Unit logic, anything not needing a running blockchain/bloklid.

Tests use `rstest` fixtures with `#[serial]`. Pattern:

```rust
#[rstest]
#[test_log::test(tokio::test)]
#[serial]
async fn test_my_feature(#[future(awt)] fixture: IntegrationFixture) -> Result<()> {
    let [sender, recipient] = fixture.sample_accounts::<2>();
    // ... build and submit transactions via fixture helpers
    Ok(())
}
```

See `tests/integration/` for fixture API (`IntegrationFixture`, `RpcClient`), Docker stack config, and environment variables. Tests are organized by client trait in `tests/blokli_query_client.rs`, `tests/blokli_subscription_client.rs`, and `tests/blokli_transaction_client.rs`.

For Safe module transactions in tests, use `SafePayloadGenerator` from `hopr-chain-connector`.

Debug: container logs saved to `/tmp/blokli-integration/<timestamp>/`, use `RUST_LOG=debug`.

## What to Avoid

- Wildcard imports, imports inside functions/impl blocks, inline fully-qualified paths
- Creating custom types when HOPR foundation types exist
- Creating custom contract encoding/decoding when `hopr-bindings` provides it
- Missing type annotations on public functions
- `unwrap()`/`expect()` in production code
- `copy_from_slice` for fixed-size arrays (use `try_into()`)
- Manual `ORDER BY ... LIMIT 1` for current state (use `*_current` views)
- Per-row account lookups in loops (batch-fetch with HashMap)
- Blocking in async contexts, hardcoded config values
- Direct DB queries without SeaORM entities

## Security & Performance

- Validate external inputs, use parameterized queries (SeaORM), no sensitive data in logs, TLS in production
- Connection pooling, proper pagination, DataLoader for N+1, Zstandard compression (>1KB)

## Development Workflow

1. Make changes
2. `just quick` (fmt, clippy, check)
3. `just test` or specific tests
4. Commit

## Docker Images

Three variants: `docker-blokli` (production), `docker-blokli-dev` (development), `docker-blokli-profile` (with debug symbols).

```bash
nix build -L .#docker-blokli-x86_64-linux    # amd64
nix build -L .#docker-blokli-aarch64-linux   # arm64 (local only, CI disabled)
```

CI builds on every PR commit and merge. Trivy scans for vulnerabilities. Version formats: `version-commit.hash`, `version-pr.number`, `version` (release).

## Additional Resources

- [SeaORM](https://www.sea-ql.org/SeaORM/) · [async-graphql](https://async-graphql.github.io/async-graphql/) · [Axum](https://docs.rs/axum/) · [Alloy](https://alloy.rs/)

<!-- BEGIN BEADS INTEGRATION v:1 profile:minimal hash:ca08a54f -->
## Beads Issue Tracker

This project uses **bd (beads)** for issue tracking. Run `bd prime` to see full workflow context and commands.

### Quick Reference

```bash
bd ready              # Find available work
bd show <id>          # View issue details
bd update <id> --claim  # Claim work
bd close <id>         # Complete work
```

### Rules

- Use `bd` for ALL task tracking — do NOT use TodoWrite, TaskCreate, or markdown TODO lists
- Run `bd prime` for detailed command reference and session close protocol
- Use `bd remember` for persistent knowledge — do NOT use MEMORY.md files

## Session Completion

**When ending a work session**, you MUST complete ALL steps below. Work is NOT complete until `git push` succeeds.

**MANDATORY WORKFLOW:**

1. **File issues for remaining work** - Create issues for anything that needs follow-up
2. **Run quality gates** (if code changed) - Tests, linters, builds
3. **Update issue status** - Close finished work, update in-progress items
4. **PUSH TO REMOTE** - This is MANDATORY:
   ```bash
   git pull --rebase
   bd dolt push
   git push
   git status  # MUST show "up to date with origin"
   ```
5. **Clean up** - Clear stashes, prune remote branches
6. **Verify** - All changes committed AND pushed
7. **Hand off** - Provide context for next session

**CRITICAL RULES:**
- Work is NOT complete until `git push` succeeds
- NEVER stop before pushing - that leaves work stranded locally
- NEVER say "ready to push when you are" - YOU must push
- If push fails, resolve and retry until it succeeds
<!-- END BEADS INTEGRATION -->
