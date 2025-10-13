# Epic A: Logs streaming end-to-end
Owner: Person A
Estimate: 8 pts (A1:5, A2:3)

Summary
Implement server-side log streaming from Kubernetes and integrate with CLI for a first-class DX (follow, tail, filters).

Tasks
- [ ] A1 Implement GET /apps/{app}/logs with Kubernetes stream
  - [x] Control-plane route `/apps/{app}/logs` wired; mock streaming path produces ndjson/text
  - [x] Query params accepted: follow/tail_lines/since/container; content-type set (ndjson or text)
  - [x] CLI `aether logs` streams response (JSON/text) with flags; tests added; mock mode for CI
  - [ ] Real Kubernetes streaming via kube-rs with labelSelector app=<name>
  - [ ] WebSocket upgrade behind feature flag; fallback to chunked transfer
  - [ ] Integration tests using mock-kube for logs endpoint (non-mock path)
- [ ] A2 Robustness: multi-pod, container selection, time filters
  - [ ] Merge multiple pod streams, tagged by pod/container
  - [ ] --container selection end-to-end; --since duration parsing and translation
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
  - Real Kubernetes streaming with kube-rs (labelSelector = app=<name>), including follow/tail/since and optional WebSocket mode.
  - Robustness work: multi-pod merge, container selection end-to-end, backpressure and reconnect behavior; mock-kube integration tests.
- Reference commits
  - CLI mock logs mode: 14a79af (main)
- Quick try (dev)
  - Mock: set `AETHER_LOGS_MOCK=1` then run `aether logs`.
  - Real: set `AETHER_API_BASE` to control-plane URL and run `aether logs` (JSON by default; set `AETHER_LOGS_FORMAT=text` for plain text).

References
- ../../SPRINT_PLAN.md (Epic A)
- ../../STATUS.md (Logs gap)
- crates/control-plane (handlers/logs)
- crates/aether-cli (new logs command)
