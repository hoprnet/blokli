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

### Import Organization

Group imports in this order:

1. Standard library (`std::`)
2. External crates (alphabetically)
3. Local crates and modules (alphabetically)

**DO NOT** use wildcard imports (`use module::*;`)

**Exception:** Migration files (`db/migration/src/*.rs`) may use wildcard imports for `sea_orm` and `sea_query` as these libraries are designed for this pattern in database migrations.

Always use workspace dependencies defined in the root `Cargo.toml`:

```toml
# In Cargo.toml of a workspace member
[dependencies]
tokio = { workspace = true }
async-graphql = { workspace = true }
```

Example:

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

## Design

### GraphQL API

- The target schema is defined in `design/target-api-schema.graphql`
- Ensure any changes in code are made in accordance with the schema
- The actual schema can be generated using `just export-schema-sqlite` and will be stored in `schema.graphql`
- Use async-graphql resolvers with proper error handling
- Implement DataLoader pattern for N+1 query prevention
- Support GraphQL subscriptions via Server-Sent Events (SSE)

### Database

- The target database schema is defined in `design/target-db-schema.mmd`
- Ensure any changes in code are made in accordance with the schema
- Database attribute names must match the target schema to minimize mapping code and avoid confusion
- Use SeaORM entities generated in `db/entity/src/codegen/`

### Configuration

- Use TOML configuration files
- Support configuration reload via SIGHUP signal
- Validate configuration at startup

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

- ❌ Wildcard imports (`use module::*;`)
- ❌ Unwrap/expect in production code (use proper error handling)
- ❌ Blocking operations in async contexts
- ❌ Hardcoded configuration values
- ❌ Direct database queries without using SeaORM entities
- ❌ Missing error context
- ❌ Undocumented public APIs

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

## Additional Resources

- [SeaORM Documentation](https://www.sea-ql.org/SeaORM/)
- [async-graphql Guide](https://async-graphql.github.io/async-graphql/)
- [Axum Documentation](https://docs.rs/axum/)
- [Alloy Documentation](https://alloy.rs/)
