# Epic C: Test stability – DB/testcontainers
Owner: Person B
Estimate: 4 pts (C1:3, C2:1)

Summary
Ensure control-plane tests run reliably in CI by provisioning Postgres or leveraging testcontainers correctly.

Tasks
- [ ] C1 CI matrix and harness
  - If Docker available → use testcontainers (unset DATABASE_URL)
  - Otherwise → start managed Postgres service and set DATABASE_URL
  - Retry guards for PoolTimedOut
- [ ] C2 Makefile and docs
  - Add `make test-ci`
  - Document env expectations in CONTRIBUTING/README

Dependencies
- CI runners with or without Docker

DoD
- CI pipeline green for control-plane tests
- Local dev instructions consistent with CI

References
- ../../SPRINT_PLAN.md (Epic C)
- ../../STATUS.md (test stability gap)
