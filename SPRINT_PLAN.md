# AetherEngine – Sprint Plan to 100% MVP

Plan date: 2025-10-13 – Target: MVP complete in 2 sprints (≈2 weeks)

## Goals
- Close functional gaps to deliver a production‑ready MVP for Node.js runtime
- Produce measurable E2E deploy latency proving ≥80% improvement vs baseline

## Guiding Principles
- Production‑first: prioritize operability (observability, RBAC, TLS)
- Risk burn‑down early: unblock DB/tests and log streaming first
- Definition of Done (DoD): code+tests+docs+CI; demoable E2E path; no TODOs

---
## Sprint 1 (Week 1): Operability & Core DX

Epic A: Log streaming end‑to‑end
- A1 Implement GET /apps/{app}/logs with K8s pod log stream (tail/stream)
  - Details: use kube‑rs; label selector app=<name>, follow=true, tail_lines=100
  - Add JSON line framing + optional plain text; WebSocket upgrade if available
  - DoD: integration tests (mock‑kube feature); CLI `aether logs` works locally
  - Est: 5 pts
- A2 Robustness: handle multiple pods, container selection, time filters (since)
  - DoD: e2e tests simulate 2 pods; CLI supports --since / --container
  - Est: 3 pts

Epic B: Helm/Kustomize & RBAC/SA for dev‑hot
- B1 Helm chart: control‑plane (Deployment/Service/Ingress), ConfigMap, Secrets
  - Values: DATABASE_URL, tokens, storage cfg, feature flags; health checks
  - DoD: helm template + lint; minikube/microk8s install doc
  - Est: 5 pts
- B2 ServiceAccount "aether-dev-hot" + Role/RoleBinding minimal permissions
  - Access: get/watch pods, read annotations; fetch from S3 if needed
  - DoD: kubectl auth can‑i checks; YAMLs tested in cluster
  - Est: 3 pts

Epic C: Test stability – DB/testcontainers
- C1 CI pipeline matrix: with/without Docker; set DATABASE_URL or use containers
  - DoD: control‑plane tests pass in CI; harness respects vars; retry flake guards
  - Est: 3 pts
- C2 Makefile targets: `make test-ci` that configures env correctly
  - DoD: docs updated; STATUS references consistent
  - Est: 1 pt

Epic D: Base image pipeline
- D1 Dockerfile aether-nodejs:20-slim + non-root user, patched
  - DoD: build locally; image scanned (grype/trivy) w/ zero critical vulns
  - Est: 3 pts
- D2 GH Actions workflow to build/push image (ghcr) w/ tags
  - DoD: automatic patch rebuild monthly; SBOM attach (cosign attest optional)
  - Est: 2 pts

Epic E: CLI polish
- E1 `aether logs` consume new logs API; flags: --app, --since, --follow
  - DoD: unit + integration tests; graceful reconnect
  - Est: 2 pts

Sprint 1 Exit Criteria
- Logs e2e usable from CLI
- Helm chart deploys control‑plane; SA/RBAC present; CI tests green
- Base image built and published

---
## Sprint 2 (Week 2): E2E Performance & Governance

Epic F: E2E smoke deploy + metrics
- F1 Sample app (examples/sample-node) polish; npm start readiness
  - DoD: repo sample works with `aether deploy`
  - Est: 2 pts
- F2 Smoke script: measure code→artifact→upload→deploy latency
  - Capture: sizes, throughput, k8s rollout timings; write JSON report
  - DoD: baseline vs MVP reduction ≥80% documented
  - Est: 5 pts

Epic G: Security/TLS & policy switches
- G1 Ingress TLS for control-plane (self-signed for dev); 
  - DoD: helm values to enable TLS; docs for certs; curl over https works
  - Est: 3 pts
- G2 Auth hardening: token rotation and scopes; limit origins (CORS)
  - DoD: tests for unauthorized/forbidden; docs for rotation procedure
  - Est: 3 pts

Epic H: SBOM/Provenance enforcement hardening
- H1 CLI CycloneDX by default; fallback legacy behind flag
  - DoD: control-plane validates manifest_digest consistency reliably
  - Est: 2 pts
- H2 Provenance generation path: sync flag + timeout behavior documented
  - DoD: tests pass w/ AETHER_REQUIRE_PROVENANCE=1
  - Est: 2 pts

Epic I: Docs & runbooks
- I1 Operator guide: install, configure MinIO/Postgres, deploy sample
  - Est: 2 pts
- I2 Troubleshooting playbook (common failures, quotas, retention, SSE)
  - Est: 2 pts

Sprint 2 Exit Criteria
- Demonstrated ≥80% deploy time reduction with report
- TLS enabled path available; enforcement toggles documented
- Docs complete; STATUS updated to 100%

---
## Dependencies & Risks
- Kubernetes cluster access (microk8s/minikube) for logs/API tests
- Docker/Podman availability for CI and testcontainers
- S3/MinIO endpoint for presign/multipart end-to-end validation

## Team Allocation (suggested)
- Person A: Epics A, E
- Person B: Epics B, C
- Person C: Epics D, F
- Person D: Epics G, H, I

## Tracking & Definition of Done (DoD)
- Each task requires: code + unit/integration tests + docs updates + CI green
- Add labels: `mvp`, `sprint-1`/`sprint-2`, `good-first` for smalls
- Weekly demo: end of sprint review with E2E demo
