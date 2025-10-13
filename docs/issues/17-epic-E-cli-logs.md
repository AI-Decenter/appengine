# Epic E: CLI polish – logs command
Owner: Person A
Estimate: 2 pts (E1)

Summary
Expose aether logs command consuming the new logs API with common UX flags.

Tasks
- [ ] E1 Implement `aether logs`
  - Flags: --app, --follow, --since, --container, --format=json|text
  - Graceful reconnect; colorize by pod/container (optional)
  - Unit + integration tests (mock server)

Dependencies
- Epic A endpoint in control-plane

DoD
- CLI command functional; documented in --help and README
- Tests green

References
- ../../SPRINT_PLAN.md (Epic E)
- ../../STATUS.md (Logs gap)
- crates/aether-cli
