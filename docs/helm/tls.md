# Helm TLS for Control Plane

This guide shows how to enable TLS for the control-plane Ingress and, for development, how to generate a self-signed certificate.

## Enable TLS via values

Two ways to configure TLS:

1) Provide an existing secret (recommended for real clusters)

values.yaml snippet:

- Set `ingress.enabled=true`
- Set `tls.enabled=true`
- Set `tls.secretName=aether-tls`

2) Legacy chart keys

Alternatively, continue using `ingress.tls` directly:

```yaml
ingress:
  enabled: true
  tls:
    - hosts: [aether.local]
      secretName: aether-tls
```

## Generate a self-signed cert (dev)

Use openssl to create a self-signed cert for `aether.local`:

```bash
openssl req -x509 -nodes -days 365 -newkey rsa:2048 \
  -keyout tls.key -out tls.crt \
  -subj "/CN=aether.local/O=aether" \
  -addext "subjectAltName=DNS:aether.local"
```

Create the secret in your namespace:

```bash
kubectl create secret tls aether-tls \
  --cert=tls.crt --key=tls.key
```

Update Helm values to reference the secret as shown above, then install/upgrade:

```bash
helm upgrade --install control-plane charts/control-plane \
  --set ingress.enabled=true \
  --set tls.enabled=true \
  --set tls.secretName=aether-tls
```

## Verify

```bash
curl -vk https://aether.local/health --resolve aether.local:443:127.0.0.1
```

You should see an HTTP 200 from the `/health` endpoint. For self-signed certs, curl will show certificate verification warnings unless you add the CA to your trust store or pass `-k`.
