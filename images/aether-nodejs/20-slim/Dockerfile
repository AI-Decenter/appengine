# syntax=docker/dockerfile:1.7

# Base: Node.js 20 slim with Debian bookworm
FROM node:20-bookworm-slim

# OCI labels
LABEL org.opencontainers.image.title="aether-nodejs:20-slim" \
      org.opencontainers.image.description="Hardened Node.js 20 slim base with CA certs, non-root user, minimal packages" \
      org.opencontainers.image.source="https://github.com/askerNQK/appengine" \
      org.opencontainers.image.licenses="MIT" \
      org.opencontainers.image.vendor="askerNQK"

# Install only what we need; ensure CA certs are present and up to date
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates dumb-init && update-ca-certificates && rm -rf /var/lib/apt/lists/*

# Create app directory owned by node user (node user exists in base image)
WORKDIR /home/node/app

# Switch to non-root user
USER node

# Use dumb-init as entrypoint to handle PID 1 signals correctly
ENTRYPOINT ["/usr/bin/dumb-init", "--"]

# Default command (overridable)
CMD ["node", "--version"]
