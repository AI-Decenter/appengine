#!/usr/bin/env bash
set -euo pipefail

# Simple guard to ensure we don't regress to legacy HTTP/TLS stacks.
# Fails if any legacy versions are found in the dependency graph.

cd "$(dirname "$0")/.."

echo "[check] Generating cargo tree (this may take a moment)..." >&2
tree_out=$(cargo tree 2>/dev/null || true)

fail=0

check() {
  local pattern="$1"; local why="$2"
  if echo "$tree_out" | grep -qE "$pattern"; then
    echo "[FAIL] Found legacy crate: pattern='$pattern' ($why)" >&2
    fail=1
  else
    echo "[OK] No match for: $why" >&2
  fi
}
  tree_out=$(cargo tree --no-dev-deps 2>/dev/null || true)
  echo "[check] Generating cargo tree (non-dev deps; this may take a moment)..." >&2

# Legacy paths we want to eliminate
check '\bhyper v0\.14\.' "hyper 0.14 present (should be >=1.0)"
check '\bh2 v0\.3\.' "h2 0.3 present (should be >=0.4)"
check '\bhttp v0\.2\.' "http 0.2 present (should be >=1.0)"
check '\brustls v0\.21\.' "rustls 0.21 present (should be >=0.23)"
check '\btokio-rustls v0\.2(4|5)\.' "tokio-rustls < 0.26 present (should be >=0.26)"
check '\bh(yper-)?rustls v0\.2(4|5|6)\.' "hyper-rustls < 0.27 present (should be >=0.27)"

if [[ $fail -ne 0 ]]; then
  echo "[check] Network stack verification FAILED." >&2
  exit 1
fi

echo "[check] Network stack verification PASSED."
exit 0
