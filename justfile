# ============================================================================
# Default Command
# ============================================================================

# Show available commands
default:
    @just --list

# ============================================================================
# Quick Workflows
# ============================================================================

# Quick check - format, clippy, and check
quick: fmt clippy check

# Development build and test cycle - format, check, and test
dev: fmt check test

# Watch for changes and run checks continuously
watch:
    cargo watch -x "check --workspace" -x "test -p bloklid"

# ============================================================================
# Build Commands
# ============================================================================

# Build all workspace packages in debug mode
build:
    cargo build --workspace

# Build all workspace packages in release mode with full optimizations
build-release:
    cargo build --workspace --release

# Check all workspace code without building binaries
check:
    cargo check --workspace

# Clean all build artifacts
clean:
    cargo clean

# ============================================================================
# Test Commands
# ============================================================================

# Run all tests in workspace
test:
    cargo test --workspace

# Run tests for a specific package
test-package package:
    cargo test -p {{package}}

# Run tests in single thread mode with output (useful for debugging)
test-debug:
    cargo test --workspace -- --test-threads=1 --nocapture

# ============================================================================
# Code Quality
# ============================================================================

# Format all code with nix formatter
fmt:
    nix fmt

# Run clippy lints with warnings as errors
clippy:
    cargo clippy --workspace -- -D warnings

# Run clippy on all targets (lib, bin, tests, benches, examples)
clippy-all:
    cargo clippy --workspace --all-targets -- -D warnings

# Automatically fix clippy warnings
clippy-fix:
    cargo clippy --workspace --fix --allow-dirty --allow-staged

# ============================================================================
# Run Commands - bloklid daemon
# ============================================================================

# Run bloklid daemon in debug mode with default configuration
run:
    cargo run --bin bloklid

# Run bloklid daemon with custom configuration file
run-config config_path:
    cargo run --bin bloklid -- -c {{config_path}}

# Run bloklid daemon in release mode with full optimizations
run-release:
    cargo run --release --bin bloklid -F runtime-tokio

# Run bloklid in debug mode against rotsee network with log output
run-local-rotsee:
    cargo run -p bloklid -- -c config.toml | tee rotsee.logs

# Run bloklid daemon with SQLite database (local testing)
run-sqlite:
    cargo run --bin bloklid -- -c config-sqlite.toml

# Clean SQLite database files (removes both index and logs databases)
clean-sqlite:
    rm -f data/bloklid-index.db data/bloklid-logs.db
    @echo "SQLite databases removed"

# ============================================================================
# Run Commands - blokli-api server
# ============================================================================

# Run blokli-api server in debug mode
run-api:
    cargo run --bin blokli-api

# Run blokli-api server in release mode with full optimizations
run-api-release:
    cargo run --release --bin blokli-api

# ============================================================================
# Documentation
# ============================================================================

# Generate and open documentation for workspace packages only
doc:
    cargo doc --no-deps -F runtime-tokio --open

# Generate and open documentation including all dependencies
doc-all:
    cargo doc -F runtime-tokio --open

# ============================================================================
# Dependency Management
# ============================================================================

# Update all dependencies to latest compatible versions
update:
    cargo update

# Show outdated dependencies that have newer versions available
outdated:
    cargo outdated

# Check for unused dependencies
cargo-udeps:
    nix develop .#experiment -c bash -c 'cargo udeps'

# ============================================================================
# Docker Compose Commands
# ============================================================================

# Start PostgreSQL and bloklid with Docker Compose
docker-up:
    docker-compose up -d

# Stop Docker Compose services
docker-down:
    docker-compose down

# Stop and remove all data (WARNING: destroys database)
docker-down-volumes:
    docker-compose down -v

# View Docker Compose logs (optionally specify service name)
docker-logs service="":
    #!/usr/bin/env bash
    if [ -z "{{service}}" ]; then
        docker-compose logs -f
    else
        docker-compose logs -f {{service}}
    fi

# Build bloklid Docker image from Nix
docker-build:
    nix build .#bloklid-docker
    docker load < result

# Rebuild bloklid and restart Docker Compose
docker-restart: docker-build
    docker-compose down
    docker-compose up -d

# Connect to PostgreSQL database with psql
docker-db:
    docker-compose exec postgres psql -U bloklid -d bloklid

# Reset database (WARNING: deletes all data)
docker-db-reset:
    docker-compose down -v
    docker-compose up -d postgres
    @echo "Waiting for PostgreSQL to be ready..."
    @sleep 5
    docker-compose up -d bloklid

# Show database tables
docker-db-tables:
    docker-compose exec postgres psql -U bloklid -d bloklid -c "\\dt"

# Backup database to file
docker-db-backup file="backup.sql":
    docker-compose exec -T postgres pg_dump -U bloklid bloklid > {{file}}
    @echo "Database backed up to {{file}}"

# Restore database from file
docker-db-restore file="backup.sql":
    docker-compose exec -T postgres psql -U bloklid -d bloklid < {{file}}
    @echo "Database restored from {{file}}"
