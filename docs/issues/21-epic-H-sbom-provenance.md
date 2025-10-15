# Epic H: SBOM/Provenance enforcement hardening
Owner: Person D
Estimate: 4 pts (H1:2, H2:2)

Summary
Default SBOM generation and reliable provenance enforcement path with clear timeouts and flags.

Tasks
- [x] H1 CycloneDX default; legacy gated by flag
  - Control-plane validation of manifest_digest (validated via mocked endpoint in test)
- [x] H2 Provenance generation behavior
  - Sync flag and timeout; tests with AETHER_REQUIRE_PROVENANCE=1

Dependencies
- Current SBOM/manifest implementation

DoD
- Tests green; docs on enforcement toggles

Implementation notes
- CLI
  - Default SBOM format set to CycloneDX (1.5 JSON). Legacy internal format available behind `--legacy-sbom`.
  - `aether deploy --dry-run --format json` now emits a machine-readable JSON object and writes mock files for manifest, SBOM, and provenance (when required). Timeout is surfaced via a `note: "timeout"` field.
  - Logging is routed to stderr to ensure stdout JSON remains clean for tooling.
  - Relevant files:
    - `crates/aether-cli/src/commands/deploy.rs` (SBOM/provenance logic; dry-run JSON)
    - `crates/aether-cli/src/commands/mod.rs` (flags: `--legacy-sbom`, `--no-sbom`)
    - `crates/aether-cli/src/logging.rs` (stderr logging)
- Tests (TDD)
  - `tests/epic_h_test.sh` validates:
    - CycloneDX default and legacy format via flag
    - Manifest digest validation against a mocked control-plane endpoint
    - Provenance presence when `AETHER_REQUIRE_PROVENANCE=1`
    - Timeout note when `AETHER_PROVENANCE_TIMEOUT_MS` is set
    - README contains docs for enforcement toggles
- Docs
  - README updated with a new section “SBOM and Provenance Controls” documenting:
    - `--legacy-sbom`, `--no-sbom`
    - `AETHER_REQUIRE_PROVENANCE`, `AETHER_PROVENANCE_TIMEOUT_MS`

Test status
- Local run: `tests/epic_h_test.sh` → PASS (dry-run/static checks)

Follow-ups
- Wire a real control-plane route for manifest digest validation (currently mocked in test).
References
- ../../SPRINT_PLAN.md (Epic H)
- ../../STATUS.md (SBOM/provenance gap)
