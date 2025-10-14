# Epic F: E2E smoke deploy + metrics
Owner: Person C
Estimate: 7 pts (F1:2, F2:5)

Summary
Polish sample app and implement a smoke script capturing code→artifact→upload→deploy latency with JSON report.

Tasks
- [x] F1 Sample app polish
  - Ensure examples/sample-node works with aether deploy
  - Readiness and simple endpoint for validation
- [x] F2 Smoke script & report
  - Capture timings: pack, upload, k8s rollout
  - Produce JSON + markdown summary; store in artifacts
  - Baseline vs MVP comparison ≥80% reduction

Dependencies
- Logs/Helm/RBAC from Sprint 1

DoD
- Script runs locally/CI against minikube/microk8s
- Report published in CI artifacts; README snippet updated

Status
- Done. Sample app and smoke harness added; CI publishes dry-run reports as artifacts.

Implementation notes
- Sample app: examples/sample-node/
  - index.js: HTTP server with `/ready`, `/` and `/healthz` endpoints
  - package.json: minimal metadata and `start` script
- Smoke script: scripts/smoke_e2e.sh
  - Dry-run support via `SMOKE_DRY_RUN=1`; emits JSON to stdout and writes Markdown summary when `SMOKE_MARKDOWN_OUT` is set
  - Fields: pack_ms, upload_ms, rollout_ms, total_ms, reduction_pct (vs static baseline env)
- CI workflow: .github/workflows/e2e-smoke.yml
  - Runs smoke in dry-run; uploads `smoke-report.json` and `smoke-summary.md` artifacts
- README updated with an "E2E Smoke" snippet showing local dry-run usage

References
- ../../SPRINT_PLAN.md (Epic F)
- ../../STATUS.md (E2E metrics gap)
