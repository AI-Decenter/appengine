# Aether Control Plane – Operator Guide

This guide walks you through installing the control-plane, configuring Postgres and MinIO (S3-compatible), and deploying the sample app.

## Install

Prerequisites:
- Kubernetes cluster (minikube/microk8s/kind)
- kubectl, helm
- Postgres URL or willingness to run a demo Postgres
- MinIO (or S3-compatible) endpoint

### Helm install (dev)

Create a minimal `values.yaml`:

```
image:
  repository: ghcr.io/internal/aether/control-plane
  tag: 0.1.0
env:
  DATABASE_URL: postgres://aether:postgres@postgres:5432/aether
  AETHER_API_TOKENS: t_admin:admin:alice,t_reader:reader:bob
serviceAccount:
  create: true
  name: aether-dev-hot
rbac:
  create: true
  namespace: aether-system
  allowSecrets: false
```

Install chart:

```
helm upgrade --install aether charts/control-plane -n aether-system \
  --create-namespace -f values.yaml
```

## MinIO configuration

Point the control-plane to your MinIO/S3:
- `AETHER_S3_ENDPOINT`, `AETHER_S3_REGION`, `AETHER_S3_BUCKET`
- `AETHER_S3_ACCESS_KEY_ID`, `AETHER_S3_SECRET_ACCESS_KEY`
- Optional: `AETHER_S3_SSE` (AES256 or aws:kms), `AETHER_S3_SSE_KMS_KEY`

## Postgres configuration

Provide `DATABASE_URL` (PostgreSQL 15 recommended). Include TLS parameters if required.

```
postgres://USER:PASSWORD@HOST:5432/DB
```

Run migrations automatically via control-plane on startup.

## Deploy sample

From repo root:

```
cd appengine/examples/sample-node
"$PWD"/../../target/debug/aether-cli deploy --format json
```

Set `AETHER_API_BASE` to point the CLI to your control-plane.

## RBAC / ServiceAccount

Ensure ServiceAccount `aether-dev-hot` exists if using dev-hot mode; Role with least privileges for pods/logs.

## TLS (optional)

Configure ingress TLS; use self-signed certs in dev. Update `charts/control-plane/values.yaml` accordingly.
