#!/usr/bin/env bash
set -euo pipefail

# AetherEngine Development Environment Bootstrap & Verification Script
# Academic-style, deterministic, idempotent where feasible.
# Usage:
#   ./dev.sh bootstrap [--no-postgres] [--no-minio]  # install / configure dependencies
#   ./dev.sh verify      # run readiness diagnostics
#   ./dev.sh help        # show help
#   ./dev.sh clean       # optional: remove local ephemeral containers

PROJECT_NAME="AetherEngine"
POSTGRES_CONTAINER="aether-postgres"
MINIO_CONTAINER="aether-minio"
REQUIRED_RUST_COMPONENTS=(rustfmt clippy)
POSTGRES_IMAGE="postgres:15-alpine"
MINIO_IMAGE="quay.io/minio/minio"
MINIO_ROOT_USER="aether"
MINIO_ROOT_PASSWORD="aethersecret"
ARTIFACT_BUCKET="aether-artifacts"

COLOR_RED='\033[0;31m'
COLOR_GREEN='\033[0;32m'
COLOR_YELLOW='\033[0;33m'
COLOR_RESET='\033[0m'

log() { echo -e "${COLOR_GREEN}[INFO]${COLOR_RESET} $*"; }
warn() { echo -e "${COLOR_YELLOW}[WARN]${COLOR_RESET} $*" >&2; }
err() { echo -e "${COLOR_RED}[ERROR]${COLOR_RESET} $*" >&2; }

need_cmd() {
  if ! command -v "$1" >/dev/null 2>&1; then
    return 1
  fi
  return 0
}

snapd_active() {
  # Return 0 if snapd socket exists and is responsive
  if [ -S /run/snapd.socket ]; then
    return 0
  fi
  return 1
}

ensure_rust() {
  if need_cmd cargo && need_cmd rustc; then
    log "Rust toolchain present: $(rustc --version)"
  else
    log "Installing rustup (stable channel)"
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal
    # shellcheck source=/dev/null
    source "$HOME/.cargo/env"
  fi
  for c in "${REQUIRED_RUST_COMPONENTS[@]}"; do
    if rustup component add "$c" >/dev/null 2>&1; then
      log "Rust component ensured: $c"
    fi
  done
}

ensure_docker() {
  if need_cmd docker; then
    log "Docker present: $(docker --version)"
  else
    warn "Docker not found. Please install Docker manually for your distribution. Skipping auto-install."
  fi
  if groups "$USER" | grep -q docker; then
    :
  else
    warn "User '$USER' not in docker group (may require sudo)."
  fi
}

ensure_microk8s() {
  if need_cmd microk8s; then
    log "MicroK8s present. Checking status..."
    if sudo microk8s status --wait-ready >/dev/null 2>&1; then
      log "MicroK8s ready."
    else
      warn "MicroK8s installed but not ready."
    fi
  else
    if need_cmd snap && snapd_active; then
      log "Installing MicroK8s via snap (requires sudo)."
      if sudo snap install microk8s --classic; then
        sudo usermod -a -G microk8s "$USER"
        sudo chown -R "$USER" ~/.kube || true
        log "Waiting for MicroK8s readiness..."
        sudo microk8s status --wait-ready || warn "MicroK8s failed to report ready."
      else
        warn "Snap install attempt for MicroK8s failed. Skipping Kubernetes bootstrap."
        return 0
      fi
    else
      warn "snap or snapd not available; skipping MicroK8s installation."
      return 0
    fi
  fi
  if need_cmd microk8s; then
    # Enable essential addons if not already
    for addon in dns storage ingress metrics-server; do
      if sudo microk8s status 2>/dev/null | grep -q "${addon}: enabled"; then
        log "Addon already enabled: $addon"
      else
        log "Enabling addon: $addon"
        sudo microk8s enable "$addon" || warn "Failed to enable addon $addon"
      fi
    done
    # Create namespace
    sudo microk8s kubectl create namespace aether-system --dry-run=client -o yaml | sudo microk8s kubectl apply -f - >/dev/null 2>&1 || true
  fi
}

ensure_postgres() {
  if need_cmd docker; then
    if docker ps -a --format '{{.Names}}' | grep -q "^${POSTGRES_CONTAINER}$"; then
      if docker ps --format '{{.Names}}' | grep -q "^${POSTGRES_CONTAINER}$"; then
        log "PostgreSQL container already running."
      else
        log "Starting existing PostgreSQL container."
        docker start "$POSTGRES_CONTAINER" >/dev/null
      fi
    else
      log "Launching PostgreSQL container (${POSTGRES_IMAGE})."
      docker run -d --name "$POSTGRES_CONTAINER" -e POSTGRES_PASSWORD=postgres -e POSTGRES_USER=aether -e POSTGRES_DB=aether_dev -p 5432:5432 "$POSTGRES_IMAGE" >/dev/null || warn "Failed to start PostgreSQL container."
    fi
  fi
}

ensure_minio() {
  if need_cmd docker; then
    if docker ps -a --format '{{.Names}}' | grep -q "^${MINIO_CONTAINER}$"; then
      if docker ps --format '{{.Names}}' | grep -q "^${MINIO_CONTAINER}$"; then
        log "MinIO container already running."
      else
        log "Starting existing MinIO container."
        docker start "$MINIO_CONTAINER" >/dev/null
      fi
    else
      log "Launching MinIO container (${MINIO_IMAGE})."
      docker run -d --name "$MINIO_CONTAINER" -p 9000:9000 -p 9001:9001 \
        -e MINIO_ROOT_USER=${MINIO_ROOT_USER} \
        -e MINIO_ROOT_PASSWORD=${MINIO_ROOT_PASSWORD} \
        ${MINIO_IMAGE} server /data --console-address :9001 >/dev/null || warn "Failed to start MinIO container."
    fi
  fi
}

create_minio_bucket() {
  if ! need_cmd mc; then
    warn "MinIO client (mc) not found; attempting on-the-fly install (requires sudo)."
    local mc_url="https://dl.min.io/client/mc/release/linux-amd64/mc"
    local mc_sha_url="https://dl.min.io/client/mc/release/linux-amd64/mc.sha256sum"
    if curl -sSf -o /tmp/mc "$mc_url" && curl -sSf -o /tmp/mc.sha256sum "$mc_sha_url"; then
      local expected
      expected=$(cut -d ' ' -f1 /tmp/mc.sha256sum)
      local actual
      actual=$(sha256sum /tmp/mc | cut -d ' ' -f1)
      if [[ "$expected" != "$actual" ]]; then
        err "Checksum mismatch for mc (expected $expected got $actual). Aborting mc install."
        return 0
      fi
      chmod +x /tmp/mc
      sudo mv /tmp/mc /usr/local/bin/mc || warn "Failed to move mc binary (permission?)."
    else
      warn "Failed to download mc or checksum. Skipping bucket creation."
      return 0
    fi
  fi
  if mc alias set aether http://127.0.0.1:9000 ${MINIO_ROOT_USER} ${MINIO_ROOT_PASSWORD} >/dev/null 2>&1; then
    if mc ls aether/${ARTIFACT_BUCKET} >/dev/null 2>&1; then
      log "MinIO bucket exists: ${ARTIFACT_BUCKET}"
    else
      log "Creating MinIO bucket: ${ARTIFACT_BUCKET}"
      mc mb aether/${ARTIFACT_BUCKET} >/dev/null 2>&1 || warn "Bucket creation may have failed or already exists."
    fi
  else
    warn "Failed to configure mc alias for MinIO."
  fi
}

verify_rust() {
  if need_cmd rustc; then
    log "Rustc version: $(rustc --version)"
  else
    err "Rust toolchain missing"
    return 1
  fi
}

verify_docker() {
  if need_cmd docker; then
    if docker info >/dev/null 2>&1; then
      log "Docker daemon reachable."
    else
      warn "Docker installed but daemon not reachable (permission or service issue)."
    fi
  else
    warn "Docker missing."
  fi
}

verify_microk8s() {
  if need_cmd microk8s; then
    if sudo microk8s status --wait-ready >/dev/null 2>&1; then
      log "MicroK8s healthy: $(sudo microk8s kubectl get nodes -o name 2>/dev/null | tr '\n' ' ')"
    else
      warn "MicroK8s not ready."
    fi
  else
    warn "MicroK8s not installed."
  fi
}

verify_postgres() {
  if need_cmd docker && docker ps --format '{{.Names}}' | grep -q "^${POSTGRES_CONTAINER}$"; then
    if docker exec -u postgres "$POSTGRES_CONTAINER" pg_isready -q; then
      log "PostgreSQL responsive."
    else
      warn "PostgreSQL container found but not responding."
    fi
  else
    warn "PostgreSQL container not running."
  fi
}

verify_minio() {
  if need_cmd docker && docker ps --format '{{.Names}}' | grep -q "^${MINIO_CONTAINER}$"; then
    log "MinIO container running."
  else
    warn "MinIO container not running."
  fi
}

clean() {
  log "Stopping and removing ephemeral containers (Postgres, MinIO)."
  if need_cmd docker; then
    docker rm -f "$POSTGRES_CONTAINER" >/dev/null 2>&1 || true
    docker rm -f "$MINIO_CONTAINER" >/dev/null 2>&1 || true
  fi
  log "Cleanup complete."
}

bootstrap() {
  local skip_postgres=0
  local skip_minio=0
  while [[ ${1:-} == --* ]]; do
    case "$1" in
      --no-postgres) skip_postgres=1 ;;
      --no-minio) skip_minio=1 ;;
      *) warn "Unknown bootstrap flag: $1" ;;
    esac
    shift || true
  done
  log "=== ${PROJECT_NAME} Bootstrap Start ==="
  ensure_rust
  ensure_docker
  ensure_microk8s
  if [[ $skip_postgres -eq 0 ]]; then
    ensure_postgres
  else
    log "Skipping PostgreSQL per flag"
  fi
  if [[ $skip_minio -eq 0 ]]; then
    ensure_minio
    create_minio_bucket
  else
    log "Skipping MinIO per flag"
  fi
  log "=== Bootstrap Complete ==="
}

verify() {
  log "=== ${PROJECT_NAME} Environment Verification ==="
  verify_rust || true
  verify_docker || true
  verify_microk8s || true
  verify_postgres || true
  verify_minio || true
  log "=== Verification Finished ==="
}

help() {
  cat <<EOF
${PROJECT_NAME} Development Script
Usage: ./dev.sh <command>

Commands:
  bootstrap   Install / configure local dependencies
  verify      Run readiness diagnostics
  clean       Remove ephemeral local service containers
  help        Show this help
EOF
}

main() {
  cmd=${1:-help}
  shift || true
  case "$cmd" in
    bootstrap) bootstrap "$@" ;;
    verify) verify ;;
    clean) clean ;;
    help|--help|-h) help ;;
    *) err "Unknown command: $cmd"; help; exit 1 ;;
  esac
}

main "$@"
