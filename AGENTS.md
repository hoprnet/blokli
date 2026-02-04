# Agent Guidelines for HOPR Blokli

This repository contains Blokli: On-chain Indexer of HOPR smart contracts and on-chain operations provider.

## Project Overview

Blokli is a Rust workspace project with two main components:

- `bloklid` - Daemon for indexing HOPR on-chain events
- `blokli-api` - GraphQL API server for querying indexed data

## Technology Stack

- **Language**: Rust (edition 2021)
- **Async Runtime**: Tokio
- **Web Framework**: Axum with async-graphql
- **Database**: PostgreSQL (via SeaORM)
- **Build System**: Nix Flakes with `just` command runner
- **Blockchain**: Ethereum/Gnosis Chain via Alloy

## Quick Reference

**Post-Changes Check**: Always run `just quick` after making code changes to ensure:

- Code is properly formatted
- No clippy warnings
- Code compiles successfully

## Documentation Map

- `README.md`: High-level overview and quickstart; keep concise and link to deeper docs
- `TESTING.md`: Test strategy and commands
- `design/architecture.md`: Conceptual system architecture (no code, CLI commands, or config snippets)
- `design/target-api-schema.graphql`: Target GraphQL schema reference
- `design/target-db-schema.mmd`: Target database schema reference

## Build Commands

- `just build` - Build all workspace packages
- `just test` - Run all tests
- `just test-package <name>` - Run tests for specific package
- `just test-indexer` - Run integration tests
- `just test-debug` - Single-threaded test execution with output
- `just clippy` - Run linter
- `just fmt` - Format code (uses nix fmt)
- `just check` - Check compilation
- `just quick` - Run fmt, clippy, and check (recommended after changes)
- `just run` - Run bloklid daemon
- `just run-api` - Run blokli-api GraphQL server
- `just doc` - Generate documentation
- `just export-schema-sqlite` - Generate GraphQL schema to verify against target

**Note**: All `just` commands must be run within `nix develop` shell or the nix dev environment.

Legacy cargo commands (prefer using `just` instead):

- `cargo build -F runtime-tokio` - Build all workspace packages
- `cargo test -F runtime-tokio` - Run all tests
- `cargo test <test_name> -F runtime-tokio` - Run specific test
- `cargo test --package <package_name> -F runtime-tokio` - Run tests for specific package

The `runtime-tokio` feature flag is required and automatically included in `just` commands.

## Code Style Guidelines

### General Rust Conventions

- **Naming**: snake_case for functions/variables, PascalCase for types/enums
- **Types**: Prefer explicit types over type inference when it improves clarity
- **Formatting**: 4 spaces for indentation, no trailing commas in single-line constructs
- **Documentation**: Use `//!` for module docs, `///` for item docs
- Document public APIs comprehensively with examples when helpful

### Type Annotations

Always use explicit type hints for function parameters and return types to improve code clarity and enable better static analysis:

```rust
// Good: Explicit type annotations
pub async fn fetch_block(block_number: u64, db: &DatabaseConnection) -> Result<Block, IndexerError> {
    let block = Block::find_by_id(block_number)
        .one(db)
        .await?
        .ok_or(IndexerError::BlockNotFound(block_number))?;
    Ok(block)
}

// Bad: Missing return type (only acceptable for simple constructors)
pub async fn fetch_block(block_number: u64, db: &DatabaseConnection) {
    // ...
}
```

**When to use type inference:**

- Local variables where the type is obvious from context
- Iterator chains where intermediate types are complex
- Closures with clear context

**When to use explicit annotations:**

- All public function parameters and return types
- Function parameters in private functions (unless trivial)
- Struct field types (always required)
- Variables where the type isn't immediately clear from the initializer

### Import Organization

**IMPORTANT**: All imports must be declared at the top of the module, never inside functions, impl blocks, or other nested scopes.

Group imports in this order:

1. Standard library (`std::`)
2. External crates (alphabetically)
3. Local crates and modules (alphabetically)

**DO NOT** use wildcard imports (`use module::*;`)

**Exception:** Migration files (`db/migration/src/*.rs`) may use wildcard imports for `sea_orm` and `sea_query` as these libraries are designed for this pattern in database migrations.

**Avoid inline fully-qualified paths** - Use imports instead to keep code clean and readable:

```rust
// Bad: Inline fully-qualified paths
fn process_data() -> std::collections::HashMap<String, Vec<u8>> {
    let mut map = std::collections::HashMap::new();
    map
}

// Good: Use imports
use std::collections::HashMap;

fn process_data() -> HashMap<String, Vec<u8>> {
    let mut map = HashMap::new();
    map
}
```

Always use workspace dependencies defined in the root `Cargo.toml`:

```toml
# In Cargo.toml of a workspace member
[dependencies]
tokio = { workspace = true }
async-graphql = { workspace = true }
```

Example of properly organized imports:

```rust
use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use tokio::sync::RwLock;

use crate::config::Config;
use crate::indexer::BlockIndexer;
```

### Error Handling

- Use `thiserror::Error` for custom error types
- Always return `Result<T>` for fallible operations
- Use `anyhow::Result` for application-level errors
- Provide meaningful error messages with context

Example:

```rust
use thiserror::Error;

#[derive(Error, Debug)]
pub enum IndexerError {
    #[error("Block {0} not found")]
    BlockNotFound(u64),
    #[error("Database error: {0}")]
    Database(#[from] sea_orm::DbErr),
}
```

#### GraphQL API Error Management

The GraphQL API uses a **centralized error management system** for consistent, maintainable error handling across all resolvers, mutations, and subscriptions.

**Key Module:**

- `api/src/errors.rs` - All error handling in one place
  - Server-level errors (`ApiError` enum)
  - GraphQL error codes (`codes` submodule)
  - Error message templates (`messages` submodule)
  - Error builder functions

**Error Code Constants** (`errors::codes`):

All error codes are defined as constants in the `codes` module:

```rust
use crate::errors::codes;

// Available error codes:
codes::CONTEXT_ERROR        // Context retrieval failures
codes::QUERY_FAILED         // Database query failures
codes::NOT_FOUND           // Resource not found
codes::INVALID_ADDRESS     // Address validation failures
codes::CONVERSION_ERROR    // Type conversion failures
codes::OVERFLOW            // Numeric overflow
codes::RPC_ERROR          // Blockchain RPC failures
codes::VALIDATION_FAILED   // Transaction validation failures
// ... and more
```

**Error Builder Functions** (`errors`):

Always use error builder functions instead of inline error construction:

```rust
use crate::errors;

// ✅ Good: Use error builders
return AccountsResult::QueryFailed(errors::db_connection_error(e));
return AccountsResult::QueryFailed(errors::query_failed("fetch accounts", e));
return SafeResult::InvalidAddress(errors::invalid_address_error(address, e));

// ❌ Bad: Inline error construction
return AccountsResult::QueryFailed(QueryFailedError {
    code: "CONTEXT_ERROR".to_string(),
    message: format!("Failed to get database connection: {:?}", e),
});
```

**Common Error Builder Functions:**

```rust
// Context and database errors
errors::context_error(context_type, error)
errors::db_connection_error(error)
errors::query_failed(operation, error)

// Validation errors
errors::invalid_address_error(address, error)
errors::empty_address_error()
errors::validation_failed(reason)

// RPC errors
errors::rpc_error(operation, error)          // Returns RpcError
errors::rpc_query_failed(operation, error)   // Returns QueryFailedError

// Resource errors
errors::not_found(resource_type, identifier)
errors::missing_filter_error(filter_name, context)

// Conversion errors
errors::conversion_error(from_type, to_type, value)
errors::overflow_error(operation, value)

// Domain-specific errors
errors::channel_not_found(channel_id)
errors::invalid_pagination(reason)
errors::ticket_params_incomplete()
errors::network_status_unavailable()
```

**Error Message Templates** (`errors::messages`):

For custom error scenarios, use message templates directly:

```rust
use crate::errors::messages;

// Create custom error with consistent formatting
let error_message = messages::query_error("custom operation", error);
let error_message = messages::invalid_address(address, error);
```

**Best Practices:**

1. **Never create errors inline** - Always use error builder functions
2. **Never hardcode error codes** - Use `errors::codes::*` constants
3. **Provide context** - Pass descriptive operation names and error details
4. **Use appropriate builders** - Choose the builder that matches the error type needed
5. **Add new builders when needed** - If a pattern repeats, create a new builder function

**Example: Complete Resolver with Error Handling:**

```rust
use crate::errors;

async fn accounts(
    &self,
    ctx: &Context<'_>,
    keyid: Option<i64>,
) -> AccountsResult {
    // Validate required filters
    if keyid.is_none() {
        return AccountsResult::MissingFilter(
            errors::missing_filter_error("keyid", "accounts query")
        );
    }

    // Get database connection
    let db = match ctx.data::<DatabaseConnection>() {
        Ok(db) => db,
        Err(e) => {
            return AccountsResult::QueryFailed(
                errors::db_connection_error(format!("{:?}", e))
            );
        }
    };

    // Fetch accounts
    let accounts = match fetch_accounts(db, keyid).await {
        Ok(accounts) => accounts,
        Err(e) => {
            return AccountsResult::QueryFailed(
                errors::query_failed("fetch accounts", e)
            );
        }
    };

    AccountsResult::Accounts(AccountsList { accounts })
}
```

**Benefits:**

- **Single source of truth** - All error handling in one `errors.rs` module
- **Consistency** - Uniform error format across the entire API
- **Maintainability** - Change messages globally in one location
- **Testability** - Easy to verify error codes without hardcoded strings
- **Type safety** - Compiler ensures correct error structure
- **Clean imports** - Just `use crate::errors` for everything

### Documentation

Use standard Rust documentation patterns with examples:

```rust
/// Indexes blockchain events for a specific block range.
///
/// # Arguments
///
/// * `from_block` - Starting block number (inclusive)
/// * `to_block` - Ending block number (inclusive)
///
/// # Returns
///
/// Returns the number of events indexed.
///
/// # Errors
///
/// Returns `IndexerError::BlockNotFound` if any block in the range doesn't exist.
pub async fn index_range(from_block: u64, to_block: u64) -> Result<usize, IndexerError> {
    // Implementation
}
```

### Async Code

- Use `async/await` syntax
- Prefer `tokio::spawn` for concurrent tasks
- Use `Arc<RwLock<T>>` for shared mutable state
- Avoid blocking operations in async contexts

### HOPR Foundation Types

The HOPR project provides three foundational crates with well-tested types that should be used whenever possible instead of creating new types from scratch. These crates are defined in the workspace `Cargo.toml` and are available to all workspace members.

#### hopr-primitive-types

Provides foundational blockchain types and conversion utilities:

**Core Types:**

- `Address` - Ethereum/blockchain addresses (20 bytes)
- `Balance<C>` - Generic balance type with currency marker
- `HoprBalance` - HOPR token balance type alias
- `XDaiBalance` - xDai native token balance type alias
- `U256` - 256-bit unsigned integer for Solidity interoperability
- `SerializableLog` - Blockchain event log representation

**Traits:**

- `ToHex` - Convert types to hexadecimal string representation
- `IntoEndian` - Handle endianness conversions for serialization

**Usage Example:**

```rust
use hopr_primitive_types::prelude::{Address, HoprBalance, ToHex};

pub fn format_account(address: Address, balance: HoprBalance) -> String {
    format!("Account {} has balance {}", address.to_hex(), balance)
}
```

#### hopr-crypto-types

Provides cryptographic primitives and key management:

**Core Types:**

- `Hash` - Cryptographic hash type (32 bytes)
- `OffchainPublicKey` - Ed25519 public key for off-chain operations
- `OffchainSignature` - Ed25519 signature
- `ChainKeypair` - ECDSA keypair for on-chain operations
- `OffchainKeypair` - Ed25519 keypair for off-chain operations

**Usage Example:**

```rust
use hopr_crypto_types::prelude::{Hash, OffchainPublicKey};

pub fn verify_announcement(
    peer_id: OffchainPublicKey,
    hash: Hash,
) -> Result<(), ValidationError> {
    // Verification logic using cryptographic types
    Ok(())
}
```

#### hopr-internal-types

Provides HOPR protocol-specific structures and abstractions:

**Channel Types:**

- `ChannelEntry` - Complete channel state representation
- `ChannelStatus` - Channel lifecycle status (Open, PendingToClose, Closed)
- `ChannelDirection` - Channel direction (Incoming/Outgoing)
- `CorruptedChannelEntry` - Tracking for corrupted channel state

**Account Types:**

- `AccountEntry` - Account state with type classification
- `AccountType` - Account classification (Announcer, NotAnnounced, etc.)

**Protocol Types:**

- `AcknowledgedTicket` - Payment ticket with acknowledgment
- `WinningProbability` - Probability calculations for ticket validation
- `KeyBinding` - Key binding verification structure

**Usage Example:**

```rust
use hopr_internal_types::channels::{ChannelEntry, ChannelStatus};
use hopr_primitive_types::prelude::Address;

pub async fn process_channel(
    channel: ChannelEntry,
    source: Address,
) -> Result<(), ProcessingError> {
    match channel.status {
        ChannelStatus::Open => {
            // Process open channel
        }
        ChannelStatus::PendingToClose { closure_time } => {
            // Handle pending closure
        }
        ChannelStatus::Closed => {
            // Handle closed channel
        }
    }
    Ok(())
}
```

#### hopr-bindings

Provides smart contract bindings and encoding/decoding utilities for HOPR on-chain contracts:

**Purpose:**

For any contract encoding/decoding or event encoding/decoding work, use the existing `hopr-bindings` crate and its modules. This crate contains generated bindings for HOPR smart contracts and provides type-safe interfaces for interacting with blockchain events and contract calls.

**Getting Started:**

To explore the full API and available contract bindings, generate the crate documentation:

```bash
# Build and open docs for hopr-bindings
cargo doc --package hopr-bindings --open
```

The generated documentation will show all available contract modules, event types, and encoding/decoding utilities.

#### Building Documentation

To explore the full API of these crates, build their documentation:

```bash
# Build and open docs for primitive types
cargo doc --package hopr-primitive-types --open

# Build and open docs for crypto types
cargo doc --package hopr-crypto-types --open

# Build and open docs for internal types
cargo doc --package hopr-internal-types --open

# Build and open docs for contract bindings
cargo doc --package hopr-bindings --open
```

#### Best Practices

1. **Always prefer HOPR types** - Before creating a new type for addresses, balances, channels, or accounts, check if a suitable type exists in these crates
2. **Use the prelude modules** - Import commonly used types from `hopr_primitive_types::prelude` and `hopr_crypto_types::prelude`
3. **Leverage conversion traits** - Use `ToHex`, `IntoEndian`, and other provided traits for consistent serialization
4. **Implement conversions** - When mapping between database models and HOPR types, implement `From`/`TryFrom` traits (see `db/entity/src/conversions/`)

## Design

### GraphQL API

- The target schema is defined in `design/target-api-schema.graphql`
- Ensure any changes in code are made in accordance with the schema
- The actual schema can be generated using `just export-schema-sqlite` and will be stored in `schema.graphql`
- Use async-graphql resolvers with proper error handling
- Implement DataLoader pattern for N+1 query prevention
- Support GraphQL subscriptions via Server-Sent Events (SSE) with keep-alive events enabled by default

#### GraphQL Context Type Safety

**CRITICAL RULE:** All GraphQL schema context data MUST use wrapper types (newtypes) for scalar values to prevent type ambiguity. Never register raw primitives (`u64`, `String`, `u16`, etc.) directly in schema context.

**Why:** Multiple resolvers accessing context could retrieve the wrong value if multiple instances of the same primitive type exist. Wrapper types provide compile-time safety.

**Pattern:**

```rust
// ❌ BAD - Raw primitives (ambiguous)
.data(chain_id: u64)
.data(network: String)

// ✅ GOOD - Wrapper types (unambiguous)
.data(ChainId(chain_id))
.data(NetworkName(network))
```

**Existing wrapper types in `api/src/schema.rs`:**

- `ChainId(u64)` - Blockchain chain ID
- `NetworkName(String)` - Network name identifier
- `ExpectedBlockTime(u64)` - Expected block time in seconds
- `Finality(u16)` - Number of block confirmations required

When adding new context data, always create a wrapper type first.

### Database Notifications

Blokli uses database-native notification mechanisms for real-time GraphQL subscriptions:

**PostgreSQL (Production)**:

- Database triggers automatically send NOTIFY when data changes
- API subscriptions use LISTEN to receive notifications
- Zero polling overhead - fully event-driven
- Supports horizontal scaling (multiple API instances can LISTEN to same channel)
- Example: `ticket_params_updated` channel notifies when ticket price or winning probability changes

**SQLite (Tests/Development)**:

- Falls back to polling with 1-second interval
- Used in test environments via `BlokliDb::new_in_memory()`
- Simplified implementation for fast test execution

**Implementation Pattern**:

When adding new subscriptions that need real-time updates:

1. Create PostgreSQL trigger in a new migration (see `m017_add_ticket_params_notify_trigger.rs`)
2. Create notification stream abstraction in `api/src/notifications.rs`
3. Use the stream in your subscription resolver
4. Test with SQLite polling in integration tests

**Key Files**:

- `db/migration/src/m017_add_ticket_params_notify_trigger.rs` - PostgreSQL trigger example
- `api/src/notifications.rs` - Notification stream abstraction
- `db/src/notifications.rs` - SQLite notification manager (foundation for future hook integration)

### Database

- The target database schema is defined in `design/target-db-schema.mmd`
- Ensure any changes in code are made in accordance with the schema
- Database attribute names must match the target schema to minimize mapping code and avoid confusion
- Use SeaORM entities generated in `db/entity/src/codegen/`

### Configuration

- Use TOML configuration files
- Support configuration reload via SIGHUP signal
- Validate configuration at startup

#### Configuration Files

The project maintains two key configuration files:

- `bloklid/src/config.rs` - Rust configuration struct definitions with defaults and validation
- `bloklid/example-config.toml` - Example configuration file showing all available options

**IMPORTANT**: These files must be kept in sync. The example configuration serves as both user documentation and a working example of all available configuration options.

#### Agent Responsibilities for Configuration Changes

**When modifying configuration code (`bloklid/src/config.rs`)**, you MUST update the example configuration file:

1. **Adding New Configuration Options**:
   - Add the new option to `bloklid/example-config.toml` with appropriate comments
   - Include the default value from the code (check the `#[default(...)]` attribute)
   - Document what the option does and when to use it
   - If it's a complex option, provide usage examples in comments

2. **Removing Configuration Options**:
   - Remove the option from `bloklid/example-config.toml`
   - Ensure any related documentation comments are also removed

3. **Changing Default Values**:
   - Update the default value in `bloklid/example-config.toml`
   - Update any comments that reference the default value

4. **Adding New Configuration Sections**:
   - Add the entire section to `bloklid/example-config.toml`
   - Include all fields in the section with their defaults
   - Add a descriptive header comment explaining the section's purpose
   - Group related options together logically

5. **Changing Configuration Structure**:
   - If moving options between sections, update `bloklid/example-config.toml` accordingly
   - If renaming fields, update all references in the example file
   - Ensure the TOML structure matches the serde attributes in the Rust code

#### Configuration Documentation Requirements

When adding or modifying configuration options, ensure:

- **Defaults are visible**: Every option in the example config should show its default value
- **Purpose is clear**: Comments should explain what the option controls and why you might change it
- **Type is obvious**: The value format should make the expected type clear (string, number, boolean, etc.)
- **Validation rules**: If there are constraints (e.g., "must be > 0"), document them
- **Related options**: If options affect each other, note this in comments
- **Database type handling**: Remember that `protocols` section is auto-generated and should NOT be in the example config

#### Example Config Best Practices

- Use inline comments for brief explanations: `max_connections = 10  # Maximum database connections`
- Use block comments for complex options or sections that need more explanation
- Show commented-out alternatives to demonstrate different configuration approaches
- Keep the example config focused on user-configurable options only
- Exclude options marked with `#[serde(skip)]` in the Rust code
- Document units (e.g., "seconds", "milliseconds", "bytes") in comments
- For enum types, show all valid values in comments

#### Verification Checklist

Before committing configuration changes, verify:

- [ ] All fields in `Config`, `IndexerConfig`, `ApiConfig`, `SubscriptionConfig`, `DatabaseConfig` structs are represented in `example-config.toml` (except `#[serde(skip)]` fields)
- [ ] Default values in the example config match the `#[default(...)]` attributes in the code
- [ ] All sections and subsections are properly documented with comments
- [ ] The TOML file is valid (can be parsed by a TOML parser)
- [ ] Database configuration shows both PostgreSQL and SQLite examples
- [ ] No internal-only options (like `protocols`) are included in the example

## Architecture

### Architecture Documentation

**IMPORTANT**: The complete system architecture is documented in `design/architecture.md`. This document provides:

- High-level component architecture and interactions
- Data flow patterns and event processing pipelines
- User flows through the GraphQL API
- Deployment architectures and scaling considerations
- Performance characteristics and optimization strategies
- Security considerations and error handling patterns

**Agent Responsibilities**:

1. **Read the Architecture Document**: Before making significant changes to the system, read `design/architecture.md` to understand how components interact and the design principles behind the current architecture.

2. **Update Architecture Documentation**: When making changes that affect the architecture, update `design/architecture.md` to reflect the new design. This includes:
   - Adding new components or services
   - Changing component interactions or data flows
   - Modifying database schema or queries patterns
   - Altering API endpoints or GraphQL schema structure
   - Changing deployment models or configuration options
   - Introducing new architectural patterns or design decisions

3. **Maintain Consistency**: Ensure that architectural changes are reflected consistently across:
   - Code implementation
   - Architecture documentation (`design/architecture.md`)
   - API schema documentation (`design/target-api-schema.graphql`)
   - Database schema documentation (`design/target-db-schema.mmd`)

**Note**: The architecture document focuses on high-level design and should NOT contain code examples, CLI commands, or configuration snippets. Keep it conceptual and architectural.

### Workspace Structure

- `bloklid` - Daemon for indexing HOPR on-chain events
  - Signal handling for config reload (SIGHUP) and shutdown (SIGINT/SIGTERM)
  - Configuration via TOML files
  - Uses tokio async runtime

- `blokli-api` - GraphQL API server
  - Built with Axum and async-graphql
  - HTTP/2 support
  - TLS 1.3 support (when configured)
  - Zstandard compression for responses >1KB
  - Server-Sent Events (SSE) for GraphQL subscriptions
  - GraphQL Playground for development

- `db/` - Database modules
  - Database abstraction via traits in `db/api`
  - SeaORM entities in `db/entity`
  - Migrations in `db/migration`

## Testing

- Write unit tests in the same file using `#[cfg(test)]`
- Use `#[tokio::test]` for async tests
- Mock external dependencies (blockchain RPC, database)
- Test error paths as well as happy paths
- All tests use tokio async runtime

Example:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_index_block() {
        // Test implementation
    }
}
```

### Integration Tests (`tests/integration`)

The `tests/integration` crate provides end-to-end tests that exercise the full Blokli stack against a real blockchain (Anvil) and real database (PostgreSQL), running inside Docker containers.

#### When to Use Integration Tests

Write integration tests when:

- Testing transaction submission, tracking, or confirmation flows end-to-end
- Verifying GraphQL query or subscription behavior against real indexed data
- Testing Safe deployment, registration, announcement, or channel operations
- Validating that bloklid correctly indexes on-chain events and exposes them via the API
- Testing interactions between the client library and the running bloklid server

Do **not** use integration tests for:

- Unit-level logic (event parsing, data conversions, DB queries) — use `#[cfg(test)]` unit tests instead
- Testing code that doesn't need a running blockchain or bloklid instance
- Fast iteration during development — integration tests require Docker and are slower

#### Running Integration Tests

```bash
# Build the Docker image first (required)
nix build .#docker-blokli-dev

# Run integration tests
just test-indexer
```

The `just test-indexer` command builds the test crate and runs all integration tests. Docker must be available on the host machine.

#### Docker Stack Architecture

The integration tests spin up a Docker Compose stack with four services:

| Service    | Container Name                | Purpose                                                |
| ---------- | ----------------------------- | ------------------------------------------------------ |
| `anvil`    | `blokli-integration-anvil`    | Local Ethereum node (Foundry Anvil) with 1s block time |
| `postgres` | `blokli-integration-postgres` | PostgreSQL database for bloklid                        |
| `bloklid`  | `blokli-integration-bloklid`  | The bloklid daemon under test                          |
| `registry` | `blokli-integration-registry` | Local Docker registry for image distribution           |

The stack is defined in `tests/integration/docker-compose.yml`. All services share a `integration-test` bridge network. The bloklid service depends on all others being healthy before starting.

#### Configuration

Test configuration is managed via environment variables (see `tests/integration/src/config.rs`):

| Environment Variable            | Default                         | Description                     |
| ------------------------------- | ------------------------------- | ------------------------------- |
| `BLOKLI_TEST_IMAGE`             | `bloklid:integration-test`      | Docker image to test            |
| `BLOKLI_TEST_CONFIG`            | `config-integration-anvil.toml` | bloklid config file             |
| `BLOKLI_TEST_BLOKLID_URL`       | `http://localhost:8081`         | bloklid GraphQL API URL         |
| `BLOKLI_TEST_RPC_URL`           | `http://localhost:8546`         | Anvil JSON-RPC URL              |
| `BLOKLI_TEST_REMOTE_IMAGE`      | (unset)                         | Pull image from remote registry |
| `BLOKLI_TEST_HTTP_TIMEOUT_SECS` | `30`                            | HTTP client timeout             |
| `BLOKLI_TEST_CONFIRMATIONS`     | `1`                             | Transaction confirmations       |
| `BLOKLI_TEST_REGISTRY_PORT`     | `5001`                          | Local registry port             |

#### Fixture Lifecycle

All integration tests share a single `IntegrationFixture` instance (via `OnceCell`). The fixture is lazily initialized on the first test that needs it:

1. Loads `TestConfig` from environment variables
2. Starts the Docker Compose stack (`docker compose up -d`)
3. Waits for the stack to stabilize (8 seconds)
4. Parses Anvil accounts from container logs
5. Creates `RpcClient` (direct Anvil JSON-RPC) and `BlokliClient` (bloklid GraphQL API)
6. Deploys HOPR smart contracts to Anvil via the deployer account (account 0)
7. Mints and distributes HOPR tokens to all test accounts
8. Registers a `libc::atexit` shutdown hook to tear down the stack on process exit

On teardown, the `DockerEnvironment` collects container logs to `/tmp/blokli-integration/` before running `docker compose down`.

#### Writing a New Integration Test

Use the `rstest` fixture pattern with `#[serial]` to ensure tests run sequentially (they share the same blockchain state):

```rust
use std::time::Duration;

use anyhow::Result;
use blokli_client::api::{BlokliQueryClient, BlokliTransactionClient, types::TransactionStatus};
use blokli_integration_tests::fixtures::{IntegrationFixture, integration_fixture as fixture};
use rstest::*;
use serial_test::serial;

#[rstest]
#[test_log::test(tokio::test)]
#[serial]
async fn test_my_feature(#[future(awt)] fixture: IntegrationFixture) -> Result<()> {
    // 1. Get test accounts
    let [sender, recipient] = fixture.sample_accounts::<2>();

    // 2. Build and submit transactions using fixture helpers
    let tx_value = U256::from(1_000_000u128);
    let nonce = fixture.rpc().transaction_count(&sender.address).await?;
    let raw_tx = fixture.build_raw_tx(tx_value, sender, recipient, nonce).await?;
    let signed_bytes = Vec::from_hex(raw_tx.trim_start_matches("0x"))?;

    // 3. Submit via bloklid client
    let txid = fixture.submit_and_track_tx(&signed_bytes).await?;

    // 4. Track and assert
    let res = fixture.client().track_transaction(txid, Duration::from_secs(30)).await?;
    assert_eq!(res.status, TransactionStatus::Confirmed);

    Ok(())
}
```

**Key attributes:**

- `#[rstest]` — enables the `fixture` parameter injection
- `#[test_log::test(tokio::test)]` — async test with tracing output
- `#[serial]` — prevents concurrent execution (shared blockchain state)
- `#[future(awt)]` — automatically `.await`s the async fixture

#### `IntegrationFixture` API

**Accessors:**

| Method                   | Returns              | Description                         |
| ------------------------ | -------------------- | ----------------------------------- |
| `config()`               | `&TestConfig`        | Test configuration                  |
| `accounts()`             | `&[AnvilAccount]`    | All available Anvil accounts        |
| `sample_accounts::<N>()` | `[&AnvilAccount; N]` | Randomly sample N accounts          |
| `client()`               | `&BlokliClient`      | Blokli GraphQL client (via bloklid) |
| `rpc()`                  | `&RpcClient`         | Direct Anvil JSON-RPC client        |
| `contract_addresses()`   | `&ContractAddresses` | Deployed HOPR contract addresses    |

**Transaction helpers:**

| Method                                               | Description                                  |
| ---------------------------------------------------- | -------------------------------------------- |
| `build_raw_tx(value, sender, recipient, nonce)`      | Build an EIP-1559 raw transaction hex string |
| `submit_tx(signed_bytes)`                            | Submit a transaction without tracking        |
| `submit_and_track_tx(signed_bytes)`                  | Submit and get a tracking ID                 |
| `submit_and_confirm_tx(signed_bytes, confirmations)` | Submit and wait for N confirmations          |

**Safe helpers:**

| Method                                    | Description                                     |
| ----------------------------------------- | ----------------------------------------------- |
| `deploy_or_get_safe(owner, amount)`       | Deploy a Safe or retrieve existing one          |
| `deploy_safe_and_announce(owner, amount)` | Deploy Safe, register, and announce the account |
| `register_safe(owner, safe_address)`      | Register a Safe by node                         |

**Channel helpers:**

| Method                                                | Description                       |
| ----------------------------------------------------- | --------------------------------- |
| `open_channel(from, to, amount, module)`              | Open a payment channel            |
| `initiate_outgoing_channel_closure(from, to, module)` | Start closing an outgoing channel |

**Account helpers:**

| Method                                     | Description                            |
| ------------------------------------------ | -------------------------------------- |
| `announce_account(account, module)`        | Announce an account on-chain           |
| `announce_or_get_account(account, module)` | Announce only if not already announced |

#### `RpcClient` — Direct Blockchain Access

For operations that bypass bloklid and go directly to Anvil:

| Method                        | Description                           |
| ----------------------------- | ------------------------------------- |
| `chain_id()`                  | Get the chain ID                      |
| `transaction_count(address)`  | Get nonce for an address              |
| `get_balance(address)`        | Get native token balance              |
| `execute_transaction(raw_tx)` | Send raw transaction directly via RPC |

#### Test File Organization

Tests are organized by client trait:

| File                                  | Covers                                                                                      |
| ------------------------------------- | ------------------------------------------------------------------------------------------- |
| `tests/blokli_query_client.rs`        | `BlokliQueryClient` — queries for accounts, channels, safes, chain info                     |
| `tests/blokli_subscription_client.rs` | `BlokliSubscriptionClient` — SSE subscriptions for real-time updates                        |
| `tests/blokli_transaction_client.rs`  | `BlokliTransactionClient` — transaction submission, tracking, confirmation, Safe enrichment |

When adding new tests, place them in the file corresponding to the client trait being tested. If the test spans multiple traits, put it in the file for the primary trait under test.

#### Safe Module Transactions in Tests

To test HOPR operations that go through a Safe module (the standard HOPR transaction path), use `SafePayloadGenerator` from `hopr-chain-connector`:

```rust
use hopr_chain_connector::{PayloadGenerator, SafePayloadGenerator};
use hopr_chain_types::prelude::SignableTransaction;
use hopr_primitive_types::prelude::Address as HoprAddress;

// Deploy a Safe and announce the owner
let safe = fixture.deploy_safe_and_announce(owner, parsed_safe_balance()).await?;

// Build a module transaction payload
let payload_generator = SafePayloadGenerator::new(
    &owner.keypair,
    *fixture.contract_addresses(),
    HoprAddress::from_str(&safe.module_address)?,
);
let payload = payload_generator.approve(spender, amount)?;
let payload_bytes = payload
    .sign_and_encode_to_eip2718(nonce, fixture.rpc().chain_id().await?, None, &owner.keypair)
    .await?;

// Submit and track
let txid = fixture.submit_and_track_tx(&payload_bytes).await?;
```

Note: `PayloadGenerator` and `SignableTransaction` traits must be in scope for `.approve()`, `.fund_channel()`, `.initiate_outgoing_channel_closure()`, and `.sign_and_encode_to_eip2718()` methods.

#### Debugging Failing Integration Tests

- Container logs are saved to `/tmp/blokli-integration/<timestamp>/` on teardown
- Use `RUST_LOG=debug` or `RUST_LOG=trace` when running tests for verbose output
- The `#[test_log::test(...)]` attribute enables tracing output in test runs
- If the Docker stack fails to start, check that ports 8081, 8546, and 5001 are not in use

## Common Patterns

### Signal Handling

```rust
use async_signal::{Signal, Signals};

let mut signals = Signals::new([Signal::Int, Signal::Term, Signal::Hup])?;
while let Some(signal) = signals.next().await {
    match signal {
        Signal::Hup => reload_config().await?,
        Signal::Int | Signal::Term => break,
        _ => {}
    }
}
```

### Database Queries

```rust
use sea_orm::{EntityTrait, QueryFilter, ColumnTrait};
use db_entity::codegen::prelude::*;

let blocks = Block::find()
    .filter(block::Column::Number.gte(from_block))
    .filter(block::Column::Number.lte(to_block))
    .all(&db)
    .await?;
```

### GraphQL Resolvers

```rust
use async_graphql::{Context, Object, Result};

#[Object]
impl Query {
    async fn block(&self, ctx: &Context<'_>, number: u64) -> Result<Block> {
        let db = ctx.data::<DatabaseConnection>()?;
        Block::find_by_id(number)
            .one(db)
            .await?
            .ok_or_else(|| "Block not found".into())
    }
}
```

## What to Avoid

- Wildcard imports (`use module::*;`)
- Imports inside functions, impl blocks, or other nested scopes
- Inline fully-qualified type paths instead of imports (e.g., `std::collections::HashMap::new()`)
- Creating custom types when HOPR foundation types exist (`hopr_primitive_types`, `hopr_crypto_types`, `hopr_internal_types`)
- Creating custom contract encoding/decoding logic when `hopr-bindings` provides the necessary types and utilities
- Missing type annotations on public function parameters and return types
- Unwrap/expect in production code (use proper error handling)
- Blocking operations in async contexts
- Hardcoded configuration values
- Direct database queries without using SeaORM entities
- Missing error context
- Undocumented public APIs

## Security Considerations

- Validate all external inputs
- Use parameterized queries (SeaORM handles this)
- Don't log sensitive information
- Handle rate limiting for external API calls
- Use TLS for production deployments

## Performance

- Use connection pooling for database
- Implement proper pagination for large result sets
- Use DataLoader for GraphQL to prevent N+1 queries
- Cache frequently accessed data when appropriate
- Use streaming for large responses (Zstandard compression enabled for >1KB)

## Development Workflow

1. Make code changes
2. Run `just quick` to format, lint, and check compilation
3. Run `just test` or specific tests as needed
4. If all checks pass, commit your changes

## Docker Images

**Current Limitation:** ARM64 (aarch64) builds are temporarily disabled in CI due to GitHub runner limitations. Only AMD64 images are currently built and deployed via CI. The multi-arch manifest structure is maintained for easy re-enablement.

Local builds for all architectures remain fully functional:

- `nix build .#docker-blokli-x86_64-linux` - Build `linux-amd64` docker image
- `nix build .#docker-blokli-aarch64-linux` - Build `linux-arm64` docker image
- `nix run .#docker-blokli-upload-x86_64-linux` - Build and publishes `linux-amd64` docker image

### Overview

The project uses Nix with nix-lib helpers to build reproducible Docker images for the `bloklid` daemon. Images are built for multiple architectures, scanned for security vulnerabilities, and pushed to Google Artifact Registry with automatic multi-arch manifest creation.

### Supported Architectures

- **amd64** (x86_64-linux) - Intel/AMD 64-bit processors ✅ Available in CI
- **arm64** (aarch64-linux) - ARM 64-bit processors (AWS Graviton, Apple Silicon servers) 🔄 Local builds only

### Image Variants

Three image variants are available for each architecture:

1. **docker-blokli** - Production build (release profile, optimized)
2. **docker-blokli-dev** - Development build (dev profile, faster compilation)
3. **docker-blokli-profile** - Profiling build (with debug symbols)

### CI/CD Workflows

#### Automated Docker Builds

Docker images are automatically built in GitHub Actions for:

- **Pull Requests** - Commit-tagged images (e.g., `1.0.0-commit.abc123`)
- **Merged PRs** - PR-tagged images (e.g., `1.0.0-pr.42`)
- **Releases** - Version-tagged images (e.g., `1.0.0`)

The build workflow follows a three-stage process:

##### Stage 1: Build Images

- Builds Docker image for amd64 architecture (arm64 disabled until runner supports it)
- Uses Nix to ensure reproducible builds
- Stores built image as artifact

##### Stage 2: Upload Manifest (Sequential)

- Uses nix-lib multi-arch helper to build OCI manifest
- Uploads platform-specific image with suffix (`-linux-amd64`)
- Creates and pushes manifest list (currently single-platform)

##### Stage 3: Security Scanning

- Runs Trivy vulnerability scan for amd64
- Generates SBOM in SPDX and CycloneDX formats
- Performs smoke test on uploaded image
- Uploads results to GitHub Security

#### Multi-Architecture Manifest

The CI/CD system creates Docker manifests for platform selection (currently amd64 only):

```bash
# Pulls the image (currently amd64 only)
docker pull <registry>/bloklid:1.0.0

# Explicitly pull amd64 image
docker pull <registry>/bloklid:1.0.0-linux-amd64
```

### Security Scanning

All Docker images undergo automated security scanning in CI using the official Trivy GitHub Action.

#### Local Security Scanning

For local security scanning, use Trivy directly:

```bash
# Install Trivy (if not already installed)
# macOS: brew install trivy
# Linux: See https://aquasecurity.github.io/trivy/latest/getting-started/installation/

# Scan a local Docker image
trivy image --severity HIGH,CRITICAL <image-name>:<tag>

# Generate SARIF report
trivy image --format sarif --output trivy-results.sarif <image-name>:<tag>

# Generate SBOM
trivy image --format cyclonedx --output sbom.json <image-name>:<tag>
```

#### CI Security Workflow

Automated security scanning in CI using `aquasecurity/trivy-action`:

- **Trivy Vulnerability Scanner**
  - Uses official Trivy GitHub Action
  - Scans for HIGH and CRITICAL severity vulnerabilities
  - Uploads results to GitHub Security tab (SARIF format)
  - Fails build if critical vulnerabilities are found in production images
  - Automatic vulnerability database updates

- **Smoke Tests**
  - Verifies container starts successfully
  - Tests entrypoint functionality with `--help` flag
  - Ensures binary is functional inside container

- **SBOM Generation**
  - Uses Trivy GitHub Action
  - Generates CycloneDX JSON format
  - Stored as workflow artifacts (90-day retention)
  - Available for supply chain security analysis

### Workflow Files

- `.github/workflows/build-docker.yaml` - Multi-arch Docker build with integrated security scanning and SBOM generation
- `.github/workflows/build.yaml` - PR validation (Docker + code quality checks)
- `.github/workflows/merge.yaml` - Post-merge Docker builds
- `.github/workflows/release.yaml` - Release workflow with Docker builds

### Image Tagging Strategy

| Version Type | Format                | Platform Image                    | Manifest              | Use Case              |
| ------------ | --------------------- | --------------------------------- | --------------------- | --------------------- |
| Commit       | `version-commit.hash` | `1.0.0-commit.abc123-linux-amd64` | `1.0.0-commit.abc123` | Development testing   |
| PR           | `version-pr.number`   | `1.0.0-pr.42-linux-amd64`         | `1.0.0-pr.42`         | Pre-merge validation  |
| Release      | `version`             | `1.0.0-linux-amd64`               | `1.0.0`               | Production deployment |

**Note:** Currently only AMD64 images are available. ARM64 will be re-enabled when GitHub runner supports aarch64. The manifest tag (without architecture suffix) points to the amd64 image.

## Additional Resources

- [SeaORM Documentation](https://www.sea-ql.org/SeaORM/)
- [async-graphql Guide](https://async-graphql.github.io/async-graphql/)
- [Axum Documentation](https://docs.rs/axum/)
- [Alloy Documentation](https://alloy.rs/)
