# Multi-stage build for dev-hot sidecar containing fetch loop dependencies + helper binaries
# Stage 1: build binaries
FROM rust:1.80-slim AS build
WORKDIR /src
# Leverage workspace; copy only necessary crates for speed (fallback copy .) 
COPY Cargo.toml Cargo.lock ./
COPY crates/json-extract ./crates/json-extract
COPY crates/ed25519-verify ./crates/ed25519-verify
# Create minimal dummy main workspace to satisfy build context
RUN mkdir -p crates/dummy && echo "[package]\nname=sidecar-dummy\nversion=0.0.0\nedition=2021\n" > crates/dummy/Cargo.toml
# Build the two binaries (release)
RUN cargo build --release -p json-extract -p ed25519-verify

# Stage 2: runtime image (busybox for wget/sh/tar)
FROM debian:stable-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates wget tar && rm -rf /var/lib/apt/lists/*
WORKDIR /
COPY --from=build /src/target/release/json-extract /json-extract
COPY --from=build /src/target/release/ed25519-verify /verifier/ed25519-verify
RUN chmod +x /json-extract /verifier/ed25519-verify
ENTRYPOINT ["/bin/sh"]
