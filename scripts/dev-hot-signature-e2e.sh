#!/usr/bin/env bash
set -euo pipefail
# E2E test: valid signature success then invalid signature failure marker.
# Requirements: cargo (for ed25519-verify), kubectl, jq, aether CLI in PATH, running control-plane listening with AETHER_API_BASE.

APP_NAME=${APP_NAME:-demo-app}
NAMESPACE=${NAMESPACE:-default}
SLO_SEC=${SLO_SEC:-15}
WORKDIR=$(mktemp -d)
cleanup(){ rm -rf "$WORKDIR" || true; }
trap cleanup EXIT

# 1. Generate seed & pubkey
SEED_HEX=$(head -c 32 /dev/urandom | hexdump -v -e '/1 "%02x"')
PUBKEY_HEX=$(cargo run --quiet -p ed25519-verify -- pubkey "$SEED_HEX")
# export for CLI signing
export AETHER_SIGNING_KEY=$SEED_HEX
# create public key secret (raw 32 bytes -> base64)
RAW_BYTES=$(printf "%s" "$PUBKEY_HEX" | xxd -r -p | base64 -w0)
cat <<EOF | kubectl apply -f -
apiVersion: v1
kind: Secret
metadata:
  name: aether-pubkey
  namespace: $NAMESPACE
type: Opaque
data:
  PUBKEY: $RAW_BYTES
EOF

echo "[info] Created aether-pubkey secret"

# 2. Prepare sample node project
pushd "$WORKDIR" >/dev/null
cat > package.json <<'P'
{ "name": "demo-app", "version": "0.0.1" }
P
cat > server.js <<'S'
const http = require('http');
const start = Date.now();
http.createServer((req,res)=>{res.end('v1 '+start);}).listen(3000);
S

export AETHER_API_BASE=${AETHER_API_BASE:-http://localhost:8080}

# 3. Deploy initial (dev-hot) signed
if ! aether deploy --dev-hot >/dev/null 2>&1; then
  echo "[error] initial deploy failed"; exit 2; fi

# Wait for pod ready and sidecar log REFRESH_OK
start_ts=$(date +%s)
while true; do
  if kubectl get pod -n $NAMESPACE -l app=$APP_NAME -o jsonpath='{.items[0].status.phase}' 2>/dev/null | grep -q Running; then
    POD=$(kubectl get pod -n $NAMESPACE -l app=$APP_NAME -o jsonpath='{.items[0].metadata.name}')
    if kubectl logs -n $NAMESPACE "$POD" -c fetcher 2>/dev/null | grep -q 'REFRESH_OK'; then
      echo "[success] Initial signed deploy verified"
      break
    fi
  fi
  if [ $(( $(date +%s) - start_ts )) -gt $SLO_SEC ]; then
    echo "[error] timeout waiting for REFRESH_OK"; exit 10; fi
  sleep 1
done

# 4. Change source to force new digest, create wrong signature
sleep 1
echo "// change" >> server.js
# normal deploy (will produce .sig) then corrupt signature
ARTIFACT_SIG=$(ls app-*.tar.gz.sig 2>/dev/null || true)
if aether deploy --dev-hot >/dev/null 2>&1; then
  # After deploy command returns, signature used in request already; we need a second attempt with bad signature.
  echo "[info] performing second deploy with corrupted signature";
else
  echo "[error] second deploy (expected success) failed"; exit 3;
fi
# Force third deploy with wrong signature: rebuild artifact but replace .sig before upload
rm -f app-*.tar.gz app-*.tar.gz.sig
# touch to change digest
echo "// second change" >> server.js
# Repack only
if ! aether deploy --dev-hot --pack-only >/dev/null 2>&1; then echo "[error] pack-only failed"; exit 4; fi
SIG_FILE=$(ls app-*.tar.gz.sig)
# Overwrite signature with random invalid 64-byte -> hex 128 chars
head -c 64 /dev/urandom | hexdump -v -e '/1 "%02x"' > "$SIG_FILE"
# Upload manually via legacy flow to include header
if ! AETHER_MULTIPART_THRESHOLD_BYTES=0 aether deploy --dev-hot >/dev/null 2>&1; then echo "[error] corrupt signature deploy request failed"; exit 5; fi

# 5. Expect REFRESH_FAIL reason=signature for new digest
NEW_POD=$(kubectl get pod -n $NAMESPACE -l app=$APP_NAME -o jsonpath='{.items[0].metadata.name}')
fail_found=0
for i in $(seq 1 $SLO_SEC); do
  if kubectl logs -n $NAMESPACE "$NEW_POD" -c fetcher | grep -q 'REFRESH_FAIL.*reason=signature'; then fail_found=1; break; fi
  sleep 1
done
if [ $fail_found -eq 1 ]; then
  echo "[success] Detected REFRESH_FAIL reason=signature as expected"; exit 0
else
  echo "[error] did not observe signature failure"; exit 20
fi
