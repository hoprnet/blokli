#!/bin/bash
set -euo pipefail

ANVIL_HOST="${ANVIL_HOST:-127.0.0.1}"
ANVIL_PORT="${ANVIL_PORT:-8545}"
ANVIL_BLOCK_TIME="${ANVIL_BLOCK_TIME:-1}"
ANVIL_ACCOUNTS="${ANVIL_ACCOUNTS:-10}"
ANVIL_BALANCE="${ANVIL_BALANCE:-10000}"
ANVIL_RPC_URL="${ANVIL_RPC_URL:-http://127.0.0.1:${ANVIL_PORT}}"

# Deployer key used for post-deploy oracle overrides (same default as blokli-contract-deployer).
DEPLOYER_PRIVATE_KEY="${ANVIL_DEPLOYER_PRIVATE_KEY:-0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80}"

# Rotsee-aligned oracle overrides. Defaults mirror live rotsee values.
#   To re-confirm rotsee's current values:
#     cast call 0xca2c60433eC6a10dDEabBbE3Ce7f9737b1a0628C \
#       "currentTicketPrice()(uint256)" --rpc-url https://rpc.gnosischain.com
#     cast call 0x5136Bac09C78af89bDA56F5086A3F3E2Ee4EAfCa \
#       "currentWinProb()(uint56)" --rpc-url https://rpc.gnosischain.com
BLOKLI_TICKET_PRICE_WEI="${BLOKLI_TICKET_PRICE_WEI:-100}"
# Raw uint56 encoding of ~0.000125 winning probability (rotsee value as of 2025-05).
# Use 72057594037927935 (U56 max) to restore the "always wins" behaviour if needed.
BLOKLI_WIN_PROB_U56="${BLOKLI_WIN_PROB_U56:-9007199254735}"

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

TICKET_PRICE_ORACLE=$(awk -F'"' '/^ticket_price_oracle/ {print $2}' "${CONTRACTS_PATH}")
WIN_PROB_ORACLE=$(awk -F'"' '/^winning_probability_oracle/ {print $2}' "${CONTRACTS_PATH}")

if [ -z "${TICKET_PRICE_ORACLE}" ] || [ -z "${WIN_PROB_ORACLE}" ]; then
  echo "Could not parse oracle addresses from ${CONTRACTS_PATH}" >&2
  exit 1
fi

echo "Overriding ticket price oracle (${TICKET_PRICE_ORACLE}) -> ${BLOKLI_TICKET_PRICE_WEI} wei"
cast send --rpc-url "${ANVIL_RPC_URL}" \
  --private-key "${DEPLOYER_PRIVATE_KEY}" \
  "${TICKET_PRICE_ORACLE}" "setTicketPrice(uint256)" "${BLOKLI_TICKET_PRICE_WEI}" \
  >/dev/null

echo "Overriding winning probability oracle (${WIN_PROB_ORACLE}) -> ${BLOKLI_WIN_PROB_U56} (U56)"
cast send --rpc-url "${ANVIL_RPC_URL}" \
  --private-key "${DEPLOYER_PRIVATE_KEY}" \
  "${WIN_PROB_ORACLE}" "setWinProb(uint56)" "${BLOKLI_WIN_PROB_U56}" \
  >/dev/null

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
max_indexer_lag = 10
timeout = "5s"
readiness_check_interval = "5s"
EOF

cat "${CONTRACTS_PATH}" >>"${CONFIG_PATH}"
rm -f "${CONTRACTS_PATH}"

exec bloklid -c "${CONFIG_PATH}"
