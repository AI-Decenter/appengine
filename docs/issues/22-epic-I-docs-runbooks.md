# Epic I: Docs & runbooks
Owner: Person D
Estimate: 4 pts (I1:2, I2:2)

Summary
Provide clear operator documentation and troubleshooting runbook.

Tasks
- [x] I1 Operator guide
  - Install, configure MinIO/Postgres, deploy sample
- [x] I2 Troubleshooting playbook
  - Common failures (quotas, retention, SSE, DB, S3)

Dependencies
- Features stabilized in Sprint 1/2

DoD
- Docs reviewed; linked from README and STATUS; versioned with sprint tags

Artifacts
- Operator Guide: `docs/operator-guide.md`
- Troubleshooting: `docs/troubleshooting.md`

Test
- `tests/epic_i_test.sh` asserts docs presence and cross-links.

References
- ../../SPRINT_PLAN.md (Epic I)
- ../../STATUS.md (docs/runbooks gap)
