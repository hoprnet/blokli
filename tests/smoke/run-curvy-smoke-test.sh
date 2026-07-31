#!/usr/bin/env bash
# Curvy smoke test for the bloklid-anvil-curvy image
#
# The stock smoke test (run-smoke-test.sh) covers bloklid against a compose stack.
# This one covers the single-container Curvy variant: it asserts that the image
# actually deploys the Curvy v2 suite on Anvil, not merely that it compiles.
#
# Checks:
#   1. bloklid inside the container becomes ready
#   2. curvy_deployed_addresses.json is written and is valid JSON
#   3. every expected Ignition key is present, well-formed and non-zero
#   4. every unique address has bytecode on chain (eth_getCode != 0x)
#   5. the HOPR contracts.toml is written too (the Curvy path must not break it)
#
# Usage:
#   SOURCE_IMAGE=myimage:tag ./run-curvy-smoke-test.sh # pull image from a registry
#   ./run-curvy-smoke-test.sh                          # build the image from nix
#
# Prefer SOURCE_IMAGE. The nix path builds a *Linux* docker image, which needs a
# Linux builder — on macOS it will fail without one, so pull a CI-published image
# instead (note it is linux/amd64 and will run under emulation).
#
# Environment variables:
#   SOURCE_IMAGE      - Image to pull from a remote registry (optional, builds from nix if unset)
#   NIX_FLAKE_TARGET  - Override the flake target to build (default: host-arch Curvy anvil image)
#   KEEP_RUNNING      - If "1" or "true", leave the container running for inspection
#   HOST_PORT_RPC     - Host port to map Anvil to (default: 8545)
#   HOST_PORT_API     - Host port to map the bloklid API to (default: 8080)
#   READY_TIMEOUT     - Seconds to wait for bloklid readiness (default: 180)
#
# Exit codes:
#   0 - All checks passed
#   1 - A check failed

set -euo pipefail

# Default to the host architecture's image rather than hardcoding x86_64, so this
# is usable on arm64 Linux too.
case "$(uname -m)" in
x86_64 | amd64) HOST_NIX_ARCH="x86_64-linux" ;;
arm64 | aarch64) HOST_NIX_ARCH="aarch64-linux" ;;
*) HOST_NIX_ARCH="x86_64-linux" ;;
esac
NIX_FLAKE_TARGET="${NIX_FLAKE_TARGET:-.#docker-bloklid-anvil-curvy-${HOST_NIX_ARCH}}"
LOCAL_IMAGE="bloklid-anvil-curvy:latest"
CONTAINER_NAME="blokli-curvy-smoke"
CURVY_JSON_IN_CONTAINER="/data/curvy_deployed_addresses.json"
CONTRACTS_TOML_IN_CONTAINER="/data/contracts.toml"

HOST_PORT_RPC="${HOST_PORT_RPC:-8545}"
HOST_PORT_API="${HOST_PORT_API:-8080}"
READY_TIMEOUT="${READY_TIMEOUT:-180}"
READY_POLL_INTERVAL=3

ZERO_ADDRESS="0x0000000000000000000000000000000000000000"

# Keys emitted by CurvyContractAddresses::to_ignition_json() in curvy-bindings.
# Kept explicit so a bindings change that drops a contract fails loudly here.
EXPECTED_KEYS=(
  "PortalFactory#CreateX"
  "PortalFactory#PortalFactory"
  "CurvyAggregator#PoseidonT4"
  "CurvyAggregator#CurvyAggregatorV2Implementation"
  "CurvyAggregator#ERC1967Proxy"
  "CurvyAggregator#CurvyAggregatorAlphaV2"
  "CurvyAggregator#CurvyAggregationVerifier"
  "CurvyAggregator#CurvyPendingNotesCommitmentVerifier"
  "CurvyAggregator#CurvyWithdrawalVerifier"
  "CurvyVault#CurvyVaultV2Implementation"
  "CurvyVault#ERC1967Proxy"
  "CurvyVault#CurvyVaultV2"
  "Devenv#Multicall3"
  "Devenv#ERC20Mock"
)

WORK_DIR=""

log_info() {
  echo "[INFO] $1"
}

log_error() {
  echo "[ERROR] $1"
}

log_pass() {
  echo "  [PASS] $1"
}

should_keep_running() {
  [ "${KEEP_RUNNING:-}" = "1" ] || [ "${KEEP_RUNNING:-}" = "true" ]
}

cleanup() {
  local exit_code=$?

  if [ $exit_code -ne 0 ] && docker ps -a --format '{{.Names}}' | grep -qx "${CONTAINER_NAME}"; then
    log_info "container logs (last 100 lines):"
    docker logs --tail 100 "${CONTAINER_NAME}" 2>&1 || true
  fi

  if should_keep_running; then
    log_info "KEEP_RUNNING set; leaving container ${CONTAINER_NAME} up"
  else
    docker rm -f "${CONTAINER_NAME}" >/dev/null 2>&1 || true
  fi

  if [ -n "${WORK_DIR}" ] && [ -d "${WORK_DIR}" ]; then
    rm -rf "${WORK_DIR}"
  fi
}
trap cleanup EXIT

require_tool() {
  if ! command -v "$1" >/dev/null 2>&1; then
    log_error "$1 is required but not installed"
    exit 1
  fi
}

# Resolve the image under test into ${LOCAL_IMAGE}, either by pulling it from a
# registry (CI, via SOURCE_IMAGE) or by building it from the flake (local runs).
resolve_image() {
  if [ -n "${SOURCE_IMAGE:-}" ]; then
    log_info "pulling image: ${SOURCE_IMAGE}"
    docker pull "${SOURCE_IMAGE}"
    LOCAL_IMAGE="${SOURCE_IMAGE}"
    return
  fi

  require_tool nix

  if [ "$(uname -s)" = "Darwin" ]; then
    log_error "building a Linux docker image on macOS needs a Linux builder."
    log_error "Set SOURCE_IMAGE to a published image instead, e.g.:"
    log_error '  SOURCE_IMAGE=europe-west3-docker.pkg.dev/hoprassociation/docker-images/bloklid-anvil-curvy:latest \'
    log_error "    ./tests/smoke/run-curvy-smoke-test.sh"
    exit 1
  fi

  log_info "building image from ${NIX_FLAKE_TARGET}"
  nix build -L "${NIX_FLAKE_TARGET}" --out-link "${WORK_DIR}/curvy-image"
  docker load <"${WORK_DIR}/curvy-image"
  log_info "loaded ${LOCAL_IMAGE}"
}

start_container() {
  docker rm -f "${CONTAINER_NAME}" >/dev/null 2>&1 || true

  log_info "starting ${CONTAINER_NAME} (rpc :${HOST_PORT_RPC}, api :${HOST_PORT_API})"
  # ANVIL_HOST=0.0.0.0 so the chain is reachable from the host for eth_getCode.
  docker run -d --name "${CONTAINER_NAME}" \
    -e ANVIL_HOST=0.0.0.0 \
    -p "${HOST_PORT_RPC}:8545" \
    -p "${HOST_PORT_API}:8080" \
    "${LOCAL_IMAGE}" >/dev/null
}

wait_for_ready() {
  local elapsed=0
  local response status

  log_info "waiting for bloklid readiness (timeout: ${READY_TIMEOUT}s)"
  while [ "${elapsed}" -lt "${READY_TIMEOUT}" ]; do
    # A dead container will never become ready; fail fast instead of waiting out
    # the whole timeout.
    if ! docker ps --format '{{.Names}}' | grep -qx "${CONTAINER_NAME}"; then
      log_error "container exited before becoming ready"
      return 1
    fi

    response=$(curl -sf "http://127.0.0.1:${HOST_PORT_API}/readyz" 2>/dev/null || echo "")
    if [ -n "${response}" ]; then
      status=$(echo "${response}" | jq -r '.status' 2>/dev/null || echo "")
      if [ "${status}" = "ready" ] || [ "${status}" = "healthy" ]; then
        log_pass "bloklid ready after ${elapsed}s"
        return 0
      fi
    fi

    sleep "${READY_POLL_INTERVAL}"
    elapsed=$((elapsed + READY_POLL_INTERVAL))
  done

  log_error "bloklid did not become ready within ${READY_TIMEOUT}s"
  return 1
}

# docker cp needs no shell in the image and no bind mount, so it sidesteps both
# volume permissions and the image's (minimal) userland.
copy_artifacts() {
  if ! docker cp "${CONTAINER_NAME}:${CURVY_JSON_IN_CONTAINER}" "${WORK_DIR}/curvy.json" 2>/dev/null; then
    log_error "${CURVY_JSON_IN_CONTAINER} missing — Curvy deployment did not complete"
    return 1
  fi
  log_pass "curvy_deployed_addresses.json present"

  if ! docker cp "${CONTAINER_NAME}:${CONTRACTS_TOML_IN_CONTAINER}" "${WORK_DIR}/contracts.toml" 2>/dev/null; then
    log_error "${CONTRACTS_TOML_IN_CONTAINER} missing — HOPR deployment did not complete"
    return 1
  fi
  log_pass "contracts.toml present"
}

validate_json() {
  if ! jq -e . "${WORK_DIR}/curvy.json" >/dev/null 2>&1; then
    log_error "curvy_deployed_addresses.json is not valid JSON:"
    cat "${WORK_DIR}/curvy.json"
    return 1
  fi
  log_pass "valid JSON"

  local key address failures=0
  for key in "${EXPECTED_KEYS[@]}"; do
    address=$(jq -r --arg k "${key}" '.[$k] // empty' "${WORK_DIR}/curvy.json")

    if [ -z "${address}" ]; then
      log_error "missing key: ${key}"
      failures=$((failures + 1))
      continue
    fi
    if ! [[ ${address} =~ ^0x[0-9a-fA-F]{40}$ ]]; then
      log_error "malformed address for ${key}: ${address}"
      failures=$((failures + 1))
      continue
    fi
    if [ "${address}" = "${ZERO_ADDRESS}" ]; then
      log_error "zero address for ${key}"
      failures=$((failures + 1))
      continue
    fi
  done

  if [ "${failures}" -ne 0 ]; then
    log_error "${failures} address(es) failed validation"
    return 1
  fi
  log_pass "all ${#EXPECTED_KEYS[@]} addresses well-formed and non-zero"
}

# The strongest cheap check: an address in the JSON proves the deployer ran, but
# only bytecode on chain proves the contract is actually there.
verify_bytecode() {
  # while-read rather than mapfile: macOS ships bash 3.2, which has no mapfile.
  local addresses=() failures=0 checked=0
  while IFS= read -r line; do
    [ -n "${line}" ] && addresses+=("${line}")
  done < <(jq -r 'to_entries | map(.value) | unique | .[]' "${WORK_DIR}/curvy.json")

  log_info "verifying bytecode for ${#addresses[@]} unique addresses"
  local address code
  for address in "${addresses[@]}"; do
    code=$(curl -sf -X POST "http://127.0.0.1:${HOST_PORT_RPC}" \
      -H 'Content-Type: application/json' \
      --data "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"eth_getCode\",\"params\":[\"${address}\",\"latest\"]}" \
      2>/dev/null | jq -r '.result // empty' || echo "")

    checked=$((checked + 1))
    if [ -z "${code}" ] || [ "${code}" = "0x" ]; then
      log_error "no bytecode at ${address}"
      failures=$((failures + 1))
    fi
  done

  if [ "${failures}" -ne 0 ]; then
    log_error "${failures}/${checked} addresses have no bytecode on chain"
    return 1
  fi
  log_pass "all ${checked} addresses have bytecode on chain"
}

main() {
  require_tool docker
  require_tool jq
  require_tool curl

  WORK_DIR="$(mktemp -d)"

  echo "== Curvy image smoke test =="
  resolve_image
  start_container
  wait_for_ready
  copy_artifacts
  validate_json
  verify_bytecode

  echo
  echo "curvy-smoke: ALL CHECKS PASSED"
}

main "$@"
