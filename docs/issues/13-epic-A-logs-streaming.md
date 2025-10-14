# Epic A: Logs streaming end-to-end
Owner: Person A
Estimate: 8 pts (A1:5, A2:3)

Summary
Implement server-side log streaming from Kubernetes and integrate with CLI for a first-class DX (follow, tail, filters).

Tasks
- [x] A1 Implement GET /apps/{app}/logs with Kubernetes stream
  - [x] Control-plane route `/apps/{app}/logs` wired; mock streaming path produces ndjson/text
  - [x] Query params accepted: follow/tail_lines/since/container; content-type set (ndjson or text)
  - [x] CLI `aether logs` streams response (JSON/text) with flags; tests added; mock mode for CI
  - [x] Real Kubernetes streaming via kube-rs with labelSelector app=<name>
  - [ ] WebSocket upgrade behind feature flag; fallback to chunked transfer
  - [ ] Integration tests using mock-kube for logs endpoint (non-mock path)
- [ ] A2 Robustness: multi-pod, container selection, time filters
  - [x] Merge multiple pod streams, tagged by pod/container
  - [x] --container selection end-to-end; --since duration parsing and translation
  - [ ] Backpressure and reconnect loop for long-lived streams
  - [ ] Tests simulate 2 pods and container filtering

Dependencies
- Kubernetes access (minikube/microk8s) or mock-kube for tests
- RBAC: get/watch logs on pods (see Epic B)

DoD
- Control-plane endpoint streams logs; documented in OpenAPI
- CLI `aether logs` works with --follow/--since/--container, reconnection handled
- Integration tests green (mock-kube) and manual demo in a cluster

Status Update — 2025-10-13

- What’s done
  - Control-plane: `/apps/{app}/logs` handler implemented with a mock/test streaming path. Accepts follow/tail_lines/since/container; emits JSON lines (default) or text/plain. Marker header added for diagnostics.
  - CLI: `aether logs` implemented to stream HTTP response to stdout (JSON or text). Added a CLI-side mock mode toggled by env (AETHER_LOGS_MOCK or base :0) to keep CI green without network.
  - Tests: Control-plane library tests cover mock path; CLI unit + integration tests pass using mock server/mock mode.
- What’s pending
  - WebSocket upgrade behind feature flag; fallback to chunked transfer.
  - Robustness work: reconnect/backpressure for long-lived streams; mock-kube integration tests for non-mock path.
- Reference commits
  - CLI mock logs mode: 14a79af (main)
  - Control-plane K8s logs streaming: c66eecb (main)
- Quick try (dev)
  - Mock: set `AETHER_LOGS_MOCK=1` then run `aether logs`.
  - Real: set `AETHER_API_BASE` to control-plane URL and run `aether logs` (JSON by default; set `AETHER_LOGS_FORMAT=text` for plain text).

Status Update — 2025-10-14

- What’s done
  - Implemented real Kubernetes logs streaming in control-plane using kube-rs. Supports follow, tail_lines, since, and container query parameters. Streams NDJSON or text and merges multiple pod streams with pod/container metadata.
  - Exposed app_logs in OpenAPI so it appears in Swagger UI.
  - Kept mock mode for CI/tests and environments without cluster access.
- What’s pending
  - WebSocket upgrade path and reconnection/backpressure tuning for long-lived sessions.
  - Mock-kube based integration tests for the non-mock path, plus 2-pod simulation tests.

References
- ../../SPRINT_PLAN.md (Epic A)
- ../../STATUS.md (Logs gap)
- crates/control-plane (handlers/logs)
- crates/aether-cli (new logs command)
