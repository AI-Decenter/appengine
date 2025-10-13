# Control Plane Helm Chart

This page documents installation and configuration for the Aether control-plane Helm chart.

- Chart path: `charts/control-plane/`
- Default namespace: choose a namespace (examples use `aether-system`)

## Quick install

```
helm upgrade --install aether charts/control-plane -n aether-system --create-namespace \
  --set env.DATABASE_URL=postgres://aether:postgres@postgres:5432/aether \
  --set env.TOKENS=t_admin:admin:alice
```

## values.yaml reference

- image.repository (string): container image repo
- image.tag (string): version tag
- image.pullPolicy (string): IfNotPresent
- replicaCount (int): default 1
- env.DATABASE_URL (string|null): direct value; when null, deployment reads from Secret below
- env.TOKENS (string): CSV of `<token>:<role>:<user>`
- secret.create (bool): create Secret with DB url
- secret.name (string): name of Secret (default `aether-postgres`)
- secret.keys.url (string): key name within Secret (default `url`)
- serviceAccount.create (bool): create SA (default true)
- serviceAccount.name (string): SA name (default `aether-dev-hot`)
- rbac.create (bool): create Role + RoleBinding (default true)
- rbac.namespace (string): namespace for Role/Binding
- rbac.allowSecrets (bool): also allow `get` on secrets (default false)
- service.type (string): ClusterIP
- service.port (int): 80
- resources: requests/limits
- ingress.enabled (bool): disabled by default

## RBAC validation

```
kubectl -n aether-system auth can-i --as=system:serviceaccount:aether-system:aether-dev-hot get pods
kubectl -n aether-system auth can-i --as=system:serviceaccount:aether-system:aether-dev-hot list pods
kubectl -n aether-system auth can-i --as=system:serviceaccount:aether-system:aether-dev-hot watch pods
kubectl -n aether-system auth can-i --as=system:serviceaccount:aether-system:aether-dev-hot get pods/log
# If rbac.allowSecrets=true
kubectl -n aether-system auth can-i --as=system:serviceaccount:aether-system:aether-dev-hot get secrets
```

## CI

The repository CI will attempt to run `helm lint` and `helm template` if Helm is available.

## Troubleshooting

- Database URL: Either set `env.DATABASE_URL` or ensure a Secret exists with name `secret.name` and key `secret.keys.url`.
- Tokens: Set `env.TOKENS` to grant console/API access (`AETHER_API_TOKENS` env).
- Ingress: Enable and configure per your ingress controller; TLS can be added in a follow-up sprint.
