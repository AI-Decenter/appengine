# Epic E: CLI polish – logs command
Owner: Person A
Estimate: 2 pts (E1)

Summary
Expose aether logs command consuming the new logs API with common UX flags.

Tasks
 - [x] E1 Implement `aether logs`
  - Flags: --app, --follow, --since, --container, --format=json|text
  - Graceful reconnect; colorize by pod/container (optional)
  - [x] Unit + integration tests (mock server) — TDD tests written and passing

Dependencies
- Epic A endpoint in control-plane


Status Update — 2025-10-14 (Final)

- Implemented `aether logs` with flags: --app, --follow, --since, --container, --format=json|text, and optional --color.
- Graceful reconnect loop with backoff; mock mode via env for CI (no network).
- TDD tests green: help/flags, mock text/json, follow/reconnect, container/since flags.
- Added CLI README documenting flags and env overrides.

Quick try
- AETHER_LOGS_MOCK=1 aether logs --app demo --format text
- AETHER_API_BASE=http://localhost:8080 aether logs --app demo --follow --since 5m

DoD
- CLI command functional; documented in --help and README
- Tests green

References
- ../../SPRINT_PLAN.md (Epic E)
- ../../STATUS.md (Logs gap)
- crates/aether-cli
