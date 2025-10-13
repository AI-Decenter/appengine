# Epic F: E2E smoke deploy + metrics
Owner: Person C
Estimate: 7 pts (F1:2, F2:5)

Summary
Polish sample app and implement a smoke script capturing code→artifact→upload→deploy latency with JSON report.

Tasks
- [ ] F1 Sample app polish
  - Ensure examples/sample-node works with aether deploy
  - Readiness and simple endpoint for validation
- [ ] F2 Smoke script & report
  - Capture timings: pack, upload, k8s rollout
  - Produce JSON + markdown summary; store in artifacts
  - Baseline vs MVP comparison ≥80% reduction

Dependencies
- Logs/Helm/RBAC from Sprint 1

DoD
- Script runs locally/CI against minikube/microk8s
- Report published in CI artifacts; README snippet updated

References
- ../../SPRINT_PLAN.md (Epic F)
- ../../STATUS.md (E2E metrics gap)
