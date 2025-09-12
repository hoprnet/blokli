# Agent Guidelines for HOPR Blokli

## Build Commands
- `cargo build` - Build all workspace packages
- `cargo test` - Run all tests
- `cargo test <test_name>` - Run specific test
- `cargo clippy` - Run linter
- `cargo fmt` - Format code

## Code Style
- **Imports**: Use workspace dependencies, group std/external/local imports separately
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
- Use `cargo test --package <package_name>` for specific package tests