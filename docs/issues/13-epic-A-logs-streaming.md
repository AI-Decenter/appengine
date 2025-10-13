# Epic A: Logs streaming end-to-end
Owner: Person A
Estimate: 8 pts (A1:5, A2:3)

Summary
Implement server-side log streaming from Kubernetes and integrate with CLI for a first-class DX (follow, tail, filters).

Tasks
- [ ] A1 Implement GET /apps/{app}/logs with Kubernetes stream
  - kube-rs: labelSelector app=<name>
  - follow=true, tail_lines=100, since (optional)
  - Stream as JSON lines (default) with metadata; optional plain text
  - WebSocket upgrade if feature-flagged; fallback to chunked transfer
  - Tests: mock-kube feature; integration path
- [ ] A2 Robustness: multi-pod, container selection, time filters
  - Handle multiple pods (merge streams, tag by pod/container)
  - --container flag, --since duration
  - Backpressure and reconnect loop
  - Tests simulate 2 pods

Dependencies
- Kubernetes access (minikube/microk8s) or mock-kube for tests
- RBAC: get/watch logs on pods (see Epic B)

DoD
- Control-plane endpoint streams logs; documented in OpenAPI
- CLI `aether logs` works with --follow/--since/--container, reconnection handled
- Integration tests green (mock-kube) and manual demo in a cluster

References
- ../../SPRINT_PLAN.md (Epic A)
- ../../STATUS.md (Logs gap)
- crates/control-plane (handlers/logs)
- crates/aether-cli (new logs command)
