#!/usr/bin/env bash
# Smoke test script for blokli
# Tests that bloklid can start and become healthy with PostgreSQL and RPC
#
# Usage:
#   ./run-smoke-test.sh                           # Test with local Anvil (fast, no external deps)
#   SMOKE_CONFIG=config-smoke-gnosis.toml ./run-smoke-test.sh  # Test with Gnosis Chain RPC (allows high lag)
#   SMOKE_CONFIG=config-smoke-gnosis-full-sync.toml ./run-smoke-test.sh  # Test with Gnosis Chain RPC (requires full sync within 10 blocks)
#   SOURCE_IMAGE=myimage:tag ./run-smoke-test.sh  # Pull image, push to local registry
#
# Environment variables:
#   SMOKE_CONFIG     - Config file to use (default: config-smoke.toml)
#                      - config-smoke.toml: Local Anvil testing (fast, 30s timeout)
#                      - config-smoke-gnosis.toml: Real Gnosis Chain RPC testing (allows high lag, 30s timeout)
#                      - config-smoke-gnosis-full-sync.toml: Real Gnosis Chain RPC testing (requires full sync within 10 blocks, 600s timeout)
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
BLOKLID_URL="http://localhost:8080"
TIMEOUT_SECONDS="${TIMEOUT_SECONDS:-30}"
POLL_INTERVAL=5

# Retry configuration
HEALTHZ_MAX_RETRIES=5
HEALTHZ_RETRY_INTERVAL=1
READYZ_MAX_RETRIES=5
READYZ_RETRY_INTERVAL=1
REGISTRY_TIMEOUT=30
REGISTRY_POLL_INTERVAL=1
CHAIN_SYNC_TIMEOUT=30      # Default timeout, overridden for full sync test
CHAIN_SYNC_POLL_INTERVAL=2 # Default poll interval, overridden for full sync test
MAX_INDEXER_LAG=10

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

# JSON parsing helper - Extract a single field from JSON response
# Usage: extract_json_field "$response" '.path.to.field' 'default_value'
extract_json_field() {
  local response="$1"
  local jq_path="$2"
  local default="${3:-}"

  if [ -z "$response" ]; then
    echo "$default"
    return
  fi

  local result
  result=$(echo "$response" | jq -r "$jq_path" 2>/dev/null || echo "$default")

  if [ "$result" = "null" ] || [ -z "$result" ]; then
    echo "$default"
  else
    echo "$result"
  fi
}

# Unified retry function with timeout and interval
# Usage: retry_with_timeout <timeout_seconds> <interval_seconds> <description> <validation_command>
# Returns: 0 on success, 1 on timeout/failure
# The validation_command should return 0 on success, non-zero on failure
retry_with_timeout() {
  local timeout="$1"
  local interval="$2"
  local description="$3"
  local validation_cmd="$4"

  local elapsed=0
  local attempt=1

  log_info "${description} (timeout: ${timeout}s, interval: ${interval}s)"

  while [ $elapsed -lt "$timeout" ]; do
    log_info "  Attempt ${attempt} (elapsed: ${elapsed}s)"

    if eval "$validation_cmd"; then
      log_info "  ${description} succeeded after ${elapsed}s (${attempt} attempts)"
      return 0
    fi

    sleep "$interval"
    elapsed=$((elapsed + interval))
    attempt=$((attempt + 1))
  done

  log_error "${description} failed after ${timeout}s (${attempt} attempts)"
  return 1
}

# Retry function with max attempts (count-based instead of time-based)
# Usage: retry_with_attempts <max_retries> <interval_seconds> <description> <validation_command>
# Returns: 0 on success, 1 on max retries exceeded
retry_with_attempts() {
  local max_retries="$1"
  local interval="$2"
  local description="$3"
  local validation_cmd="$4"

  local retry=0

  log_info "${description} (max retries: ${max_retries}, interval: ${interval}s)"

  while [ $retry -lt "$max_retries" ]; do
    log_info "  Attempt $((retry + 1))/${max_retries}"

    if eval "$validation_cmd"; then
      log_info "  ${description} succeeded after $((retry + 1)) attempts"
      return 0
    fi

    retry=$((retry + 1))
    if [ $retry -lt "$max_retries" ]; then
      sleep "$interval"
    fi
  done

  log_error "${description} failed after ${max_retries} attempts"
  return 1
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
  aarch64 | arm64)
    echo "aarch64"
    ;;
  *)
    log_error "Unsupported architecture: ${arch}"
    exit 1
    ;;
  esac
}

# Collect blokli container logs for inspection
collect_blokli_logs() {
  local config_name
  config_name=$(basename "${SMOKE_CONFIG:-config-smoke.toml}" .toml)
  local timestamp
  timestamp=$(date +%Y%m%d_%H%M%S)
  local log_file="${SCRIPT_DIR}/blokli-smoke-${config_name}-${timestamp}.log"

  # Collect logs from the blokli container
  if docker logs blokli-smoke-bloklid >"${log_file}" 2>&1; then
    log_info "Blokli logs saved to: ${log_file}"
  else
    log_warn "Failed to collect blokli logs"
  fi
}

# Cleanup function
cleanup() {
  log_info "Collecting blokli container logs..."
  collect_blokli_logs

  log_info "Cleaning up Docker Compose stack..."
  docker compose -f "${COMPOSE_FILE}" down -v --remove-orphans 2>/dev/null || true
}

# Register cleanup on exit
trap cleanup EXIT

# Check prerequisites
check_prerequisites() {
  log_info "Checking prerequisites..."

  if ! command -v docker &>/dev/null; then
    log_error "Docker is not installed"
    exit 1
  fi

  if ! command -v curl &>/dev/null; then
    log_error "curl is not installed"
    exit 1
  fi

  if ! command -v jq &>/dev/null; then
    log_error "jq is not installed"
    exit 1
  fi
}

# Check if registry is healthy
check_registry_health() {
  local status
  status=$(docker inspect --format='{{.State.Health.Status}}' blokli-smoke-registry 2>/dev/null || echo "starting")
  log_info "    Registry status: ${status}"
  [ "$status" = "healthy" ]
}

# Start the local registry
start_registry() {
  log_info "Starting local Docker registry on port ${REGISTRY_PORT}..."

  # Export for docker-compose
  export REGISTRY_PORT

  # Start only the registry service
  docker compose -f "${COMPOSE_FILE}" up -d registry

  # Wait for registry to be healthy (via Docker healthcheck)
  if ! retry_with_timeout "$REGISTRY_TIMEOUT" "$REGISTRY_POLL_INTERVAL" "Waiting for local registry to be ready" "check_registry_health"; then
    log_error "Dumping registry logs:"
    docker logs blokli-smoke-registry
    exit 1
  fi
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
    if ! command -v nix &>/dev/null; then
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
    docker load <"${PROJECT_ROOT}/result"
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
  export SMOKE_CONFIG="${SMOKE_CONFIG:-config-smoke.toml}"

  docker compose -f "${COMPOSE_FILE}" up -d

  log_info "Waiting for services to start..."
}

# Check if bloklid is ready (not just alive)
check_bloklid_health() {
  local readyz_response
  readyz_response=$(curl -sf "${BLOKLID_URL}/readyz" 2>&1) || return 1

  # Show container status for debugging
  local container_status
  container_status=$(docker inspect --format='{{.State.Health.Status}}' blokli-smoke-bloklid 2>/dev/null || echo "unknown")
  log_info "    Container status: ${container_status}"

  # Extract and validate status from readyz response
  local status
  status=$(extract_json_field "$readyz_response" '.status' '')
  log_info "    Readiness status: ${status}"

  [ "$status" = "ready" ]
}

# Wait for bloklid to become healthy
wait_for_healthy() {
  if ! retry_with_timeout "$CHAIN_SYNC_TIMEOUT" "$CHAIN_SYNC_POLL_INTERVAL" "Waiting for bloklid to become healthy" "check_bloklid_health"; then
    log_error "Dumping bloklid logs:"
    docker compose -f "${COMPOSE_FILE}" logs bloklid
    return 1
  fi

  return 0
}

# Global variable to store healthz response
HEALTHZ_RESPONSE=""

# Check healthz endpoint
check_healthz() {
  HEALTHZ_RESPONSE=$(curl -sf "${BLOKLID_URL}/healthz" 2>/dev/null || echo "")
  local status
  status=$(extract_json_field "$HEALTHZ_RESPONSE" '.status' '')

  log_info "    Status: ${status}"
  [ "$status" = "healthy" ]
}

# Test healthz endpoint
test_healthz() {
  log_info "Testing /healthz endpoint..."

  if ! retry_with_attempts "$HEALTHZ_MAX_RETRIES" "$HEALTHZ_RETRY_INTERVAL" "Checking /healthz endpoint" "check_healthz"; then
    log_error "healthz returned unexpected status"
    log_error "Response: ${HEALTHZ_RESPONSE}"
    return 1
  fi

  local version
  version=$(extract_json_field "$HEALTHZ_RESPONSE" '.version' 'unknown')
  log_info "healthz: status=healthy, version=${version}"

  return 0
}

# Global variables to store readyz response
READYZ_RESPONSE=""
READYZ_HTTP_CODE=""

# Check readyz endpoint
check_readyz() {
  local full_response
  full_response=$(curl -sf -w "\n%{http_code}" "${BLOKLID_URL}/readyz" 2>/dev/null || echo "")

  if [ -n "$full_response" ]; then
    READYZ_HTTP_CODE=$(echo "$full_response" | tail -n1)
    READYZ_RESPONSE=$(echo "$full_response" | sed '$d')
    local status
    status=$(extract_json_field "$READYZ_RESPONSE" '.status' '')

    log_info "    Status: ${status}, HTTP code: ${READYZ_HTTP_CODE}"
    [ -n "$status" ] && [ "$status" != "null" ]
  else
    return 1
  fi
}

# Test readyz endpoint
test_readyz() {
  log_info "Testing /readyz endpoint..."

  if ! retry_with_attempts "$READYZ_MAX_RETRIES" "$READYZ_RETRY_INTERVAL" "Checking /readyz endpoint" "check_readyz"; then
    log_error "readyz returned no valid response"
    log_error "Response: ${READYZ_RESPONSE}"
    return 1
  fi

  # Extract check statuses using helper function
  local status db_status rpc_status indexer_status
  status=$(extract_json_field "$READYZ_RESPONSE" '.status' 'unknown')
  db_status=$(extract_json_field "$READYZ_RESPONSE" '.checks.database.status' 'unknown')
  rpc_status=$(extract_json_field "$READYZ_RESPONSE" '.checks.rpc.status' 'unknown')
  indexer_status=$(extract_json_field "$READYZ_RESPONSE" '.checks.indexer.status' 'unknown')

  log_info "readyz: status=${status}, http_code=${READYZ_HTTP_CODE}"
  log_info "  database: ${db_status}"
  log_info "  rpc: ${rpc_status}"
  log_info "  indexer: ${indexer_status}"

  # Show additional details
  local rpc_block indexer_block indexer_lag
  rpc_block=$(extract_json_field "$READYZ_RESPONSE" '.checks.rpc.block_number' 'null')
  indexer_block=$(extract_json_field "$READYZ_RESPONSE" '.checks.indexer.last_indexed_block' 'null')
  indexer_lag=$(extract_json_field "$READYZ_RESPONSE" '.checks.indexer.lag' 'null')

  log_info "  rpc_block_number: ${rpc_block}"
  log_info "  last_indexed_block: ${indexer_block}, lag: ${indexer_lag}"

  # Check for errors
  local db_error rpc_error indexer_error
  db_error=$(extract_json_field "$READYZ_RESPONSE" '.checks.database.error' '')
  rpc_error=$(extract_json_field "$READYZ_RESPONSE" '.checks.rpc.error' '')
  indexer_error=$(extract_json_field "$READYZ_RESPONSE" '.checks.indexer.error' '')

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

  # For Gnosis full sync test, indexer must be healthy (within 10 blocks)
  # For other tests, indexer might not be fully synced yet
  if [[ $SMOKE_CONFIG == *"gnosis-full-sync"* ]] && [ "$indexer_status" != "healthy" ]; then
    log_error "Indexer check failed for Gnosis full sync test - must be within 10 blocks of chain head"
    return 1
  elif [ "$indexer_status" != "healthy" ]; then
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
    -d '{"query": "{ chainInfo { ... on ChainInfo { network chainId } } }"}')

  local network chain_id
  network=$(extract_json_field "$response" '.data.chainInfo.network' '')
  chain_id=$(extract_json_field "$response" '.data.chainInfo.chainId' '')

  if [ -z "$network" ] || [ -z "$chain_id" ]; then
    log_error "GraphQL query failed"
    log_error "Response: ${response}"
    return 1
  fi

  log_info "GraphQL: network=${network}, chainId=${chain_id}"
  return 0
}

# Check if blocks have been mined
check_blocks_mined() {
  local response
  response=$(curl -sf "${BLOKLID_URL}/readyz" || echo "{}")

  local rpc_block
  rpc_block=$(extract_json_field "$response" '.checks.rpc.block_number' '0')

  log_info "    RPC block number: ${rpc_block}"
  [ "$rpc_block" -gt 0 ]
}

# Check if indexer has synced
check_indexer_synced() {
  local response
  response=$(curl -sf "${BLOKLID_URL}/readyz" || echo "{}")

  local indexed_block rpc_block lag
  indexed_block=$(extract_json_field "$response" '.checks.indexer.last_indexed_block' '0')
  rpc_block=$(extract_json_field "$response" '.checks.rpc.block_number' '0')
  lag=$(extract_json_field "$response" '.checks.indexer.lag' '999')

  log_info "    Indexed: ${indexed_block}, RPC: ${rpc_block}, Lag: ${lag}"

  if [ "$indexed_block" -gt 0 ]; then
    # Verify indexer is reasonably close to chain head
    if [ "$lag" -gt "$MAX_INDEXER_LAG" ]; then
      log_warn "Indexer lag is high: ${lag} blocks (threshold: ${MAX_INDEXER_LAG})"
    fi
    return 0
  fi

  return 1
}

# Test that indexer follows the chain
test_indexer_follows_chain() {
  log_info "Testing indexer chain following..."

  # Step 1: Wait for blocks to be mined
  if ! retry_with_timeout "$CHAIN_SYNC_TIMEOUT" "$CHAIN_SYNC_POLL_INTERVAL" "Waiting for blocks to be mined" "check_blocks_mined"; then
    log_error "No blocks mined"
    return 1
  fi

  # Step 2: Wait for indexer to catch up
  if ! retry_with_timeout "$CHAIN_SYNC_TIMEOUT" "$CHAIN_SYNC_POLL_INTERVAL" "Waiting for indexer to catch up" "check_indexer_synced"; then
    log_error "Indexer did not index any blocks"
    return 1
  fi

  log_info "Indexer successfully following chain"
  return 0
}

# Main test execution
main() {
  local config_desc
  case "${SMOKE_CONFIG:-config-smoke.toml}" in
  "config-smoke.toml")
    config_desc="Local Anvil testnet (fast, no external dependencies)"
    ;;
  "config-smoke-gnosis.toml")
    config_desc="Gnosis Chain RPC (allows high lag for connectivity testing)"
    ;;
  "config-smoke-gnosis-full-sync.toml")
    config_desc="Gnosis Chain RPC (requires full sync within 10 blocks)"
    # Use longer timeouts for full sync test
    CHAIN_SYNC_TIMEOUT=600 # 10min
    CHAIN_SYNC_POLL_INTERVAL=5
    ;;
  *)
    config_desc="Custom config: ${SMOKE_CONFIG}"
    ;;
  esac

  log_info "Starting blokli smoke tests"
  log_info "================================"
  log_info "Config file: ${SMOKE_CONFIG:-config-smoke.toml}"
  log_info "Test type: ${config_desc}"
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

  if ! test_indexer_follows_chain; then
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
