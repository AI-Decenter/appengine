# Epic D: Base image pipeline (aether-nodejs:20-slim)
Owner: Person C
Estimate: 5 pts (D1:3, D2:2)

Summary
Provide a hardened Node.js base image used by deployments; automate build/publish and security scanning.

Tasks
- [ ] D1 Dockerfile and local build
  - Non-root user, minimal packages, correct CA certs
  - Scan with trivy/grype; 0 critical vulns
- [ ] D2 CI workflow
  - Build & push to GHCR; tags by date/patch version
  - Monthly rebuilds; SBOM attach; (optional) cosign attest

Dependencies
- CI credentials for GHCR

DoD
- Image published; README with usage
- Vulnerability scan reports attached

References
- ../../SPRINT_PLAN.md (Epic D)
- ../../STATUS.md (Base image gap)
