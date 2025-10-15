## Benchmarks (Performance Suite)

This repo includes a small performance suite to guard against regressions.

What we measure
- Packaging (duration in ms)
- Streaming upload (throughput in MB/s via a local mock server)

Outputs
- `crates/aether-cli/target/benchmarks/bench-pack.json`
- `crates/aether-cli/target/benchmarks/bench-stream.json`

Baselines (committed)
- Packaging: `crates/aether-cli/benches/baseline/bench-pack.json`
- Streaming: `crates/aether-cli/benches/baseline/bench-stream.json`

Regression policy
- CI emits warnings and exits non-zero when p95 degrades by > 20% vs baseline.

Run locally
```bash
cd appengine

# Optional determinism knobs
export RAYON_NUM_THREADS=2
export RUST_LOG=off

# Run benches
cargo bench -p aether-cli --bench pack_bench --bench stream_bench --quiet

# Compare to baselines
bash scripts/check-bench-regression.sh \
	crates/aether-cli/benches/baseline/bench-pack.json \
	crates/aether-cli/target/benchmarks/bench-pack.json
bash scripts/check-bench-regression.sh \
	crates/aether-cli/benches/baseline/bench-stream.json \
	crates/aether-cli/target/benchmarks/bench-stream.json
```

Update baselines
- After stabilizing on main, copy new JSON to the relevant file under `crates/aether-cli/benches/baseline/` and commit in a PR dedicated to baseline updates.
- Keep inputs fixed (payload size, chunk size, warm-up/sample counts) to reduce noise.

Troubleshooting
- If JSON files are missing, ensure the benches ran and that you’re looking under the crate-local path.
- For noisy results on laptops/VMs, pin CPU, close background workloads, and increase measurement time locally.

# AetherEngine (MVP v1.0)

![CI (Main)](https://github.com/askerNQK/appengine/actions/workflows/ci.yml/badge.svg)
![Feature CI](https://github.com/askerNQK/appengine/actions/workflows/feature-ci.yml/badge.svg)
![Coverage](https://img.shields.io/badge/coverage-internal--lcov-lightgrey)
<!-- Coverage badge is placeholder; replace with dynamic source (Codecov / shields endpoint) later. -->

> Platform Test Matrix: Linux (Ubuntu) + macOS
> * Linux: Full workspace (including Control Plane DB tests, migrations, schema drift, coverage, performance)
> * macOS: Full workspace including Control Plane (PostgreSQL 15 via Homebrew service)
> This ensures cross-platform parity for the CLI, operator, and control-plane.

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

### E2E Smoke (Deploy + Metrics)

Run a quick smoke flow to measure code→artifact→upload→(mock)rollout timings and produce JSON + Markdown summary:

```
# Dry-run locally (no cluster required)
SMOKE_DRY_RUN=1 SMOKE_MARKDOWN_OUT=smoke-summary.md ./scripts/smoke_e2e.sh sample-node > smoke-report.json

# Outputs:
# - smoke-report.json (machine-readable metrics)
# - smoke-summary.md  (human summary)
```

CI workflow `.github/workflows/e2e-smoke.yml` runs the dry-run and publishes artifacts.

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

## Operator docs and runbooks

- Operator Guide: `docs/operator-guide.md`
- Troubleshooting: `docs/troubleshooting.md`

### 3.3 SBOM and Provenance Controls

- Default SBOM format: CycloneDX 1.5 JSON. Pass `--legacy-sbom` to emit the internal legacy format instead (schema `aether-sbom-v1`).
- Disable SBOM generation entirely with `--no-sbom` (useful for quick iterations or constrained environments).
- Provenance enforcement: set environment variable `AETHER_REQUIRE_PROVENANCE=1` to require provenance generation during deploy flows. In dry-run mode, this will emit a minimal `.provenance.json` file path in the JSON output.
- Provenance timeout: `AETHER_PROVENANCE_TIMEOUT_MS=<millis>` can be set to enforce a maximum waiting time for provenance; when exceeded, the CLI will include a `note: "timeout"` field in JSON dry-run output.

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
## Helm install (Control Plane)

Quick start (dev):

1) Provide DB URL and optional tokens via values (or use existing Secret `aether-postgres` with key `url`).

Example minimal values.yaml:

```
image:
	repository: ghcr.io/internal/aether/control-plane
	tag: 0.1.0
env:
	DATABASE_URL: postgres://aether:postgres@postgres:5432/aether
	TOKENS: t_admin:admin:alice,t_reader:reader:bob
serviceAccount:
	create: true
	name: aether-dev-hot
rbac:
	create: true
	namespace: aether-system
	allowSecrets: false
```

Install:

```
helm upgrade --install aether charts/control-plane -n aether-system --create-namespace -f values.yaml
```

CI checks run `helm lint` and `helm template` if Helm is present on the runner.

RBAC notes:
- ServiceAccount `aether-dev-hot` is bound to a Role with least-privilege access to pods and pod logs in the target namespace.
- Optional secret read can be enabled with `rbac.allowSecrets=true`.

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

### 4.8 Extended Artifact Upload (Two-Phase + Multipart)

Two-phase single-part flow:
1. `POST /artifacts/presign` – obtain presigned PUT (or method NONE if duplicate already stored)
2. Client performs PUT directly to object storage (S3 / MinIO) using returned headers
3. `POST /artifacts/complete` – finalize (size & optional remote hash verification, quota + retention enforcement, idempotency)

Multipart flow (large artifacts) adds:
1. `POST /artifacts/multipart/init` – returns `upload_id` + `storage_key`
2. Loop: `POST /artifacts/multipart/presign-part` – presign each part (client uploads via PUT)
3. `POST /artifacts/multipart/complete` – supply list of `(part_number, etag)` pairs, finalize record

Idempotency: supply `idempotency_key` on complete endpoints; conflicting reuse across different digests is rejected with `409 idempotency_conflict`.

Quota Enforcement: configurable per-app limits on artifact count and cumulative bytes. Rejections return `403 quota_exceeded`.

Retention Policy: keep latest N stored artifacts per app; older rows deleted post-store (retention events emitted).

Server-Side Encryption (S3): set `AETHER_S3_SSE` to `AES256` or `aws:kms` (optionally `AETHER_S3_SSE_KMS_KEY`).

Remote Verification Toggles:
* Size: `AETHER_VERIFY_REMOTE_SIZE` (default on)
* Metadata digest: `AETHER_VERIFY_REMOTE_DIGEST` (default on)
* Full hash (small objects only): `AETHER_VERIFY_REMOTE_HASH` + `AETHER_REMOTE_HASH_MAX_BYTES`

### 4.9 Prometheus Metrics (Extended)

Existing (core) metrics plus newly added artifact lifecycle instrumentation:

Counters:
* `artifact_presign_requests_total` – presign attempts
* `artifact_presign_failures_total` – presign errors (backend/head failures)
* `artifact_complete_failures_total` – completion DB / logic errors
* `artifact_upload_bytes_total` – bytes of legacy (deprecated) direct uploads
* `artifact_digest_mismatch_total` – remote metadata/hash mismatches
* `artifact_size_exceeded_total` – rejected for per-object size limit
* `artifact_pending_gc_runs_total` / `artifact_pending_gc_deleted_total` – stale pending cleanup
* `artifact_events_total` – audit events written
* `artifact_legacy_upload_requests_total` – deprecated `/artifacts` hits
* `artifact_multipart_inits_total` – multipart session starts
* `artifact_multipart_part_presigns_total` – part presign calls
* `artifact_multipart_completes_total` – successful multipart completes
* `artifact_multipart_complete_failures_total` – multipart completion failures
* `artifact_quota_exceeded_total` – quota rejections

Gauges:
* `artifact_uploads_in_progress` – active legacy direct uploads
* `artifacts_total` – stored artifact rows (adjusted on insert/init)

Histograms:
* `artifact_upload_duration_seconds` – legacy direct upload wall time
* `artifact_put_duration_seconds` – client-reported PUT transfer duration (two-phase + multipart)
* `artifact_complete_duration_seconds` – server-side complete handler time
* `artifact_multipart_part_size_bytes` – distribution of part sizes (approximate; estimated at completion)
* `artifact_multipart_parts_per_artifact` – distribution of part counts per multipart artifact

Cardinality Guidance: all metrics intentionally have zero or minimal label cardinality (no per-app labels) to remain low cost at scale; future segmentation (e.g. per-app) would use dynamic metric families + allow lists.

### 4.10 Environment Variables (Artifact & Storage Subsystem)

Core limits & behavior:
* `AETHER_MAX_ARTIFACT_SIZE_BYTES` – reject complete if reported size exceeds (0=disabled)
* `AETHER_MAX_CONCURRENT_UPLOADS` – semaphore permits for legacy endpoint (default 32)
* `AETHER_PRESIGN_EXPIRE_SECS` – expiry for presigned URLs (default 900)
* `AETHER_REQUIRE_PRESIGN` – force presign before complete (`true|1`)

Quotas & retention:
* `AETHER_MAX_ARTIFACTS_PER_APP` – limit count per app (0/absent disables)
* `AETHER_MAX_TOTAL_BYTES_PER_APP` – cumulative byte quota per app
* `AETHER_RETAIN_LATEST_PER_APP` – keep N newest stored artifacts; delete older

Remote verification:
* `AETHER_VERIFY_REMOTE_SIZE` – enable size HEAD check (default true)
* `AETHER_VERIFY_REMOTE_DIGEST` – validate remote metadata sha256
* `AETHER_VERIFY_REMOTE_HASH` – fetch full object (<= `AETHER_REMOTE_HASH_MAX_BYTES`) and hash
* `AETHER_REMOTE_HASH_MAX_BYTES` – cap for remote hash download (default 8,000,000)

Multipart thresholds:
* `AETHER_MULTIPART_THRESHOLD_BYTES` – client selects multipart if artifact size >= threshold
* `AETHER_MULTIPART_PART_SIZE_BYTES` – desired part size (client buffer; default 8 MiB)

Storage/S3:
* `AETHER_STORAGE_MODE` – `mock` or `s3`
* `AETHER_ARTIFACT_BUCKET` – S3 bucket name (default `artifacts`)
* `AETHER_S3_BASE_URL` – mock base URL (for mock backend only)
* `AETHER_S3_ENDPOINT_URL` – custom S3 endpoint (MinIO / alternative)
* `AETHER_S3_SSE` – `AES256` | `aws:kms` (enables SSE)
* `AETHER_S3_SSE_KMS_KEY` – KMS key id/arn when using `aws:kms`

Pending GC:
* `AETHER_PENDING_TTL_SECS` – external GC driver: delete pending older than TTL (used by helper `run_pending_gc`)
* `AETHER_PENDING_GC_INTERVAL_SECS` – operator side scheduling hint (not yet wired)

Client / CLI related:
* `AETHER_MAX_CONCURRENT_UPLOADS` – legacy path concurrency limit
* `AETHER_API_BASE` – base URL used by CLI for API calls

All boolean style env vars treat `true|1` (case-insensitive) as enabled.

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
1. Ensure Linux host with Docker (and optionally Snap if using MicroK8s).
2. Option A (script): `./dev.sh bootstrap`

---

## 11. Testing (Control Plane)

Fast, reliable tests depend on sane DB pool and background settings. The harness in `crates/control-plane/src/test_support.rs` provides defaults that work well locally and in CI.

Key environment flags (defaults in tests):

- AETHER_DISABLE_BACKGROUND=1 – disables background loops (metrics refreshers, GC timers)
- AETHER_DISABLE_WATCH=1 – disables k8s watch tasks in tests
- AETHER_STORAGE_MODE=mock – uses a mock storage backend (no network)
- AETHER_FAST_TEST=1 – skips heavy external validations where supported
- AETHER_MAX_CONCURRENT_CONTROL=4 – limits DB-bound handler concurrency
- AETHER_TEST_MAX_CONNS=8 – Postgres pool max connections for tests

Optional:

- DATABASE_URL – Postgres connection string (preferred when Docker is not available)
- AETHER_FORCE_TESTCONTAINERS=1 – use testcontainers-backed Postgres for isolation

Recommended setups:

1) Local Postgres (no Docker):

```bash
export DATABASE_URL=postgres://user:pass@localhost:5432/aether_test
cargo test -p control-plane --tests -q
```

Optionally increase pool size slightly on fast machines:

```bash
AETHER_TEST_MAX_CONNS=12 cargo test -p control-plane --tests -q
```

2) Testcontainers (requires Docker):

```bash
AETHER_FORCE_TESTCONTAINERS=1 cargo test -p control-plane --tests -q
```

Run focused suites:

```bash
cargo test -p control-plane --test artifacts -q
cargo test -p control-plane --test upload_integrity -q
```

Notes:

- The test harness creates helpful indexes at startup to keep queries fast.
- Connection/lock timeouts are short to fail fast rather than hang; if your DB is slow, raise `AETHER_TEST_DB_ACQUIRE_TIMEOUT_SECS`.
- Background tasks are disabled in tests to avoid pool starvation.

### 10.1 Test Database Strategy (PostgreSQL)

Integration & migration tests now use a Docker ephemeral Postgres (via `testcontainers`) by default when `DATABASE_URL` is not set. This replaces the previous `pg-embed` binary extraction approach (which was fragile in CI with cached/corrupt archives). Behavior:

* If `DATABASE_URL` is defined, tests connect directly (database auto-created if absent).
* Else a container `postgres:15-alpine` is started once per test process; a database `aether_test` is created and migrations applied.
* Environment variables:
	* `AETHER_TEST_SHARED_POOL=1` – reuse a single connection pool across tests.
	* `AETHER_DISABLE_TESTCONTAINERS=1` – force failure if no `DATABASE_URL` (debug / hard fail mode).
	* `AETHER_TEST_PG_IMAGE=postgres:16-alpine` – override image.
* The harness exports `DATABASE_URL` after container startup so tests that expect it (e.g. schema checks) transparently work.
* First run pays the image pull cost; subsequent runs are typically <10s for schema tests (previously ~56s with fallback retries).

Rationale: deterministic startup, less custom retry logic, smaller maintenance surface vs. embedded binaries.
3. Option B (docker-compose):
	```bash
	docker compose up -d postgres minio
	export DATABASE_URL=postgres://aether:postgres@localhost:5432/aether_dev
	make test
	```
4. To just start DB for tests: `make db-start` or `./dev.sh db-start`
5. Run full suite: `make test`

docker-compose services (Postgres + MinIO) are defined in `docker-compose.yml` to enable S3-mode (`AETHER_STORAGE_MODE=s3`).
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
