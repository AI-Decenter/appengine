#!/usr/bin/env bash
# Dev Hot Reload E2E latency harness
# Usage: ./scripts/dev-hot-e2e.sh <app> <artifact-url> <sha256-digest-no-prefix> [namespace]
# Requires: kubectl, jq, date (with ms: GNU date), bash.
# Flow:
#  1. Patch deployment annotations with new artifact URL + digest (adds sha256: prefix automatically).
#  2. Find fetcher sidecar pod.
#  3. Measure time until REFRESH_OK log with matching digest appears (<=10s expected).
# Exit codes:
#  0 success (within SLO)
#  10 success but exceeded SLO
#  20 failure (no refresh)
set -euo pipefail
APP=${1?-app}
ART=${2?-artifact-url}
DIGEST=${3?-digest}
NS=${4:-default}
SLO_MS=${SLO_MS:-10000}
TMP=$(mktemp)
PATCH_JSON=$(cat <<EOF
{"spec":{"template":{"metadata":{"annotations":{"aether.dev/artifact-url":"${ART}","aether.dev/digest":"sha256:${DIGEST}"}}}}}
EOF
)
START_MS=$(date +%s%3N || date +%s000)
echo "[e2e] Patching deployment annotations app=$APP ns=$NS digest=${DIGEST}"
kubectl -n "$NS" patch deployment "$APP" --type=merge -p "$PATCH_JSON" >/dev/null
# wait for pod list (could be rolling restart). We look for running pod with label app=$APP
TRIES=0
POD=""
while [ $TRIES -lt 30 ]; do
  POD=$(kubectl -n "$NS" get pods -l app="$APP" -o jsonpath='{.items[0].metadata.name}' 2>/dev/null || true)
  [ -n "$POD" ] && PHASE=$(kubectl -n "$NS" get pod "$POD" -o jsonpath='{.status.phase}' 2>/dev/null || true) || PHASE=""
  if [ "$PHASE" = "Running" ]; then break; fi
  sleep 0.5; TRIES=$((TRIES+1))
done
if [ -z "$POD" ]; then echo "[e2e] no pod found"; exit 20; fi
echo "[e2e] Watching logs pod=$POD container=fetcher"
TIMEOUT=$((SLO_MS * 2))
END_DEADLINE=$((START_MS + TIMEOUT))
FOUND=0
while true; do
  NOW=$(date +%s%3N || date +%s000)
  if [ $NOW -gt $END_DEADLINE ]; then break; fi
  # Fetch recent logs (since start) and grep for REFRESH_OK with digest
  kubectl -n "$NS" logs "$POD" -c fetcher --since-time="$(date -Iseconds -u -d @$(($START_MS/1000)))" 2>/dev/null | grep -E "^REFRESH_OK app=.* digest=${DIGEST} " >"$TMP" || true
  if [ -s "$TMP" ]; then FOUND=1; MATCH_LINE=$(tail -n1 "$TMP"); break; fi
  sleep 0.5
done
if [ $FOUND -eq 1 ]; then
  STOP_MS=$(date +%s%3N || date +%s000)
  LAT=$((STOP_MS-START_MS))
  echo "[e2e] REFRESH_OK after ${LAT}ms line='${MATCH_LINE}'"
  if [ $LAT -le $SLO_MS ]; then
    echo "[e2e] SUCCESS within SLO (${SLO_MS}ms)"; exit 0
  else
    echo "[e2e] REFRESH exceeded SLO (${SLO_MS}ms)"; exit 10
  fi
else
  echo "[e2e] FAILED no REFRESH_OK for digest ${DIGEST} within ${TIMEOUT}ms"; exit 20
fi
