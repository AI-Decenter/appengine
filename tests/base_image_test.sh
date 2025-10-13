#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")"/.. && pwd)"
IMG_DIR="$ROOT_DIR/images/aether-nodejs/20-slim"
DOCKERFILE="$IMG_DIR/Dockerfile"
README="$IMG_DIR/README.md"
WORKFLOW="$ROOT_DIR/.github/workflows/base-image.yml"
MAKEFILE="$ROOT_DIR/Makefile"

fail() { echo "[FAIL] $*" >&2; exit 1; }
pass() { echo "[PASS] $*"; }

assert_file() {
  local f="$1"
  [[ -f "$f" ]] || fail "Expected file to exist: $f"
  pass "File exists: $f"
}

assert_grep() {
  local pattern="$1"; shift
  local file="$1"; shift || true
  grep -E "${pattern}" "$file" >/dev/null || fail "Pattern not found in ${file}: ${pattern}"
  pass "Pattern found in $(basename "$file"): ${pattern}"
}

assert_make_target() {
  local target="$1"
  grep -E "^${target}:" "$MAKEFILE" >/dev/null || fail "Make target missing: ${target}"
  pass "Make target present: ${target}"
}

echo "== Base image pipeline tests =="

# 1) Files must exist
assert_file "$DOCKERFILE"
assert_file "$README"
assert_file "$WORKFLOW"

# 2) Dockerfile content checks
assert_grep '^FROM\s+node:20(-bookworm)?-slim' "$DOCKERFILE"
assert_grep '^# OCI labels' "$DOCKERFILE"
assert_grep 'RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates' "$DOCKERFILE"
assert_grep 'rm -rf /var/lib/apt/lists/' "$DOCKERFILE"
assert_grep '^USER\s+node' "$DOCKERFILE"
assert_grep '^WORKDIR\s+/home/node/app' "$DOCKERFILE"

# 3) README usage hints
assert_grep 'Usage' "$README"
assert_grep 'ghcr.io' "$README"

# 4) Makefile targets
assert_make_target base-image-build
assert_make_target base-image-scan
assert_make_target base-image-sbom
assert_make_target base-image-push

# 5) GitHub workflow basics
assert_grep '^name: Base image' "$WORKFLOW"
assert_grep 'on:' "$WORKFLOW"
assert_grep 'schedule:' "$WORKFLOW"
assert_grep 'build-push-action' "$WORKFLOW"
assert_grep 'ghcr.io' "$WORKFLOW"
assert_grep 'trivy' "$WORKFLOW"
assert_grep 'grype' "$WORKFLOW"
assert_grep 'SBOM' "$WORKFLOW"

echo "All checks passed (static)."
