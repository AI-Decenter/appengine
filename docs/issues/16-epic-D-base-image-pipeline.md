# Epic D: Base image pipeline (aether-nodejs:20-slim)
Owner: Person C
Estimate: 5 pts (D1:3, D2:2)

Summary
Provide a hardened Node.js base image used by deployments; automate build/publish and security scanning.

Tasks
- [x] D1 Dockerfile and local build
  - Non-root user, minimal packages, correct CA certs
  - Scan with trivy/grype; 0 critical vulns
- [x] D2 CI workflow
  - Build & push to GHCR; tags by date/patch version
  - Monthly rebuilds; SBOM attach; (optional) cosign attest

Dependencies
- CI credentials for GHCR

DoD
- Image published; README with usage
- Vulnerability scan reports attached

Status
- Done. Base image implemented, scanned, and CI-published with gates.

Implementation notes
- Dockerfile: images/aether-nodejs/20-slim/Dockerfile
  - Based on node:20-bookworm-slim, adds ca-certificates and dumb-init, runs apt-get upgrade, cleans APT cache, and runs as non-root user.
  - npm upgraded to latest to reduce known HIGH vulnerabilities while keeping the image slim.
- README: images/aether-nodejs/20-slim/README.md (usage and hardening notes)
- Make targets: base-image-build, base-image-scan, base-image-sbom, base-image-push (documented in Makefile)
- CI workflow: .github/workflows/base-image.yml
  - Builds local image, runs Trivy gating (HIGH/CRITICAL) before push, runs Grype as non-blocking informational scan, generates SBOM, uploads SARIF and summary artifacts, then pushes to GHCR on success. Scheduled monthly rebuilds included.
  - Allowlists: .trivyignore and security/grype-ignore.yaml supported by workflow.
  - Findings summary artifact: trivy-findings.txt; summary also echoed in job logs for quick triage.
- Tagging: uses standard tags including 20-slim, semver/date variants per workflow metadata.
- Publishing: pushed to GHCR under the repo’s owner namespace as configured in the workflow.

References
- ../../SPRINT_PLAN.md (Epic D)
- ../../STATUS.md (Base image gap)
