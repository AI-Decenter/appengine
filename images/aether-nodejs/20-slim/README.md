# aether-nodejs:20-slim

Hardened Node.js 20 slim base image with:
- Non-root `node` user
- Minimal packages (ca-certificates, dumb-init)
- Up-to-date CA roots

Usage

- From GHCR:
  - Image: `ghcr.io/askernqk/aether-nodejs:20-slim`
  - Pin by date or patch tag: e.g. `ghcr.io/askernqk/aether-nodejs:20-slim-2025-10-13`

- As base in your Dockerfile:

  FROM ghcr.io/askernqk/aether-nodejs:20-slim
  WORKDIR /home/node/app
  COPY --chown=node:node package*.json ./
  RUN npm ci --only=production
  COPY --chown=node:node . .
  CMD ["node", "server.js"]

Security
- Scanned by Trivy and Grype in CI; goal: 0 critical vulnerabilities
- SBOM attached to image artifacts