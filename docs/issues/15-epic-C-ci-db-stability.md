# Epic C: Test stability – DB/testcontainers
Owner: Person B
Estimate: 4 pts (C1:3, C2:1)

Summary
Ensure control-plane tests run reliably in CI by provisioning Postgres or leveraging testcontainers correctly.

Tasks
- [x] C1 CI matrix and harness
  - If Docker available → use testcontainers (unset DATABASE_URL)
  - Otherwise → start managed Postgres service and set DATABASE_URL
  - Retry guards for PoolTimedOut
- [x] C2 Makefile and docs
  - Add `make test-ci`
  - Document env expectations in CONTRIBUTING/README

Dependencies
- CI runners with or without Docker

DoD
- CI pipeline green for control-plane tests
- Local dev instructions consistent with CI

Implementation Notes
- Added a DB strategy matrix to CI (both fast and full jobs): `matrix.db: [testcontainers, service]`.
  - testcontainers mode: unset `DATABASE_URL`, set `AETHER_FORCE_TESTCONTAINERS=1` and `AETHER_TEST_SHARED_POOL=0`.
  - service mode: set `DATABASE_URL=postgres://aether:postgres@localhost:5432/aether_test` with service Postgres.
- Test harness (`crates/control-plane/src/test_support.rs`):
  - Honors `AETHER_FORCE_TESTCONTAINERS`, `AETHER_DISABLE_TESTCONTAINERS`, and uses `DATABASE_URL` when provided.
  - Adds connection retry guards around pool connect to mitigate transient `PoolTimedOut`/refused.
  - Tuned acquire timeout and pool sizes for CI.
- Makefile: added `test-ci` target that auto-selects DB strategy based on Docker presence.

How to run locally
- With Docker: run control-plane tests using testcontainers
  - `AETHER_FORCE_TESTCONTAINERS=1 AETHER_TEST_SHARED_POOL=0 AETHER_FAST_TEST=1 cargo test -p control-plane -- --nocapture`
- Without Docker: start local Postgres and run tests
  - `make ensure-postgres`
  - `DATABASE_URL=postgres://aether:postgres@localhost:5432/aether_test AETHER_TEST_SHARED_POOL=0 cargo test -p control-plane -- --nocapture`

References
- ../../SPRINT_PLAN.md (Epic C)
- ../../STATUS.md (test stability gap)

References
- ../../SPRINT_PLAN.md (Epic C)
- ../../STATUS.md (test stability gap)
