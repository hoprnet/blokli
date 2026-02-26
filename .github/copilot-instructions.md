# GitHub Copilot Instructions for HOPR Blokli

For comprehensive project guidelines, code style, architecture patterns, and development workflows, see
[AGENTS.md](../AGENTS.md) in the repository root.

## Key Points

- **Build System**: Use `just` commands within `nix develop` shell
- **Post-Changes**: Always run `just quick` after making code changes
- **No Wildcards**: Never use wildcard imports (`use module::*;`)
- **Error Handling**: Use `thiserror::Error` for custom errors, return `Result<T>`
- **Documentation**: Document all public APIs with examples
- **Testing**: Use `#[tokio::test]` for async tests

Refer to AGENTS.md for detailed examples, patterns, and best practices.
