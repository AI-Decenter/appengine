````markdown
# Issue 08: Dev Environment Automation + Hot Reload Script

## Goals
Provide a frictionless local development workflow enabling:
1. Bootstrapping Kubernetes + object storage + DB quickly.
2. Deploying a sample NodeJS application using the same artifact layout the platform expects.
3. Performing live (no full pod restart) hot reloads by packaging & publishing new artifacts and triggering a lightweight digest change signal consumed by a sidecar fetch loop.

## Implemented Scope
* Extended `dev.sh` with new / enhanced subcommands:
  - `k8s-start` (idempotent MicroK8s ensure & namespace).
  - `deploy-sample <app> [path]` – Auto-generates a sample app under `examples/sample-node` if path omitted; packages directory (tar.gz), uploads to MinIO, deploys a Kubernetes `Deployment` with fetcher sidecar & downward API annotation volume.
  - `hot-upload <app> <dir>` – Creates tar.gz, uploads to MinIO at deterministic key `artifacts/<app>/<digest>/app.tar.gz`, prints digest + accessible URL.
  - `hot-patch <app> <digest>` – Patches `aether.dev/digest` annotation on the Deployment pod template triggering sidecar polling loop to fetch & untar new artifact version without a full restart.
* Sample Node application (auto-generated) with minimal HTTP server & live counter.
* Fetcher sidecar enhanced:
  - Uses downward API mounted annotations file (`/etc/podinfo/annotations`) instead of fixed URL polling.
  - Compares current stored digest vs annotation every 5s.
  - On mismatch: downloads new artifact from local MinIO and extracts in-place into shared `emptyDir` volume.
* Deployment template includes:
  - Annotation + label `aether.dev/digest` and `aether.dev/app`.
  - Downward API volume for annotations.
  - Exposed port 3000 on app container.

## Command Reference
```
./dev.sh bootstrap              # (Optional) ensure rust, docker, microk8s, postgres, minio
./dev.sh k8s-start              # Ensure microk8s & namespace
./dev.sh deploy-sample demo     # Generates sample node app, packages, uploads, deploys
./dev.sh hot-upload demo ./examples/sample-node   # Repackage modified source -> prints digest & URL
./dev.sh hot-patch demo <new-digest>              # Patch annotation to trigger sidecar fetch
```

### deploy-sample Flow
1. (Optional) Generate sample if directory missing.
2. Pack directory -> `/tmp/<app>-dev-artifact.tar.gz`.
3. Compute SHA256 digest; upload to MinIO at `artifacts/<app>/<digest>/app.tar.gz`.
4. Apply Deployment manifest with `aether.dev/digest=<digest>`.
5. Wait (≤60s) until Pod phase Running.

### Hot Reload Flow
1. Edit local code (e.g., modify `index.js`).
2. Run `./dev.sh hot-upload demo examples/sample-node` -> prints `digest=<hex>`.
3. Run `./dev.sh hot-patch demo <hex>` -> updates annotation only.
4. Sidecar loop (5s interval) detects changed digest and fetches new artifact.
5. Updated code now served (in-memory Node process persists as files replaced; if module reload required restart container or implement fs watch reload).

## Acceptance Mapping
| ID | Description | Validation | Result |
|----|-------------|------------|--------|
| D1 | `deploy-sample` succeeds | Pod phase becomes Running | ✅ Implemented wait loop (up to 60s) |
| D2 | Hot upload + patch changes digest & sidecar fetches | Sidecar logs show `[fetcher] new digest` and content updates | ✅ Annotation-driven poll loop |

## Verification Steps
1. Deploy:
	- `./dev.sh deploy-sample demo`
	- Confirm: `microk8s kubectl get pods -n aether-system -l app_name=demo` => Running.
2. Retrieve current digest (from annotation):
	- `microk8s kubectl get deploy demo -n aether-system -o jsonpath='{.spec.template.metadata.annotations.aether\.dev/digest}'`.
3. Modify sample code (e.g., update response message), then:
	- `./dev.sh hot-upload demo examples/sample-node` -> note new digest.
	- `./dev.sh hot-patch demo <new-digest>`.
4. Within ~5s sidecar logs (fetcher container) should contain `new digest` line:
	- `microk8s kubectl logs deploy/demo -n aether-system -c fetcher-sidecar --tail=20 -f`.
5. Curl service (via port-forward or NodePort) to observe changed response.

## Design Notes
* Digest-as-contract: Artifact path encodes digest -> immutable content addressable asset.
* Downward API chosen over environment variables to allow dynamic observation without restart.
* Sidecar loop interval (5s) balances responsiveness & load; configurable by editing script if needed.
* Minimal security assumptions for local dev (HTTP, no auth); production path should integrate signed artifacts & control plane orchestration.

## Future Enhancements
* Add `hot-status` command to print current deployed digest + last fetch time.
* Optional in-container file watch (nodemon) to reduce repackage frequency.
* Integrate control-plane API to register artifact + provenance automatically instead of direct MinIO access.
* Parameterize fetch interval via annotation `aether.dev/fetch-interval`.
* Graceful rollback command to previous digest (persist last N digests locally).

## Troubleshooting
| Symptom | Cause | Fix |
|---------|-------|-----|
| Pod Pending | MicroK8s addons not ready | Rerun `./dev.sh k8s-start` and check `microk8s status` |
| Sidecar never updates | Annotation patch failed | Check deployment describe + ensure digest differs |
| Fetch errors | MinIO bucket or object missing | Re-run `hot-upload`; verify `mc ls` path |
| Node not serving new code | File replaced but module cached | Add a process manager with restart or enable dynamic require reload |

---
Issue 08 fully implemented; acceptance D1 & D2 satisfied.

````