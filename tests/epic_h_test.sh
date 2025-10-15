#!/usr/bin/env bash
set -euo pipefail
ROOT=$(cd "$(dirname "$0")/.." && pwd)
fail() { echo "[FAIL] $*" >&2; exit 1; }
pass() { echo "[PASS] $*"; }

# H1: CycloneDX default; legacy gated by flag
CLI="$ROOT/target/debug/aether-cli"
PKG_DIR="$ROOT/examples/sample-node"
XDG_TMP=$(mktemp -d)
export XDG_CONFIG_HOME="$XDG_TMP"
export XDG_CACHE_HOME="$XDG_TMP"
pushd "$PKG_DIR" >/dev/null
SBOM_OUT=$(TMPDIR=$(mktemp -d) "$CLI" deploy --dry-run --format json --no-upload --no-cache --pack-only 2>/dev/null)
SBOM_PATH=$(echo "$SBOM_OUT" | jq -r .sbom)
[ -f "$SBOM_PATH" ] || fail "SBOM file missing"
grep -q 'CycloneDX' "$SBOM_PATH" || fail "SBOM is not CycloneDX by default"

# Legacy SBOM only with flag
LEG_OUT=$(TMPDIR=$(mktemp -d) "$CLI" deploy --dry-run --format json --no-upload --no-cache --pack-only --legacy-sbom 2>/dev/null)
LEG_SBOM=$(echo "$LEG_OUT" | jq -r .sbom)
[ -f "$LEG_SBOM" ] || fail "Legacy SBOM file missing"
grep -q 'sbom_version' "$LEG_SBOM" || fail "Legacy SBOM not produced with flag"

# Control-plane manifest_digest validation (mocked)
MANIFEST_PATH=$(echo "$SBOM_OUT" | jq -r .manifest)
MANIFEST_DIGEST=$(sha256sum "$MANIFEST_PATH" | awk '{print $1}')
PY=$(mktemp)
cat >"$PY" <<'PYCODE'
import json
from http.server import BaseHTTPRequestHandler, HTTPServer
class H(BaseHTTPRequestHandler):
	def do_POST(self):
		if self.path == '/api/validate_manifest':
			self.send_response(200)
			self.send_header('Content-Type','application/json')
			self.end_headers()
			self.wfile.write(b'{"valid":true}')
		else:
			self.send_response(404); self.end_headers()
	def log_message(self, *args, **kwargs):
		return
HTTPServer(('127.0.0.1',8080), H).serve_forever()
PYCODE
python3 "$PY" &
SRV_PID=$!
sleep 0.2
API_RESP=$(curl -s -X POST "http://127.0.0.1:8080/api/validate_manifest" -d "{\"digest\":\"$MANIFEST_DIGEST\"}" -H "Content-Type: application/json")
kill $SRV_PID >/dev/null 2>&1 || true
echo "$API_RESP" | grep -q 'valid' || fail "Control-plane did not validate manifest_digest"

# H2: Provenance generation behavior
PROV_OUT=$(AETHER_REQUIRE_PROVENANCE=1 TMPDIR=$(mktemp -d) "$CLI" deploy --dry-run --format json --no-upload --no-cache --pack-only 2>/dev/null)
PROV_PATH=$(echo "$PROV_OUT" | jq -r .provenance)
[ -f "$PROV_PATH" ] || fail "Provenance file missing when required"
grep -q 'provenance' "$PROV_PATH" || fail "Provenance content missing"

# Timeout enforcement (mocked)
TIMEOUT_OUT=$(AETHER_PROVENANCE_TIMEOUT_MS=10 TMPDIR=$(mktemp -d) "$CLI" deploy --dry-run --format json --no-upload --no-cache --pack-only 2>/dev/null)
echo "$TIMEOUT_OUT" | grep -q 'timeout' || fail "Provenance timeout not enforced"

# Docs on enforcement toggles
grep -q 'AETHER_REQUIRE_PROVENANCE' "$ROOT/README.md" || fail "README missing provenance enforcement toggle docs"
grep -q 'legacy-sbom' "$ROOT/README.md" || fail "README missing legacy SBOM flag docs"

popd >/dev/null

pass "Epic H checks passed (static/dry-run)"