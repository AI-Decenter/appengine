#!/usr/bin/env bash
set -euo pipefail
# E2E Smoke deploy + metrics
# Usage: scripts/smoke_e2e.sh <app>
# Env:
#  - SMOKE_DRY_RUN=1         Do not hit cluster; simulate timings
#  - SMOKE_MARKDOWN_OUT=path Write markdown summary
#  - AETHER_CLI=aether-cli   CLI binary (default: aether-cli in PATH)
#  - NAMESPACE=default       k8s namespace

APP=${1:-sample-node}
NS=${NAMESPACE:-default}
AETHER_BIN=${AETHER_CLI:-aether-cli}

now_ms() { date +%s%3N 2>/dev/null || echo $(( $(date +%s) * 1000 )); }
dur_ms() { echo $(( $2 - $1 )); }

START_ALL=$(now_ms)

# Step 1: pack
T0=$(now_ms)
if [ "${SMOKE_DRY_RUN:-}" = "1" ]; then
  sleep 0.01
  ARTIFACT="/tmp/${APP}.tar.gz"
  DIGEST="deadbeef"
else
  OUT=$("${AETHER_BIN}" deploy --dry-run --format json 2>/dev/null)
  ARTIFACT=$(echo "$OUT" | jq -r .artifact)
  DIGEST=$(echo "$OUT" | jq -r .digest)
fi
T1=$(now_ms)
PACK_MS=$(dur_ms $T0 $T1)

# Step 2: upload (mocked in dry-run)
T2=$(now_ms)
if [ "${SMOKE_DRY_RUN:-}" = "1" ]; then
  sleep 0.01
  ART_URL="file://${ARTIFACT}"
else
  ART_URL="file://${ARTIFACT}"
fi
T3=$(now_ms)
UPLOAD_MS=$(dur_ms $T2 $T3)

# Step 3: rollout / k8s readiness (mocked here; real flow could helm/kubectl)
T4=$(now_ms)
if [ "${SMOKE_DRY_RUN:-}" = "1" ]; then
  sleep 0.02
  ROLL_MS=20
else
  # Placeholder: real rollout measurement logic would go here
  ROLL_MS=100
fi
T5=$(now_ms)
ROLLOUT_MS=${ROL_MS:-$(dur_ms $T4 $T5)}

STOP_ALL=$(now_ms)
TOTAL_MS=$(dur_ms $START_ALL $STOP_ALL)

# Baseline comparison (static for now; real pipeline can fetch from repo artifact)
BASELINE_TOTAL=${BASELINE_TOTAL_MS:-100000}
REDUCTION=$(( 100 - (100 * TOTAL_MS / BASELINE_TOTAL) ))

JSON=$(jq -n \
  --arg app "$APP" \
  --arg artifact "$ARTIFACT" \
  --arg digest "$DIGEST" \
  --arg art_url "$ART_URL" \
  --arg ns "$NS" \
  --argjson pack $PACK_MS \
  --argjson upload $UPLOAD_MS \
  --argjson rollout $ROLLOUT_MS \
  --argjson total $TOTAL_MS \
  --argjson reduction $REDUCTION \
  '{app:$app, artifact:$artifact, artifact_url:$art_url, digest:$digest, namespace:$ns, pack_ms:$pack, upload_ms:$upload, rollout_ms:$rollout, total_ms:$total, reduction_pct:$reduction}')

if [ -n "${SMOKE_MARKDOWN_OUT:-}" ]; then
  cat >"$SMOKE_MARKDOWN_OUT" <<EOF
# E2E Smoke Summary

App: $APP  
Namespace: $NS  

Timings (ms):
- Pack: $PACK_MS
- Upload: $UPLOAD_MS
- Rollout: $ROLLOUT_MS
- Total: $TOTAL_MS

Reduction vs baseline: ${REDUCTION}%

Artifact: $ART_URL
Digest: $DIGEST
EOF
fi

echo "$JSON"