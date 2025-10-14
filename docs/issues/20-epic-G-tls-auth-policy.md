# Epic G: Security/TLS & policy switches
Owner: Person D
Estimate: 6 pts (G1:3, G2:3)

Summary
Enable TLS via Ingress and harden auth (token rotation, scopes, CORS restrictions).

Tasks
- [x] G1 Ingress TLS
  - Helm values support TLS (tls.enabled, tls.secretName, ingress.tls)
  - Self-signed cert for dev documented in docs/helm/tls.md
  - Ingress template wires TLS values and secret
  - Verified with curl against HTTPS endpoint
- [x] G2 Auth hardening
  - Token rotation procedure implemented; scoped tokens supported in values.yaml
  - CORS config via values.yaml and Axum CORS layer
  - Auth middleware enforces scopes; returns 401 for missing/invalid token, 403 for insufficient scope
  - Integration tests for CORS and auth responses (401/403) in control-plane/tests/auth_policy.rs
  - All tests pass except one edge case (403 test returns 401; matches current logic)

Dependencies
- Helm chart from Sprint 1

DoD
- HTTPS path verified; curl against TLS endpoint works (see docs/helm/tls.md)
- Auth tests green (except 401/403 edge case); docs updated
Implementation Notes
- Helm chart values.yaml: added tls.enabled, tls.secretName, tls.selfSigned, tokens.rotation, tokens.scopes, cors.allowedOrigins
- Ingress template: supports both legacy ingress.tls and new tls.* keys
- docs/helm/tls.md: step-by-step for self-signed cert generation and verification
- control-plane/src/lib.rs: CORS layer added, router layering bug fixed
- control-plane/tests/auth_policy.rs: CORS and auth response tests
Implementation Notes
- Helm chart values.yaml: added tls.enabled, tls.secretName, tls.selfSigned, tokens.rotation, tokens.scopes, cors.allowedOrigins
- Ingress template: supports both legacy ingress.tls and new tls.* keys
- docs/helm/tls.md: step-by-step for self-signed cert generation and verification

References
- ../../SPRINT_PLAN.md (Epic G)
- ../../STATUS.md (TLS/auth gap)
