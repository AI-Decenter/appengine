# AetherEngine (MVP v1.0)

![Coverage](https://img.shields.io/badge/coverage-internal--lcov-lightgrey)
<!-- TODO: Replace badge with dynamic value (Codecov or shields.io endpoint) once publishing workflow is added. -->

An internal Platform-as-a-Service (PaaS) designed to minimize application deployment latency and systematically elevate Developer Experience (DX) by transferring the entirety of the build pipeline (dependency resolution, compilation, packaging) to the client edge through a high‑performance Rust CLI. Rather than executing non‑deterministic server‑side builds, developers upload a pre‑assembled, production‑ready artifact. This model is intended to reduce end‑to‑end deployment latency from minutes to seconds while decreasing infrastructure consumption and variance.

---

## 1. Executive Overview

AetherEngine provides an opinionated, artifact‑centric deployment paradigm initially constrained to Node.js Long-Term Support (LTS) runtimes (e.g. 18.x, 20.x). The Minimum Viable Product (MVP) aims to empirically demonstrate an ≥80% reduction in mean deployment time for existing internal Node.js services relative to the incumbent pipeline.

### MVP Success Criteria
* Business: ≥80% reduction in p95 deploy duration versus baseline.
* Product: A cohesive workflow—from local source to production runtime—achieved through 3–5 intuitive CLI commands.
* Technical: A stable, horizontally extensible control plane (Rust + Axum + SQLx + PostgreSQL) and operational data plane (Kubernetes) supporting artifact‑based rollouts.

> Historical Note: Early exploratory drafts referenced TiDB. The authoritative MVP datastore is PostgreSQL (SQLx). TiDB may re‑enter the roadmap for multi‑region or HTAP scenarios.

---

## 2. High-Level Architecture

The platform decomposes into four bounded components:

| Component | Role | Core Technologies |
|-----------|------|-------------------|
| Aether CLI | Local build, packaging, artifact upload, deployment orchestration | Rust, clap, reqwest, tokio |
| Control Plane | API surface, deployment metadata, orchestration, Kubernetes integration | Rust, Axum, SQLx, PostgreSQL, kube-rs |
| Artifact Registry | Immutable storage for packaged application artifacts | S3-compatible object storage (e.g. MinIO) |
| Data Plane | Deterministic execution environment for runtime containers | Kubernetes, optimized Node.js base images |

### Deployment Flow (Node.js)
1. Developer executes `aether deploy` at the project root.
2. CLI detects Node.js project (presence of `package.json`).
3. Runs `npm install --production` (or a deterministic equivalent) locally.
4. Packages source + `node_modules` into a compressed artifact (`app.tar.gz`).
5. Requests a pre‑signed upload URL from the Control Plane.
6. Uploads the artifact to the Artifact Registry.
7. Issues a deployment request (`POST /deployments`) containing artifact digest + runtime metadata.
8. Control Plane persists deployment record and synthesizes a Kubernetes workload specification.
9. Data Plane init container downloads & decompresses the artifact.
10. Main container (base image: `aether-nodejs:20-slim`) executes the defined start command (default: `npm start`).

### Core Principles
* Artifact Immutability & Addressability (content hash).
* Deterministic Local Build (eliminating CI variability).
* Minimal Server Trust Surface (no remote build execution).
* Observability‑first (deployment UUID + digest propagation across logs / traces).

---

## 3. CLI (Aether CLI)

| Command | Purpose | Notes |
|---------|---------|-------|
| `aether login` | Authenticate user; store credential securely | Supports token rotation |
| `aether deploy` | Package & publish artifact; trigger deployment | Auto runtime detection, hash computation |
| `aether logs` | Stream live or historical logs | Pod label selectors |
| `aether list` | Enumerate applications & recent deployments | Future: filtering & pagination |

Planned Enhancements:
* Parallel compression + hashing for large dependency graphs
* Local SBOM generation (supply chain visibility)
* Integrity verification before runtime entrypoint execution

### 3.1 Usage Quick Reference

```
$ aether --help
Global Flags:
	--log-level <trace|debug|info|warn|error> (default: info)
	--log-format <auto|text|json> (default: auto)

Subcommands:
	login [--username <name>]            Authenticate (mock)
	deploy [--dry-run]                   Package and (mock) deploy current project
	logs [--app <name>]                  Show recent logs (mock)
	list                                 List applications (mock)
	completions --shell <bash|zsh|fish>  Generate shell completion script (hidden)
```

Examples:
```
aether login
aether deploy --dry-run
aether deploy
aether --log-format json list
aether completions --shell bash > aether.bash
aether deploy --format json --no-sbom --pack-only
```

Configuration:
* Config file: `${XDG_CONFIG_HOME:-~/.config}/aether/config.toml`
* Session file: `${XDG_CACHE_HOME:-~/.cache}/aether/session.json`
* Env override: `AETHER_DEFAULT_NAMESPACE`
* Ignore file: `.aetherignore` (glob patterns, one per line, # comments)

Exit Codes:
| Code | Meaning |
|------|---------|
| 0 | Success |
| 2 | Usage / argument error (clap) |
| 10 | Config error |
| 20 | Runtime internal |
| 30 | I/O error |
| 40 | Network error (reserved) |

Performance:
Target cold start <150ms (local); CI threshold set to <800ms for noise tolerance.

### 3.2 Deploy JSON Output

When invoking `aether deploy --format json`, the CLI prints a single JSON object to stdout (logs remain on stderr) with the following stable fields:

| Field | Type | Description |
|-------|------|-------------|
| `artifact` | string | Path to generated `.tar.gz` artifact |
| `digest` | string (hex sha256) | Content hash of packaged files (streaming computed) |
| `size_bytes` | number | Size of artifact on disk |
| `manifest` | string | Path to manifest file listing per‑file hashes |
| `sbom` | string|null | Path to SBOM (`.sbom.json`) or null when `--no-sbom` supplied |
| `signature` | string|null | Path to signature file when `AETHER_SIGNING_KEY` provided |

Error Behavior (JSON mode): currently non‑zero failures may still emit human readable text before JSON; future work will standardize an error envelope `{ "error": { code, message } }` (tracked in Issue 01 follow-up – now resolved in this branch by suppressing SBOM generation when skipped).

---

## 4. Control Plane

Responsibilities:
* REST API (Axum) for authentication, deployment management, log access
* Artifact metadata tracking (digest, runtime, size, provenance timestamps)
* Kubernetes workload synthesis via `kube-rs`
* Enforcement of environment, secret, and resource policies

Representative Endpoints (MVP subset):
* `POST /deployments` – Register new deployment (idempotent via artifact digest)
* `GET /apps/{app}/logs` – Stream or tail logs (upgrade: WebSocket or chunked HTTP)
* `GET /apps/{app}/deployments` – List historical deployments
* `POST /artifacts` – Upload artifact (headers: `X-Aether-Artifact-Digest`, optional `X-Aether-Signature`)
* `GET /artifacts` – List recent artifacts (metadata only)
* `GET /healthz`, `GET /readyz` – Liveness / readiness probes

### 4.1 Error Format
All API errors return a stable JSON envelope and appropriate HTTP status code:

```
HTTP/1.1 409 Conflict
Content-Type: application/json

{
	"code": "conflict",
	"message": "application name exists"
}
```

Canonical error codes (subject to extension):
| Code | HTTP | Semantics |
|------|------|-----------|
| `bad_request` | 400 | Payload / validation failure |
| `not_found` | 404 | Entity does not exist |
| `conflict` | 409 | Uniqueness or state conflict |
| `service_unavailable` | 503 | Dependency (DB, downstream) not ready |
| `internal` | 500 | Unclassified unexpected error |

Design Notes:
* Machine-friendly `code` enables future localization / client mapping.
* `message` intentionally human oriented; avoid leaking internal stack traces.
* Additional diagnostic fields (e.g. `details`, `trace_id`) may be added when tracing is wired.
* Non-error (2xx) responses never include this envelope.

### 4.2 Artifact Upload JSON Fields

On success (`200 OK`) the control plane returns:

| Field | Type | Meaning |
|-------|------|---------|
| `artifact_url` | string | Location reference (currently file URI mock) |
| `digest` | string | SHA-256 hex digest (server recomputed) |
| `duplicate` | bool | True if digest already existed (idempotent; file not re-written) |
| `app_linked` | bool | True if `app_name` matched an existing application and was linked |
| `verified` | bool | True if an attached Ed25519 signature matched a registered application public key |

Additional error codes related to artifact upload:
| Code | HTTP | Semantics |
|------|------|-----------|
| `missing_digest` | 400 | Header `X-Aether-Artifact-Digest` absent |
| `invalid_digest` | 400 | Malformed digest (length/hex) |
| `digest_mismatch` | 400 | Provided digest did not match recomputed |

### 4.3 Artifact Public Keys (Signature Verification)

Register an Ed25519 public key for an application so subsequent uploads with header `X-Aether-Signature` over the digest value set `verified=true`.

Endpoint:
`POST /apps/{app_name}/public-keys`
```
{ "public_key_hex": "<64 hex chars>" }
```
Response `201 Created`:
```
{ "app_id": "<uuid>", "public_key_hex": "<hex>", "active": true }
```

Multiple keys per app are allowed (all `active=true` by default). Deactivation endpoint TBD.

### 4.4 Artifact Existence Fast Path
`HEAD /artifacts/{digest}` returns `200` if present, `404` if absent (no body). Enables the CLI to skip re‑uploads.

### 4.5 Concurrency & Backpressure
Uploads are limited by a semaphore (env: `AETHER_MAX_CONCURRENT_UPLOADS`, default `32`). Excess uploads await a permit, preventing resource exhaustion.

### 4.6 Metrics (Prometheus)
| Metric | Type | Description |
|--------|------|-------------|
| `artifact_upload_bytes_total` | Counter | Bytes successfully persisted (new uploads) |
| `artifact_upload_duration_seconds` | Histogram | End-to-end upload + verify duration |
| `artifact_uploads_in_progress` | Gauge | Concurrent in-flight uploads |
| `artifacts_total` | Gauge | Total stored artifacts (initial load + increment on insert) |

### 4.7 Security Scheme
Bearer token auth (`Authorization: Bearer <token>`) configured via `AETHER_API_TOKENS` (CSV) or fallback `AETHER_API_TOKEN`. OpenAPI spec exposes a `bearer_auth` security scheme applied globally.

---

## 5. Artifact Registry

Initial Target: Self‑hosted MinIO (S3-compatible API).

Requirements:
* Pre‑signed URL issuance (time‑boxed; ideally single‑use)
* Content-addressed hierarchy (e.g. `artifacts/<app>/<sha256>/app.tar.gz`)
* Optional server-side encryption (future)
* Lifecycle policies: Age + unreferenced digest reclamation

---

## 6. Data Plane

Kubernetes Design:
* Init Container: Fetch + decompress artifact into ephemeral volume (EmptyDir or ephemeral CSI)
* Main Container: Execute Node.js process (non-root user) with env injection
* Rollout Strategy (MVP): Replace; roadmap includes canary + blue/green
* Observability: Standardized labels `app=aether, app_name=<name>, deployment_id=<uuid>`

Base Image Objectives:
* Slim, reproducible, frequently patched
* Non-root (USER 1000)
* Reduced attack surface (no build toolchain in runtime layer)

---

## 7. Data Model (Concise Overview)

Tables (PostgreSQL via SQLx):
* `applications(id, name, owner, created_at)`
* `artifacts(id, app_id, digest, runtime, size_bytes, created_at)`
* `deployments(id, app_id, artifact_id, status, rollout_started_at, rollout_completed_at)`
* `deployment_events(id, deployment_id, phase, message, timestamp)`
* `users(id, email, auth_provider, created_at)`

---

## 8. Security & Integrity (MVP Orientation)
* Transport Security: HTTPS/TLS enforced for Control Plane + artifact operations.
* Authentication: Short‑lived bearer tokens acquired via `aether login`.
* Authorization: Ownership / role validation on mutating endpoints.
* Integrity: SHA‑256 digest validation at deploy time + (optionally) runtime recheck.
* Isolation: Single shared namespace initially; namespace-per-application in roadmap.

---

## 9. Roadmap (Illustrative Post-MVP Trajectory)

| Theme | Enhancement |
|-------|-------------|
| Multi-Runtime | Python, Go, JVM adapters |
| Progressive Delivery | Canary, automated rollback on SLO breach |
| Observability | Structured event streaming, OpenTelemetry tracing |
| Policy | OPA integration, admission control gates |
| Supply Chain | Artifact signing (Cosign), provenance attestations |
| Scalability | Sharded registry, multi-cluster scheduling |

---

## 10. Local Development Environment

Detailed procedures are specified in `DEVELOPMENT.md`. A provisioning and verification script `dev.sh` automates environment bootstrap (Rust toolchain, Docker, MicroK8s, PostgreSQL container) and readiness checks.

Quick Start:
1. Ensure Linux host with Docker + Snap available.
2. Execute `./dev.sh bootstrap` (installs or configures missing components where feasible).
3. Run `./dev.sh verify` to confirm environment readiness.

---

## 11. Contributing Guidelines
* Adopt conventional commits (`feat:`, `fix:`, `docs:`, `refactor:` etc.).
* Run `cargo fmt` and `cargo clippy --all-targets --all-features -D warnings` pre‑PR.
* Provide architectural rationale in PR descriptions for substantial changes.
* Maintain backward compatibility for any published CLI flags until a deprecation path is documented.

---

## 12. Licensing & Ownership
Internal proprietary platform (license designation TBD). All code, documentation, and artifacts are confidential. External distribution prohibited without explicit authorization.

---

## 13. Contact & Operational Support
* Architecture Lead: (TBD)
* Platform Engineering Channel: (internal) `#aether-engine`
* Incident Escalation: On-call rotation (TBD)

---

## 14. Document History

| Version | Date | Author | Notes |
|---------|------|--------|-------|
| 1.0 (MVP Draft) | 2025-09-19 | Initial Compilation | First canonical English architecture document |
