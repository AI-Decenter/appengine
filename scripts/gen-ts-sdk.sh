#!/usr/bin/env bash
set -euo pipefail
API_URL=${API_URL:-http://localhost:3000/openapi.json}
OUT=${OUT:-sdk/types.ts}
mkdir -p "$(dirname "$OUT")"
if ! command -v npx >/dev/null 2>&1; then
  echo "npx not found. Install Node.js to generate TypeScript SDK." >&2
  exit 1
fi
curl -fsSL "$API_URL" -o /tmp/openapi.json
npx --yes openapi-typescript /tmp/openapi.json --output "$OUT"
echo "Generated TypeScript types at $OUT"