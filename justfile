# Shows available commands
default:
    @just --list

# Check all code in workspace
check:
    cargo check --workspace -F runtime-tokio

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

# Format code
fmt:
    nix fmt

# Run clippy lints
clippy:
    cargo clippy -F runtime-tokio -- -D warnings

# Run clippy on all targets
clippy-all:
    cargo clippy --all-targets -F runtime-tokio -- -D warnings

# Fix clippy warnings automatically
clippy-fix:
    cargo clippy --fix -F runtime-tokio --allow-dirty --allow-staged

# Quick check - format, clippy, and check
quick: fmt clippy check

# Development build and test cycle
dev: fmt check test

# Watch for changes and run checks
watch:
    cargo watch -x "check -F runtime-tokio" -x "test -p bloklid -F runtime-tokio"

# Run bloklid daemon
run:
    cargo run --bin bloklid -F runtime-tokio

# Run bloklid daemon with custom config
run-config config_path:
    cargo run --bin bloklid -F runtime-tokio -- -c {{config_path}}

# Run bloklid daemon in release mode
run-release:
    cargo run --release --bin bloklid -F runtime-tokio

# Generate documentation
doc:
    cargo doc --no-deps -F runtime-tokio --open

# Generate documentation for all dependencies
doc-all:
    cargo doc -F runtime-tokio --open

# Update dependencies
update:
    cargo update

# Show outdated dependencies
outdated:
    cargo outdated
