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
    cargo test --workspace --exclude blokli-integration-tests --no-fail-fast

# Run tests for a specific package
test-package package:
    cargo test -p {{ package }} --no-fail-fast

# Run tests in single thread mode with output (useful for debugging)
test-debug:
    cargo test --workspace -- --test-threads=1 --nocapture

# Run all tests in workspace using nextest
nextest:
    cargo nextest run --workspace --exclude blokli-integration-tests

# Run tests for a specific package using nextest
nextest-package package:
    cargo nextest run -p {{ package }}

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

# Run ast-grep to check import organization and code style rules
lint-imports:
    ast-grep scan

# ============================================================================
# Run Commands - bloklid daemon
# ============================================================================

# Run bloklid daemon in debug mode with default configuration
run:
    cargo run --bin bloklid

# Run bloklid daemon with custom configuration file
run-config config_path:
    cargo run --bin bloklid -- -c {{ config_path }}

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

# Export GraphQL schema to file (requires database URL)
export-schema database_url output="schema.graphql":
    cargo run --bin blokli-api -- export-schema -d {{ database_url }} -o {{ output }}

# Export GraphQL schema using SQLite database
export-schema-sqlite output="schema.graphql":
    #!/usr/bin/env bash
    set -euo pipefail
    # Create data directory if it doesn't exist
    mkdir -p data
    # Run migrations to create/update the database schema
    # Note: ?mode=rwc allows SQLite to create the database file if it doesn't exist
    cargo run --bin migration -- up -u "sqlite://data/bloklid-index.db?mode=rwc"
    # Export the GraphQL schema
    cargo run --bin blokli-api -- export-schema -d "sqlite://data/bloklid-index.db" -o {{ output }}
    echo "GraphQL schema exported to {{ output }}"

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
    docker-compose -f docker/docker-compose.yml up -d

# Stop Docker Compose services
docker-down:
    docker-compose -f docker/docker-compose.yml down

# Stop and remove all data (WARNING: destroys database)
docker-down-volumes:
    docker-compose -f docker/docker-compose.yml down -v

# View Docker Compose logs (optionally specify service name)
docker-logs service="":
    #!/usr/bin/env bash
    if [ -z "{{ service }}" ]; then
        docker-compose -f docker/docker-compose.yml logs -f
    else
        docker-compose -f docker/docker-compose.yml logs -f {{ service }}
    fi

# Build bloklid Docker image from Nix
docker-build:
    nix build .#bloklid-docker
    docker load < result

# Build bloklid + anvil Docker image from Nix
docker-build-anvil:
    nix build .#bloklid-anvil-docker-amd64
    docker load < result

# Run bloklid + anvil container (GraphQL API on port 8080)
docker-run-anvil log_level="info":
    docker run --rm -p 8080:8080 -e RUST_LOG={{ log_level }} bloklid-anvil:latest

# Rebuild bloklid and restart Docker Compose
docker-restart: docker-build
    docker-compose -f docker/docker-compose.yml down
    docker-compose -f docker/docker-compose.yml up -d

# Connect to PostgreSQL database with psql
docker-db:
    docker-compose -f docker/docker-compose.yml exec postgres psql -U bloklid -d bloklid

# Reset database (WARNING: deletes all data)
docker-db-reset:
    docker-compose -f docker/docker-compose.yml down -v
    docker-compose -f docker/docker-compose.yml up -d postgres
    @echo "Waiting for PostgreSQL to be ready..."
    @sleep 5
    docker-compose -f docker/docker-compose.yml up -d bloklid

# Show database tables
docker-db-tables:
    docker-compose -f docker/docker-compose.yml exec postgres psql -U bloklid -d bloklid -c "\\dt"

# Backup database to file
docker-db-backup file="backup.sql":
    docker-compose -f docker/docker-compose.yml exec -T postgres pg_dump -U bloklid bloklid > {{ file }}
    @echo "Database backed up to {{ file }}"

# Restore database from file
docker-db-restore file="backup.sql":
    docker-compose -f docker/docker-compose.yml exec -T postgres psql -U bloklid -d bloklid < {{ file }}
    @echo "Database restored from {{ file }}"

# Generate SVG for target db schema
generate-target-db-schema-svg:
    docker run --rm -u $(id -u):$(id -g) -v ./design:/data \
      ghcr.io/mermaid-js/mermaid-cli/mermaid-cli \
      -i target-db-schema.mmd -o target-db-schema.svg
    @echo "SVG stored at ./design/target-db-schema.svg"

# ============================================================================
# Helm Chart Commands
# ============================================================================

# Template Helm chart to YAML files
helm-template:
    helm template blokli ./charts/blokli -f ./charts/blokli/values-testing.yaml

# Lint Helm chart for issues
helm-lint:
    helm lint ./charts/blokli -f ./charts/blokli/values-testing.yaml

helm-docs:
    $HOME/.npm-global/bin/readme-generator  --values ./charts/blokli/values.yaml --readme ./charts/blokli/README.md --schema "/tmp/schema.json" 

# Package Helm chart for distribution
helm-package:
    helm package ./charts/blokli

helm-login:
    #!/usr/bin/env bash
    token=$(gcloud auth print-access-token)
    helm registry login -u oauth2accesstoken --password "$token" https://europe-west3-docker.pkg.dev

helm-push:
    #!/usr/bin/env bash
    set -euo pipefail
    version=$(yq -r '.version' ./charts/blokli/Chart.yaml)
    helm push "blokli-${version}.tgz" oci://europe-west3-docker.pkg.dev/hoprassociation/helm-charts

# ============================================================================
# Smoke Tests
# ============================================================================

# Run smoke tests with Docker Compose (PostgreSQL + Anvil) - includes checkpoint resume validation
smoke-test:
    ./tests/smoke/run-smoke-test.sh

# Run smoke tests against Gnosis Chain RPC (allows high lag) - includes checkpoint resume validation
smoke-test-gnosis:
    SMOKE_CONFIG=config-smoke-gnosis.toml ./tests/smoke/run-smoke-test.sh

# Run smoke tests against Gnosis Chain RPC with full sync (requires indexer to catch up within 10 blocks) - includes checkpoint resume validation
smoke-test-gnosis-full-sync:
    SMOKE_CONFIG=config-smoke-gnosis-full-sync.toml ./tests/smoke/run-smoke-test.sh

# Build Docker image and run all smoke tests (Anvil + Gnosis Chain connectivity + Gnosis Chain full sync) - all include checkpoint resume validation
smoke-test-full: smoke-test smoke-test-gnosis smoke-test-gnosis-full-sync
