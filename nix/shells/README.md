# Nix Shell Configuration

This directory contains the unified shell configuration for the blokli project.

## Overview

A single comprehensive development environment (`default.nix`) that includes all tools needed for development, testing, CI/CD, and documentation workflows.

## Usage

```bash
nix develop
```

This provides a complete environment with:

### Core Development Tools
- Full Rust toolchain from `rust-toolchain.toml`
- Pre-commit hooks for code quality
- Build tools (cargo, just, make)
- Core utilities (bash, curl, jq, yq, gnuplot)

### Database & Storage
- SQLite for local database testing

### Python Environment
- Python 3.13 runtime
- uv package manager
- Automatic virtual environment setup

### Documentation Tools
- HTML validation (html-tidy)
- Universal document converter (pandoc)
- Rust documentation generation

### CI/CD Tools
- GitHub Actions local runner (act)
- GitHub CLI (gh)
- Google Cloud SDK
- Container tools (skopeo, dive)
- API tools (swagger-codegen3, vacuum-go)
- Security analysis (zizmor, cargo-audit)
- Package building (nfpm)

### Formatting & Quality
- Rust formatting (rustfmt, clippy)
- Tree formatter integration
- Pre-commit hook validation

## Design Philosophy

**One Shell to Rule Them All**

Instead of maintaining multiple profile-based shells (dev, ci, test, docs), we provide a single comprehensive environment. This approach:

- **Simplifies workflow**: No need to remember which shell to use
- **Ensures consistency**: Everyone uses the same tools
- **Reduces maintenance**: Single file to update
- **Improves discoverability**: All tools available immediately
- **Eliminates confusion**: No profile selection needed

The marginal cost of including additional tools is minimal compared to the cognitive overhead of managing multiple environments.

## Architecture

### Evolution

**Before**: 6 separate shell files with complex inheritance
```
base.nix ← test.nix ← dev.nix ← docs.nix
                    └─ ci-test.nix
      └─ ci.nix
```

**After**: Single unified shell
```
default.nix (all tools included)
```

### Benefits

- **Simplicity**: One shell definition, one command
- **No configuration needed**: Works out of the box
- **Fast setup**: All tools available immediately
- **Predictable**: Same environment everywhere
- **Easy to extend**: Add tools once for everyone

## Shell Hook

The shell automatically:
1. Syncs Python dependencies with `uv sync --frozen`
2. Unsets `SOURCE_DATE_EPOCH` for build compatibility
3. Auto-patches ELF binaries on Linux
4. Runs pre-commit hooks installation

## Environment Variables

- `LD_LIBRARY_PATH`: Set to include OpenSSL and curl libraries
- `CARGO_BUILD_RUSTFLAGS`: Configured for fast linking (mold on Linux, lld on macOS)

## Adding New Tools

To add tools to the environment:

1. Edit `nix/shells/default.nix`
2. Add the package to the `allPackages` list with a comment
3. Group with related tools (e.g., add database tools near sqlite)

Example:
```nix
# Database
sqlite
postgresql  # Add this line
```
