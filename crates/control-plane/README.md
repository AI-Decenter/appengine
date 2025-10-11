# Control Plane

## Auth & RBAC (Issue 10)

The API supports optional Bearer token authentication with simple RBAC:

- AETHER_AUTH_ENABLED=1 enables authentication
- Modes:
  - env (default): compare against AETHER_ADMIN_TOKEN and AETHER_USER_TOKEN
  - db: compare SHA-256(token) against users.token_hash in the database, using role column (admin|user)
- Public routes remain unauthenticated: /health, /readyz, /startupz, /metrics, /openapi.json, /swagger
- Admin-only endpoints include POST /deployments and PATCH /deployments/:id

Example (env mode):

```
export AETHER_AUTH_ENABLED=1
export AETHER_ADMIN_TOKEN=admin_secret
export AETHER_USER_TOKEN=user_secret
```

Tests cover env and db modes; set AETHER_DISABLE_K8S=1 in tests/dev to avoid contacting a real cluster.
