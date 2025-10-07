# Claude Instructions for HOPR Blokli

Always refer to @AGENTS.md for project-specific guidelines including:

- Build commands and preferred tooling (`just` vs `cargo`)
- Code style and formatting requirements
- Architecture patterns and conventions
- Testing workflows and commands
- Development workflow best practices

## Key Reminders

- **Always run `just quick`** after making code changes for formatting, linting, and compilation checks
- Use `just` commands instead of direct `cargo` commands when available
- The `runtime-tokio` feature flag is required and automatically included in `just` commands
- Follow the workspace structure with proper import organization
