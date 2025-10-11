#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."

MODE=${1:-release} # or debug
OUT_DIR=docs/issues/11-network-stack-unification-hyper-rustls-upgrade
mkdir -p "$OUT_DIR"

echo "[measure] Cleaning..." >&2
cargo clean

echo "[measure] Building ($MODE)..." >&2
START=$(date +%s)
if [[ "$MODE" == "release" ]]; then
  cargo build --workspace --release -q
else
  cargo build --workspace -q
fi
END=$(date +%s)
DUR=$((END-START))

echo "[measure] Build time: ${DUR}s" | tee "$OUT_DIR/build-time-${MODE}.txt"

echo "[measure] Binary sizes:" | tee "$OUT_DIR/binary-sizes-${MODE}.txt"
for bin in target/${MODE}/aether-cli target/${MODE}/control-plane target/${MODE}/aether-operator; do
  if [[ -f "$bin" ]]; then
    sz=$(stat -c%s "$bin")
    echo "$(basename "$bin"): $sz bytes" | tee -a "$OUT_DIR/binary-sizes-${MODE}.txt"
  fi
done

echo "[measure] Done. Outputs in $OUT_DIR" >&2
