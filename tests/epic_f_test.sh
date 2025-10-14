#!/usr/bin/env bash
set -euo pipefail

ROOT=$(cd "$(dirname "$0")/.." && pwd)
fail() { echo "[FAIL] $*" >&2; exit 1; }
pass() { echo "[PASS] $*"; }

# F1: Sample app polish
SAMPLE_DIR="$ROOT/examples/sample-node"
[ -d "$SAMPLE_DIR" ] || fail "examples/sample-node directory missing"
[ -f "$SAMPLE_DIR/index.js" ] || fail "index.js missing in sample app"
[ -f "$SAMPLE_DIR/package.json" ] || fail "package.json missing in sample app"
grep -q "\\\"name\\\"" "$SAMPLE_DIR/package.json" || fail "package.json missing name field"
grep -q "index.js" "$SAMPLE_DIR/package.json" || fail "package.json missing main/script reference"
grep -q "/ready" "$SAMPLE_DIR/index.js" || grep -q "ready" "$SAMPLE_DIR/index.js" || fail "index.js missing readiness endpoint"

# F2: Smoke script & report (dry-run validation)
SMOKE="$ROOT/scripts/smoke_e2e.sh"
[ -x "$SMOKE" ] || fail "scripts/smoke_e2e.sh missing or not executable"
TMP=$(mktemp -d)
MD_OUT="$TMP/summary.md"
JSON_OUT=$(
  SMOKE_DRY_RUN=1 \
  SMOKE_MARKDOWN_OUT="$MD_OUT" \
  AETHER_CLI=echo \
  "$SMOKE" sample-node 2>/dev/null
)
echo "$JSON_OUT" | grep -q '"pack_ms"' || fail "JSON output missing pack_ms"
echo "$JSON_OUT" | grep -q '"upload_ms"' || fail "JSON output missing upload_ms"
echo "$JSON_OUT" | grep -q '"rollout_ms"' || fail "JSON output missing rollout_ms"
echo "$JSON_OUT" | grep -q '"total_ms"' || fail "JSON output missing total_ms"
echo "$JSON_OUT" | grep -q '"reduction_pct"' || fail "JSON output missing reduction_pct"
[ -f "$MD_OUT" ] || fail "Markdown summary not produced at $MD_OUT"
grep -qi "smoke" "$MD_OUT" || fail "Markdown summary seems incorrect"

# Workflow presence
WF="$ROOT/.github/workflows/e2e-smoke.yml"
[ -f "$WF" ] || fail "Workflow .github/workflows/e2e-smoke.yml missing"
grep -q "smoke_e2e.sh" "$WF" || fail "Workflow must invoke scripts/smoke_e2e.sh"
grep -qi "artifact" "$WF" || fail "Workflow should upload artifacts"

# README snippet
grep -qi "e2e smoke" "$ROOT/README.md" || fail "README missing E2E smoke mention"

pass "Epic F checks passed (static/dry-run)"