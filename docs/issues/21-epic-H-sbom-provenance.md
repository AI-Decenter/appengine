# Epic H: SBOM/Provenance enforcement hardening
Owner: Person D
Estimate: 4 pts (H1:2, H2:2)

Summary
Default SBOM generation and reliable provenance enforcement path with clear timeouts and flags.

Tasks
- [ ] H1 CycloneDX default; legacy gated by flag
  - Control-plane validation of manifest_digest
- [ ] H2 Provenance generation behavior
  - Sync flag and timeout; tests with AETHER_REQUIRE_PROVENANCE=1

Dependencies
- Current SBOM/manifest implementation

DoD
- Tests green; docs on enforcement toggles

References
- ../../SPRINT_PLAN.md (Epic H)
- ../../STATUS.md (SBOM/provenance gap)
