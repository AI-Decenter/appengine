#!/usr/bin/env bash
set -euo pipefail
IMAGE_TAG=${1:-aether-dev-hot-sidecar:latest}
DIR=$(cd "$(dirname "$0")/.." && pwd)
cd "$DIR"
if ! command -v docker >/dev/null 2>&1; then
  echo "docker not found" >&2
  exit 1
fi
docker build -f sidecar/Dockerfile -t "$IMAGE_TAG" .
echo "Built $IMAGE_TAG"
