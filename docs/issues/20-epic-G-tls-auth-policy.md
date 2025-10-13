# Epic G: Security/TLS & policy switches
Owner: Person D
Estimate: 6 pts (G1:3, G2:3)

Summary
Enable TLS via Ingress and harden auth (token rotation, scopes, CORS restrictions).

Tasks
- [ ] G1 Ingress TLS
  - Helm values to enable TLS; self-signed for dev
  - Docs for cert generation and verification
- [ ] G2 Auth hardening
  - Token rotation procedure; scoped tokens
  - Limit origins (CORS); tests for 401/403 cases

Dependencies
- Helm chart from Sprint 1

DoD
- HTTPS path verified; curl against TLS endpoint works
- Auth tests green; docs updated

References
- ../../SPRINT_PLAN.md (Epic G)
- ../../STATUS.md (TLS/auth gap)
