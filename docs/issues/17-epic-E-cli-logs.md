# Epic E: CLI polish – logs command
Owner: Person A
Estimate: 2 pts (E1)

Summary
Expose aether logs command consuming the new logs API with common UX flags.

Tasks
 - [ ] E1 Implement `aether logs`
  - Flags: --app, --follow, --since, --container, --format=json|text
  - Graceful reconnect; colorize by pod/container (optional)
  - [x] Unit + integration tests (mock server) — TDD tests written and passing

Dependencies
- Epic A endpoint in control-plane


Status Update — 2025-10-14

- TDD tests for `aether logs` written and passing: help/flags, mock text/json, follow/reconnect, container/since flags.

DoD
- CLI command functional; documented in --help and README
- Tests green

References
- ../../SPRINT_PLAN.md (Epic E)
- ../../STATUS.md (Logs gap)
- crates/aether-cli
