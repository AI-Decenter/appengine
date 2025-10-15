# Epic B: Helm/Kustomize & RBAC/SA
Owner: Person B
Estimate: 8 pts (B1:5, B2:3)

Status: Done (Merged to main)

Summary
Package the control-plane for easy install and provide required ServiceAccount/Role/RoleBinding for dev-hot operations.

Tasks
- [x] B1 Helm chart for control-plane
  - Deployment, Service, ConfigMap, Secrets
  - Values: DATABASE_URL, tokens, (extensible via env extras)
  - Ingress (optional in Sprint 1, TLS in Sprint 2)
  - Add helm lint and template checks in CI
- [x] B2 SA/RBAC for "aether-dev-hot"
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

Implementation notes
- Chart path: `charts/control-plane/`
- RBAC: namespaced Role `aether-dev-hot-reader` + RoleBinding `aether-dev-hot` bound to ServiceAccount `aether-dev-hot`
- Optional secret read: set `rbac.allowSecrets=true`

Validate RBAC (examples)
```
kubectl -n aether-system auth can-i --as=system:serviceaccount:aether-system:aether-dev-hot get pods
kubectl -n aether-system auth can-i --as=system:serviceaccount:aether-system:aether-dev-hot list pods
kubectl -n aether-system auth can-i --as=system:serviceaccount:aether-system:aether-dev-hot watch pods
kubectl -n aether-system auth can-i --as=system:serviceaccount:aether-system:aether-dev-hot get pods/log
# Optional (only if rbac.allowSecrets=true)
kubectl -n aether-system auth can-i --as=system:serviceaccount:aether-system:aether-dev-hot get secrets
```

Install (minimal)
```
helm upgrade --install aether charts/control-plane -n aether-system --create-namespace \
  --set env.DATABASE_URL=postgres://aether:postgres@postgres:5432/aether \
  --set env.TOKENS=t_admin:admin:alice
```

Further reading
- Helm chart usage guide: ../helm/README.md

References
- ../../SPRINT_PLAN.md (Epic B)
- ../../STATUS.md (Helm/RBAC gap)
- k8s/control-plane-deployment.yaml (as source material)
