#!/usr/bin/env bash
set -euo pipefail
ROOT=$(cd "$(dirname "$0")/.." && pwd)
fail() { echo "[FAIL] $*" >&2; exit 1; }
pass() { echo "[PASS] $*"; }

# I1: Operator guide present and includes key sections
OP="$ROOT/docs/operator-guide.md"
[ -f "$OP" ] || fail "Missing operator guide"
grep -q "Install" "$OP" || fail "Operator guide missing Install section"
grep -q "MinIO" "$OP" || fail "Operator guide missing MinIO config"
grep -q "Postgres" "$OP" || fail "Operator guide missing Postgres config"
grep -q "Deploy sample" "$OP" || fail "Operator guide missing sample deploy"

# I2: Troubleshooting playbook present and includes common failures
TR="$ROOT/docs/troubleshooting.md"
[ -f "$TR" ] || fail "Missing troubleshooting playbook"
grep -q "Quotas" "$TR" || fail "Troubleshooting missing Quotas section"
grep -q "Retention" "$TR" || fail "Troubleshooting missing Retention section"
grep -q "SSE" "$TR" || fail "Troubleshooting missing SSE section"
grep -q "Database" "$TR" || fail "Troubleshooting missing Database section"
grep -q "S3" "$TR" || fail "Troubleshooting missing S3 section"
grep -q "Presign" "$TR" || fail "Troubleshooting missing Presign section"
grep -q "Multipart" "$TR" || fail "Troubleshooting missing Multipart section"

# Cross-links from README and STATUS
grep -q "operator-guide.md" "$ROOT/README.md" || fail "README missing link to operator guide"
grep -q "troubleshooting.md" "$ROOT/README.md" || fail "README missing link to troubleshooting"
grep -q "operator guide" -i "$ROOT/STATUS.md" || fail "STATUS missing operator guide mention"
grep -q "troubleshooting" -i "$ROOT/STATUS.md" || fail "STATUS missing troubleshooting mention"

pass "Epic I docs checks passed (static)"