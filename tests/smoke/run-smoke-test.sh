#!/usr/bin/env bash
# Smoke test script for blokli
# Tests that bloklid can start and become healthy with PostgreSQL and Anvil
#
# Usage:
#   ./run-smoke-test.sh                           # Build from nix, push to local registry
#   SOURCE_IMAGE=myimage:tag ./run-smoke-test.sh  # Pull image, push to local registry
#
# Environment variables:
#   SOURCE_IMAGE     - Image to pull from remote registry (optional, builds from nix if not set)
#   REGISTRY_HOST    - Local registry hostname (default: localhost)
#   REGISTRY_PORT    - Local registry port (default: 5000)
#
# Exit codes:
#   0 - All tests passed
#   1 - Tests failed

set -euo pipefail

# Configuration
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "${SCRIPT_DIR}/../.." && pwd)"
COMPOSE_FILE="${SCRIPT_DIR}/docker-compose.yml"

# Registry configuration with defaults
REGISTRY_HOST="${REGISTRY_HOST:-localhost}"
REGISTRY_PORT="${REGISTRY_PORT:-5000}"
LOCAL_REGISTRY="${REGISTRY_HOST}:${REGISTRY_PORT}"
LOCAL_IMAGE="${LOCAL_REGISTRY}/bloklid:smoke-test"

# Bloklid API configuration
BLOKLID_URL="http://localhost:3064"
TIMEOUT_SECONDS=120
POLL_INTERVAL=5

# Logging functions
log_info() {
    echo "[INFO] $1"
}

log_warn() {
    echo "[WARN] $1"
}

log_error() {
    echo "[ERROR] $1"
}

# Detect system architecture for nix build
detect_arch() {
    local os
    os=$(uname -s)

    # On macOS (Darwin), always use amd64 for now
    if [ "$os" = "Darwin" ]; then
        echo "amd64"
        return
    fi

    local arch
    arch=$(uname -m)
    case "$arch" in
        x86_64)
            echo "amd64"
            ;;
        aarch64|arm64)
            echo "aarch64"
            ;;
        *)
            log_error "Unsupported architecture: ${arch}"
            exit 1
            ;;
    esac
}

# Cleanup function
cleanup() {
    log_info "Cleaning up Docker Compose stack..."
    docker compose -f "${COMPOSE_FILE}" down -v --remove-orphans 2>/dev/null || true
}

# Register cleanup on exit
trap cleanup EXIT

# Check prerequisites
check_prerequisites() {
    log_info "Checking prerequisites..."

    if ! command -v docker &> /dev/null; then
        log_error "Docker is not installed"
        exit 1
    fi

    if ! command -v curl &> /dev/null; then
        log_error "curl is not installed"
        exit 1
    fi

    if ! command -v jq &> /dev/null; then
        log_error "jq is not installed"
        exit 1
    fi
}

# Start the local registry
start_registry() {
    log_info "Starting local Docker registry on port ${REGISTRY_PORT}..."

    # Export for docker-compose
    export REGISTRY_PORT

    # Start only the registry service
    docker compose -f "${COMPOSE_FILE}" up -d registry

    # Wait for registry to be healthy (via Docker healthcheck)
    local elapsed=0
    local max_wait=30
    while [ $elapsed -lt $max_wait ]; do
        local status
        status=$(docker inspect --format='{{.State.Health.Status}}' blokli-smoke-registry 2>/dev/null || echo "starting")

        if [ "$status" = "healthy" ]; then
            log_info "Local registry is ready"
            return 0
        fi

        log_info "Waiting for registry... status=${status} (${elapsed}s)"
        sleep 1
        elapsed=$((elapsed + 1))
    done

    log_error "Local registry failed to start within ${max_wait}s"
    docker logs blokli-smoke-registry
    exit 1
}

# Build or pull the source image and push to local registry
prepare_image() {
    local source_image

    if [ -n "${SOURCE_IMAGE:-}" ]; then
        # CI mode: pull from remote registry
        log_info "Pulling image from remote registry: ${SOURCE_IMAGE}"
        docker pull "${SOURCE_IMAGE}"
        source_image="${SOURCE_IMAGE}"
    else
        # Local dev mode: build from nix
        log_info "Building Docker image from nix..."

        # Check if nix is available
        if ! command -v nix &> /dev/null; then
            log_error "nix is not installed. Either install nix or set SOURCE_IMAGE environment variable."
            exit 1
        fi

        # Detect architecture
        local arch
        arch=$(detect_arch)
        log_info "Detected architecture: ${arch}"

        # Build the Docker image for the correct architecture
        (cd "${PROJECT_ROOT}" && nix build .#bloklid-docker-${arch})

        # Load the image
        docker load < "${PROJECT_ROOT}/result"
        source_image="bloklid:latest"

        log_info "Built and loaded image: ${source_image}"
    fi

    # Tag and push to local registry
    log_info "Pushing image to local registry: ${LOCAL_IMAGE}"
    docker tag "${source_image}" "${LOCAL_IMAGE}"
    docker push "${LOCAL_IMAGE}"

    log_info "Image ready in local registry"
}

# Start the Docker Compose stack (excluding registry which is already running)
start_stack() {
    log_info "Starting Docker Compose stack..."

    # Export environment variables for docker-compose
    export REGISTRY_PORT
    export BLOKLID_IMAGE="${LOCAL_IMAGE}"

    docker compose -f "${COMPOSE_FILE}" up -d

    log_info "Waiting for services to start..."
}

# Wait for bloklid to become healthy
wait_for_healthy() {
    log_info "Waiting for bloklid to become healthy (timeout: ${TIMEOUT_SECONDS}s)..."

    local elapsed=0
    local healthy=false

    while [ $elapsed -lt $TIMEOUT_SECONDS ]; do
        # Check healthz endpoint
        if curl -sf "${BLOKLID_URL}/healthz" > /dev/null 2>&1; then
            healthy=true
            break
        fi

        # Show container status
        local status
        status=$(docker inspect --format='{{.State.Health.Status}}' blokli-smoke-bloklid 2>/dev/null || echo "unknown")
        log_info "Container health status: ${status} (${elapsed}s elapsed)"

        sleep $POLL_INTERVAL
        elapsed=$((elapsed + POLL_INTERVAL))
    done

    if [ "$healthy" = false ]; then
        log_error "bloklid failed to become healthy within ${TIMEOUT_SECONDS}s"
        log_error "Container logs:"
        docker compose -f "${COMPOSE_FILE}" logs bloklid
        return 1
    fi

    log_info "bloklid is healthy after ${elapsed}s"
}

# Test healthz endpoint
test_healthz() {
    log_info "Testing /healthz endpoint..."

    local response
    response=$(curl -sf "${BLOKLID_URL}/healthz")

    local status
    status=$(echo "$response" | jq -r '.status')

    if [ "$status" != "healthy" ]; then
        log_error "healthz returned unexpected status: ${status}"
        log_error "Response: ${response}"
        return 1
    fi

    local version
    version=$(echo "$response" | jq -r '.version')
    log_info "healthz: status=${status}, version=${version}"

    return 0
}

# Test readyz endpoint
test_readyz() {
    log_info "Testing /readyz endpoint..."

    local response
    local http_code

    # Get both response body and HTTP status code
    response=$(curl -sf -w "\n%{http_code}" "${BLOKLID_URL}/readyz" || true)
    http_code=$(echo "$response" | tail -n1)
    response=$(echo "$response" | sed '$d')

    local status
    status=$(echo "$response" | jq -r '.status')

    # Extract check statuses
    local db_status rpc_status indexer_status
    db_status=$(echo "$response" | jq -r '.checks.database.status')
    rpc_status=$(echo "$response" | jq -r '.checks.rpc.status')
    indexer_status=$(echo "$response" | jq -r '.checks.indexer.status')

    log_info "readyz: status=${status}, http_code=${http_code}"
    log_info "  database: ${db_status}"
    log_info "  rpc: ${rpc_status}"
    log_info "  indexer: ${indexer_status}"

    # Show additional details
    local rpc_block
    rpc_block=$(echo "$response" | jq -r '.checks.rpc.block_number // "null"')
    log_info "  rpc_block_number: ${rpc_block}"

    local indexer_block indexer_lag
    indexer_block=$(echo "$response" | jq -r '.checks.indexer.last_indexed_block // "null"')
    indexer_lag=$(echo "$response" | jq -r '.checks.indexer.lag // "null"')
    log_info "  last_indexed_block: ${indexer_block}, lag: ${indexer_lag}"

    # Check for errors
    local db_error rpc_error indexer_error
    db_error=$(echo "$response" | jq -r '.checks.database.error // empty')
    rpc_error=$(echo "$response" | jq -r '.checks.rpc.error // empty')
    indexer_error=$(echo "$response" | jq -r '.checks.indexer.error // empty')

    if [ -n "$db_error" ]; then
        log_error "Database error: ${db_error}"
    fi
    if [ -n "$rpc_error" ]; then
        log_error "RPC error: ${rpc_error}"
    fi
    if [ -n "$indexer_error" ]; then
        log_warn "Indexer error: ${indexer_error}"
    fi

    # Validate results
    if [ "$db_status" != "healthy" ]; then
        log_error "Database check failed"
        return 1
    fi

    if [ "$rpc_status" != "healthy" ]; then
        log_error "RPC check failed"
        return 1
    fi

    # Indexer might not be fully synced yet, just warn
    if [ "$indexer_status" != "healthy" ]; then
        log_warn "Indexer is not fully healthy (this may be expected for fresh start)"
    fi

    return 0
}

# Test GraphQL endpoint
test_graphql() {
    log_info "Testing GraphQL endpoint..."

    local response
    response=$(curl -sf "${BLOKLID_URL}/graphql" \
        -H "Content-Type: application/json" \
        -d '{"query": "{ info { network chainId version } }"}')

    local network chain_id version
    network=$(echo "$response" | jq -r '.data.info.network // empty')
    chain_id=$(echo "$response" | jq -r '.data.info.chainId // empty')
    version=$(echo "$response" | jq -r '.data.info.version // empty')

    if [ -z "$network" ] || [ -z "$chain_id" ]; then
        log_error "GraphQL query failed"
        log_error "Response: ${response}"
        return 1
    fi

    log_info "GraphQL: network=${network}, chainId=${chain_id}, version=${version}"
    return 0
}

# Main test execution
main() {
    log_info "Starting blokli smoke tests"
    log_info "================================"
    log_info "Local registry: ${LOCAL_REGISTRY}"
    log_info "Local image: ${LOCAL_IMAGE}"
    if [ -n "${SOURCE_IMAGE:-}" ]; then
        log_info "Source image: ${SOURCE_IMAGE}"
    else
        log_info "Source: Building from nix"
    fi
    log_info "================================"

    check_prerequisites
    start_registry
    prepare_image
    start_stack

    # Wait for bloklid to become healthy
    if ! wait_for_healthy; then
        log_error "Smoke test FAILED: bloklid did not become healthy"
        exit 1
    fi

    # Run tests
    local tests_passed=true

    if ! test_healthz; then
        tests_passed=false
    fi

    if ! test_readyz; then
        tests_passed=false
    fi

    if ! test_graphql; then
        tests_passed=false
    fi

    # Summary
    log_info "================================"
    if [ "$tests_passed" = true ]; then
        log_info "Smoke test PASSED"
        exit 0
    else
        log_error "Smoke test FAILED"
        exit 1
    fi
}

# Run main
main "$@"
