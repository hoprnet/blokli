# Agent Guidelines for HOPR Blokli

## Quick Reference

**Post-Changes Check**: Always run `just quick` after making code changes to ensure:

- Code is properly formatted
- No clippy warnings
- Code compiles successfully

## Build Commands

- `just build` - Build all workspace packages
- `just test` - Run all tests
- `just test-package <name>` - Run tests for specific package
- `just clippy` - Run linter
- `just fmt` - Format code (uses nix fmt)
- `just check` - Check compilation
- `just quick` - Run fmt, clippy, and check (recommended after changes)
- `just run` - Run bloklid daemon
- `just run-api` - Run blokli-api GraphQL server
- `just doc` - Generate documentation

## Code Style

- **Imports**: Use workspace dependencies, group std/external/local imports separately, do not use wildcard imports
- **Error Handling**: Use `thiserror::Error` for custom errors, return `Result<T>`
- **Naming**: snake_case for functions/variables, PascalCase for types/enums
- **Documentation**: Use `//!` for module docs, `///` for item docs
- **Formatting**: Use `cargo fmt` - 4 spaces, no trailing commas in single-line
- **Types**: Prefer explicit types, use `prelude` modules for common imports

## Design

### GraphQL API

- The target schema is defined in `design/target-api-schema.graphql`
- Ensure any changes in code are made in accordance with the schema
- The actual schema can be generated using `just export-schema-sqlite` and will be stored in `schema.graphql`. It can be used to verify the generated schema matches the target schema.

### Database

- The target database schema is defined in `design/target-db-schema.mmd`
- Ensure any changes in code are made in accordance with the schema
- The database code and internal entities should use the same attribute names as
  defined in the database target schema to minimize mapping code and avoid
  confusion.

## Architecture

### Workspace Structure

- `bloklid` - Daemon for indexing HOPR on-chain events

  - Signal handling for config reload (SIGHUP) and shutdown (SIGINT/SIGTERM)
  - Configuration via YAML files
  - Uses tokio async runtime

- `blokli-api` - GraphQL API server

  - Built with Axum and async-graphql
  - HTTP/2 support
  - TLS 1.3 support (when configured)
  - Zstandard compression for responses >1KB
  - Server-Sent Events (SSE) for GraphQL subscriptions
  - GraphQL Playground for development

- `db/` - Database modules (future: api, entity, migration, sql)
  - Database abstraction via traits in `db/api`

## Testing

- Tests in modules with `#[cfg(test)]`
- Use `just test-package <package_name>` for specific package tests
- Use `just test-indexer` to run integration tests
- Use `just test-debug` for single-threaded test execution with output
- `just test-package <name>` - Run tests for specific package

Legacy cargo commands (prefer using `just` instead):

- `cargo build -F runtime-tokio` - Build all workspace packages
- `cargo test -F runtime-tokio` - Run all tests
- `cargo test <test_name> -F runtime-tokio` - Run specific test
- `cargo test --package <package_name> -F runtime-tokio` - Run tests for specific package

- All tests use tokio async runtime

## Development Workflow

1. Make code changes
2. Run `just quick` to format, lint, and check compilation
3. Run `just test` or specific tests as needed
4. If all checks pass, commit your changes

**Note**: All `just` commands must be run within `nix develop` shell or the nix dev environment.
