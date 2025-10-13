# Epic B: Helm/Kustomize & RBAC/SA
Owner: Person B
Estimate: 8 pts (B1:5, B2:3)

Summary
Package the control-plane for easy install and provide required ServiceAccount/Role/RoleBinding for dev-hot operations.

Tasks
- [ ] B1 Helm chart for control-plane
  - Deployment, Service, ConfigMap, Secrets
  - Values: DATABASE_URL, tokens, storage config, feature flags
  - Ingress (optional in Sprint 1, TLS in Sprint 2)
  - Add helm lint and template checks in CI
- [ ] B2 SA/RBAC for "aether-dev-hot"
  - Permissions: get/watch/list pods, logs; read annotations; optional secrets
  - Authorize limited namespace scope
  - kubectl auth can-i checks; sample YAMLs

Dependencies
- Control-plane container image published (existing CI)
- Cluster access for validation

DoD
- `helm install` deploys control-plane with minimal values
- SA/RBAC manifests exist and validated via auth can-i
- Documentation in README/Docs; example values.yaml provided

References
- ../../SPRINT_PLAN.md (Epic B)
- ../../STATUS.md (Helm/RBAC gap)
- k8s/control-plane-deployment.yaml (as source material)
