# Shows available commands
default:
    @just --list

# Check all code in workspace
check:
    cargo check --workspace

# Build the project with runtime-tokio feature
build:
    cargo build -F runtime-tokio

# Build in release mode
build-release:
    cargo build --release -F runtime-tokio

# Run all tests with runtime-tokio feature
test:
    cargo test -F runtime-tokio

# Run tests for a specific package
test-package package:
    cargo test -p {{package}} -F runtime-tokio

# Run tests in single thread mode (useful for debugging)
test-debug:
    cargo test -F runtime-tokio -- --test-threads=1 --nocapture

# Run local debug build against rotsee
run-local-rotsee:
    cargo run -p bloklid -- -c config.toml | tee rotsee.logs

# Clean build artifacts
clean:
    cargo clean

# Quick check - format, clippy, and check
quick: fmt clippy check

# Run bloklid daemon in release mode
run-release:
    cargo run --release --bin bloklid -F runtime-tokio

# Generate documentation
doc:
    cargo doc --no-deps -F runtime-tokio --open

# Generate documentation for all dependencies
doc-all:
    cargo doc -F runtime-tokio --open

# Build the project
build:
    cargo build --workspace

# Build in release mode
build-release:
    cargo build --workspace --release

# Run all tests
test:
    cargo test --workspace

# Run tests for a specific package
test-package package:
    cargo test -p {{package}}

# Run tests in single thread mode (useful for debugging)
test-debug:
    cargo test --workspace -- --test-threads=1 --nocapture

# Format code
fmt:
    nix fmt

# Run clippy lints
clippy:
    cargo clippy --workspace -- -D warnings

# Run clippy on all targets
clippy-all:
    cargo clippy --workspace --all-targets -- -D warnings

# Fix clippy warnings automatically
clippy-fix:
    cargo clippy --workspace --fix --allow-dirty --allow-staged

# Development build and test cycle
dev: fmt check test

# Watch for changes and run checks
watch:
    cargo watch -x "check --workspace" -x "test -p bloklid"

# Run bloklid daemon
run:
    cargo run --bin bloklid

# Run bloklid daemon with custom config
run-config config_path:
    cargo run --bin bloklid -- -c {{config_path}}

# Run blokli-api server
run-api:
    cargo run --bin blokli-api

# Run blokli-api server in release mode
run-api-release:
    cargo run --release --bin blokli-api

# Update dependencies
update:
    cargo update

# Show outdated dependencies
outdated:
    cargo outdated
