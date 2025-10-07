# Agent Guidelines for HOPR Blokli

## Quick Reference

**Post-Changes Check**: Always run `just quick` after making code changes to ensure:

- Code is properly formatted
- No clippy warnings
- Code compiles successfully

## Build Commands

- `just build` - Build with runtime-tokio feature
- `just test` - Run all tests
- `just test-package <name>` - Run tests for specific package
- `just clippy` - Run linter
- `just fmt` - Format code

Legacy cargo commands (prefer using `just` instead):

- `cargo build -F runtime-tokio` - Build all workspace packages
- `cargo test -F runtime-tokio` - Run all tests
- `cargo test <test_name> -F runtime-tokio` - Run specific test
- `cargo test --package <package_name> -F runtime-tokio` - Run tests for specific package

## Code Style

- **Imports**: Use workspace dependencies, group std/external/local imports separately, do not use wildcard imports
- **Error Handling**: Use `thiserror::Error` for custom errors, return `Result<T>`
- **Naming**: snake_case for functions/variables, PascalCase for types/enums
- **Documentation**: Use `//!` for module docs, `///` for item docs
- **Formatting**: Use `cargo fmt` - 4 spaces, no trailing commas in single-line
- **Types**: Prefer explicit types, use `prelude` modules for common imports

## Architecture

- Workspace with `bloklid` daemon and `db/` modules (api, entity, migration, sql)
- Async/await with tokio runtime
- Database abstraction via traits in `db/api`
- Signal handling for config reload (SIGHUP) and shutdown (SIGINT/SIGTERM)

## Testing

- Tests in modules with `#[cfg(test)]`
- Use `just test-package <package_name>` for specific package tests
- `just test-indexer` - Run indexer integration tests
- `just test-package <name>` - Run tests for specific package

Legacy cargo commands (prefer using `just` instead):

- `cargo build -F runtime-tokio` - Build all workspace packages
- `cargo test -F runtime-tokio` - Run all tests
- `cargo test <test_name> -F runtime-tokio` - Run specific test
- `cargo test --package <package_name> -F runtime-tokio` - Run tests for specific package

## Development Workflow

1. Make code changes
2. Run `just quick` to format, lint, and check compilation
3. Run `just test` or specific tests as needed
4. If all checks pass, commit your changes

**Note**: The `runtime-tokio` feature flag is required for most operations and is automatically included in all `just` commands.
