#!/usr/bin/env bash

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
  aarch64 | arm64)
    echo "aarch64"
    ;;
  *)
    log_error "Unsupported architecture: ${arch}"
    exit 1
    ;;
  esac
}

detect_arch
