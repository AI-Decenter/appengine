# Troubleshooting Playbook

This playbook lists common failure modes and how to diagnose and fix them.

## Quotas
- Symptom: 403 quota_exceeded on artifact complete.
- Check: per-app limits env vars `AETHER_MAX_ARTIFACTS_PER_APP`, `AETHER_MAX_TOTAL_BYTES_PER_APP`.
- Action: raise limits or delete old artifacts.

## Retention
- Symptom: older artifacts missing.
- Check: `AETHER_RETAIN_LATEST_PER_APP`.
- Action: increase retention or pin required artifacts.

## SSE
- Symptom: S3 PUT errors due to encryption params.
- Check: `AETHER_S3_SSE`, `AETHER_S3_SSE_KMS_KEY`.
- Action: set `AES256` or supply KMS key; verify bucket policy.

## Database
- Symptom: 503 service_unavailable or PoolTimedOut.
- Check: `DATABASE_URL`, DB reachability, migrations.
- Action: ensure DB is reachable; run migrations; increase pool size if needed.

## S3
- Symptom: presign or complete fails.
- Check: `AETHER_S3_ENDPOINT`, creds, bucket; network connectivity.
- Action: verify credentials; ensure bucket exists; check path-style vs virtual-hosted.

## Presign
- Symptom: 400 presign status or missing headers.
- Check client/CLI logs; ensure `AETHER_REQUIRE_PRESIGN` if desired.
- Action: retry; validate clock skew; inspect trace id in server logs.

## Multipart
- Symptom: multipart complete 4xx/5xx.
- Check: part size env `AETHER_MULTIPART_PART_SIZE_BYTES`, `AETHER_MULTIPART_THRESHOLD_BYTES`.
- Action: adjust thresholds; ensure ETags preserved; verify ordering.

## SBOM / Manifest / Provenance
- Symptom: missing SBOM or manifest digest mismatch; provenance required.
- Check: `AETHER_REQUIRE_PROVENANCE`, `AETHER_PROVENANCE_TIMEOUT_MS`.
- Action: re-generate SBOM/manifest; provide provenance; tune timeout.

## Logs
- Symptom: empty logs.
- Check: `AETHER_MOCK_LOGS`, K8s connectivity, labels; API `/apps/{app}/logs`.
- Action: enable mock for tests; configure K8s permissions.
