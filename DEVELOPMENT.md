# AetherEngine Development Environment Guide

Audience: Platform engineers and contributors implementing or extending the MVP control plane, CLI, and runtime integration.

This document provides academically styled, precise, and reproducible instructions for establishing a fully functional local development and test environment on a Linux workstation.

---
## 1. System Prerequisites

| Category | Requirement | Recommended | Notes |
|----------|-------------|-------------|-------|
| OS | Linux (x86_64) | Ubuntu 22.04 LTS | Other modern distributions acceptable |
| CPU | 4 cores | 8+ cores | Parallel builds & local cluster |
| Memory | 8 GB | 16+ GB | MicroK8s + Docker + Postgres + tooling |
| Disk | 10 GB free | 30+ GB | Artifact caching & containers |
| Network | Outbound HTTPS | Stable broadband | Required for crates.io, npm registry, image pulls |

Ensure the system clock is synchronized (chrony or systemd-timesyncd) for TLS + token validity.

---
## 2. Core Toolchain Components

| Component | Purpose | Installation Strategy |
|-----------|---------|-----------------------|
| Rust Toolchain | Core language for CLI + Control Plane | rustup (stable channel) |
| Cargo Clippy / Fmt | Linting + formatting | rustup component add |
| Docker Engine | Image packaging + local runtime base | Distribution packages / convenience script |
| MicroK8s | Local single-node Kubernetes cluster | snap install microk8s --classic |
| kubectl | K8s API client (bundled with MicroK8s alias) | microk8s kubectl or separate binary |
| PostgreSQL | Metadata datastore | Docker container (ephemeral) |
| MinIO (Optional) | Artifact registry emulator | Docker container |
| Node.js LTS | Runtime detection / local parity | nvm or distribution package |

---
## 3. Installation Automation

A helper script `dev.sh` provides two principal verbs:
* bootstrap – Install or configure missing dependencies (idempotent where feasible).
* verify – Run a battery of diagnostics confirming operational readiness.

Invoke:
```
./dev.sh bootstrap
./dev.sh verify
```

---
## 4. Rust Toolchain Policy
* Channel: `stable` (pin with rust-toolchain.toml in future).
* Minimum Version: ≥ 1.74 (adjust based on feature adoption).
* Mandatory Components: `clippy`, `rustfmt`.
* Optional (future): `miri`, `wasm32-wasi` target for experimental modules.

Update cadence: Weekly (automated CI job can enforce). Contributors should rebase after toolchain bumps.

---
## 5. Kubernetes (MicroK8s) Configuration

Enable required addons (after install):
```
sudo microk8s status --wait-ready
sudo microk8s enable dns storage ingress metrics-server
sudo usermod -a -G microk8s "$USER"
sudo chown -R "$USER" ~/.kube || true
newgrp microk8s
```

Context Access:
```
microk8s kubectl get nodes
alias kubectl='microk8s kubectl'
```

Namespace Strategy (MVP): single namespace `aether-system` plus per-app ephemeral namespaces (future roadmap). Create base namespace:
```
microk8s kubectl create namespace aether-system --dry-run=client -o yaml | microk8s kubectl apply -f -
```

---
## 6. PostgreSQL Local Instance

Run ephemeral developer instance:
```
docker run -d --name aether-postgres -e POSTGRES_PASSWORD=postgres -e POSTGRES_USER=aether -e POSTGRES_DB=aether_dev -p 5432:5432 postgres:15-alpine
```

Health Check:
```
pg_isready -h 127.0.0.1 -p 5432 -U aether || docker logs aether-postgres
```

Schema migrations (placeholder until integrated migration tool added). Adopt `sqlx migrate` or `refinery` later.

---
## 7. (Optional) MinIO for Artifact Registry
```
docker run -d --name aether-minio -p 9000:9000 -p 9001:9001 \
  -e MINIO_ROOT_USER=aether \
  -e MINIO_ROOT_PASSWORD=aethersecret \
  quay.io/minio/minio server /data --console-address :9001
```

Create bucket (example):
```
mc alias set aether http://127.0.0.1:9000 aether aethersecret
mc mb aether/aether-artifacts || true
```

If `mc` (MinIO client) absent:
```
wget -q https://dl.min.io/client/mc/release/linux-amd64/mc -O /tmp/mc && chmod +x /tmp/mc && sudo mv /tmp/mc /usr/local/bin/mc
```

---
## 8. Node.js Runtime (For Validation)

Install via nvm (preferred for isolation):
```
command -v nvm >/dev/null || curl -o- https://raw.githubusercontent.com/nvm-sh/nvm/v0.39.7/install.sh | bash
export NVM_DIR="$HOME/.nvm" && . "$NVM_DIR/nvm.sh"
nvm install 20 && nvm use 20
```

Verify:
```
node -v
npm -v
```

---
## 9. Environment Variables (Proposed)

| Variable | Purpose | Example |
|----------|---------|---------|
| AETHER_API_BASE | Control Plane endpoint base URL | http://localhost:8080 |
| AETHER_TOKEN | Auth bearer token (dev) | dev-local-token |
| AETHER_ARTIFACT_BUCKET | Artifact bucket name | aether-artifacts |
| AETHER_RUNTIME | Default runtime tag | nodejs-20 |

Place developer overrides in `.env.development` (not committed) or use a local secrets manager.

---
## 10. Logging & Observability (Local)
* Control Plane: Structured logs (JSON) to stdout (ingested by future aggregator). For local, pretty-print mode acceptable.
* Kubernetes: `microk8s kubectl logs -l app=aether -n aether-system` for aggregated view.
* Database: Enable pg_stat_statements (future optimization instrumentation).

---
## 11. Testing Strategy (Forthcoming)
| Layer | Approach |
|-------|----------|
| Unit | Standard Rust tests (`cargo test`) with feature flags |
| Integration | Spun-up ephemeral Postgres + mocked object storage |
| End-to-End | Local CLI -> Control Plane -> MicroK8s Pod lifecycle |
| Performance | Baseline artifact packaging timing harness |

Tooling to add: `cargo nextest`, `criterion` (microbenchmarks), `k6` or `vegeta` for API load.

---
## 12. Security Hygiene (Local)
* Never commit real credentials or production tokens.
* Use sanitized sample data for fixtures.
* Validate third-party crate licenses prior to inclusion (scriptable audit with `cargo deny`).

---
## 13. Contribution Workflow
1. Fork / feature branch naming: `feature/<concise-scope>`.
2. Implement + add / update tests.
3. Run format + lint + tests: `cargo fmt && cargo clippy --all-targets --all-features -D warnings && cargo test`.
4. Update relevant documentation sections (README, DEVELOPMENT.md) if behavior changes.
5. Open PR with architectural reasoning (why this approach, trade-offs, alternatives).
6. Expect at least one peer technical review before merge.

---
## 14. Failure Modes & Diagnostics Cheatsheet
| Symptom | Likely Cause | Remediation |
|---------|--------------|-------------|
| Pod CrashLoopBackOff | Missing env / bad command | Inspect `kubectl logs <pod> -c main` |
| Deployment stuck Pending | Node resource pressure | `kubectl describe pod` for scheduling events |
| Artifact not found | Wrong digest or bucket | Re-run deploy with `--verbose`, inspect registry listing |
| DB connection refused | Postgres container not ready | `docker logs aether-postgres` + readiness retry |
| CLI auth failures | Expired token | Re-run `aether login` |

---
## 15. Future Enhancements (Dev Environment)
* Tilt or Skaffold integration for rapid Control Plane iteration.
* Local OPA sidecar for policy prototyping.
* Telepresence / mirrord for live traffic shadow testing.

---
## 16. Appendix: Command Recap
```
./dev.sh bootstrap
./dev.sh verify
microk8s kubectl get pods -A
cargo test
```

---
Document Version: 1.0 (MVP)  
Maintainer: (TBD)
