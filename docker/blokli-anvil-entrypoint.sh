#!/bin/bash
set -euo pipefail

ANVIL_HOST="${ANVIL_HOST:-127.0.0.1}"
ANVIL_PORT="${ANVIL_PORT:-8545}"
ANVIL_BLOCK_TIME="${ANVIL_BLOCK_TIME:-1}"
ANVIL_ACCOUNTS="${ANVIL_ACCOUNTS:-10}"
ANVIL_BALANCE="${ANVIL_BALANCE:-10000}"
ANVIL_RPC_URL="${ANVIL_RPC_URL:-http://127.0.0.1:${ANVIL_PORT}}"

DATA_DIR="${BLOKLI_DATA_DIRECTORY:-/data}"
CONFIG_PATH="${BLOKLI_CONFIG_PATH:-/config.toml}"

# Cleanup function for graceful shutdown
ANVIL_PID=""
cleanup() {
  if [ -n "${ANVIL_PID}" ]; then
    kill "${ANVIL_PID}" 2>/dev/null || true
    wait "${ANVIL_PID}" 2>/dev/null || true
  fi
}
trap cleanup EXIT INT TERM

mkdir -p "${DATA_DIR}"

anvil \
  --host "${ANVIL_HOST}" \
  --port "${ANVIL_PORT}" \
  --block-time "${ANVIL_BLOCK_TIME}" \
  --accounts "${ANVIL_ACCOUNTS}" \
  --balance "${ANVIL_BALANCE}" &

ANVIL_PID=$!

anvil_ready=false
for _ in $(seq 1 60); do
  if curl -sf -X POST "${ANVIL_RPC_URL}" \
    -H "content-type: application/json" \
    --data '{"jsonrpc":"2.0","method":"eth_blockNumber","params":[],"id":1}' \
    >/dev/null; then
    anvil_ready=true
    break
  fi
  sleep 0.5
done

if [ "${anvil_ready}" != "true" ]; then
  kill "${ANVIL_PID}" >/dev/null 2>&1 || true
  echo "Anvil did not become ready in time" >&2
  exit 1
fi

CONTRACTS_PATH="${DATA_DIR}/contracts-deploy.toml"
DEPLOYER_ARGS=()
if [ -n "${ANVIL_DEPLOYER_PRIVATE_KEY:-}" ]; then
  DEPLOYER_ARGS+=(--private-key "${ANVIL_DEPLOYER_PRIVATE_KEY}")
fi

# Deploy contracts with error handling
if ! blokli-contract-deployer \
  --rpc-url "${ANVIL_RPC_URL}" \
  --output "${CONTRACTS_PATH}" \
  ${DEPLOYER_ARGS[@]+"${DEPLOYER_ARGS[@]}"}; then
  echo "Contract deployment failed" >&2
  exit 1
fi

cat >"${CONFIG_PATH}" <<EOF
data_directory = "${DATA_DIR}"
network = "anvil-localhost"
rpc_url = "${ANVIL_RPC_URL}"
max_rpc_requests_per_sec = 0

[database]
type = "sqlite"
index_path = "${DATA_DIR}/bloklid-index.db"
logs_path = "${DATA_DIR}/bloklid-logs.db"
max_connections = 10

[indexer]
fast_sync = false
enable_logs_snapshot = false

[indexer.subscription]
event_bus_capacity = 100
shutdown_signal_capacity = 10
batch_size = 50

[api]
bind_address = "0.0.0.0:8080"
enabled = true
playground_enabled = true

[api.health]
max_indexer_lag = 2
timeout = "5s"
readiness_check_interval = "5s"
EOF

cat "${CONTRACTS_PATH}" >>"${CONFIG_PATH}"
rm -f "${CONTRACTS_PATH}"

exec bloklid -c "${CONFIG_PATH}"
