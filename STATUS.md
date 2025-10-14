# AetherEngine – Báo cáo trạng thái MVP (v1.0)

Cập nhật ngày: 2025-10-13 — Nhánh hiện tại: feat/complete-aether-engine-mvp

## 1) Tóm tắt nhanh và % hoàn thành

- Mục tiêu MVP: PaaS nội bộ cho Node.js, build phía client (CLI), artifact upload (S3/MinIO), Control Plane (Axum + SQLx + Postgres), Data Plane (K8s) với init/sidecar tải artifact và chạy Node.
- Đánh giá tổng thể: ~75–80% hoàn thành.
  - Kỹ thuật: ~75–80% — CLI và Control Plane gần như đủ, S3 presign/two-phase/multipart, K8s apply (có dev-hot). Thiếu log streaming thực chiến, chart/SA/RBAC hoàn chỉnh, operator mới CRD.
  - Sản phẩm: ~70–80% — Luồng code → deploy hoạt động (CLI deploy + Control Plane APIs). Cần base image chính thức, Helm/K8s manifests đầy đủ, “logs” end-to-end.
  - Kinh doanh: ~50–60% — Bench packaging/streaming có sẵn nhưng chưa có số liệu E2E thực tế chứng minh ≥80% giảm thời gian deploy.

## 2) Kiến trúc đã triển khai

- Aether CLI (Rust)
  - Tự phát hiện dự án Node (package.json), chạy npm/yarn/pnpm install/prune (production), đóng gói source + node_modules thành app-<sha256>.tar.gz.
  - Tính sha256 streaming, sinh manifest per-file, SBOM (legacy hoặc CycloneDX 1.5), ký Ed25519 (optional AETHER_SIGNING_KEY).
  - Upload artifact:
    - Legacy multipart: POST /artifacts (đã đánh dấu deprecated).
    - Chuẩn 2 pha: /artifacts/presign → PUT lên S3 → /artifacts/complete (HEAD verify size/metadata), hỗ trợ multipart (init/presign-part/complete), idempotency, quota, retention.
  - Triển khai: POST /deployments với artifact_url/storage_key, có tùy chọn dev-hot.
  - Tối ưu: progress bar cho upload lớn, cache node_modules theo lockfile + NODE_VERSION, benchmark packaging/streaming kèm baseline.

- Control Plane (Rust + Axum + SQLx + Postgres)
  - API: health/ready/startupz, artifacts (legacy + presign/complete + multipart + meta + HEAD), deployments (list/get/create/patch), apps (list/create + public keys), logs (stub), provenance/SBOM/manifest (upload, enforce khi bật), metrics Prometheus, OpenAPI JSON + Swagger.
  - Auth/RBAC: Bearer token qua env (AETHER_API_TOKENS), guard Admin cho endpoints ghi; middleware trace id, request id, HTTP metrics.
  - Storage: mock backend (mặc định) và S3 backend (feature `s3`) với presign PUT, HEAD size/metadata, remote hashing có retry/backoff, SSE AES256/KMS, endpoint MinIO path-style.
  - K8s apply Deployment (kube-rs):
    - Non dev-hot: init container tải artifact, sha256 verify, giải nén; main container chạy node server.js.
    - Dev-hot: sidecar “fetcher” poll/watch annotations để tải artifact mới, verify checksum, hot-refresh; supervisor script + readiness drain.
  - Migrations Postgres: bảng applications, artifacts, deployments, public_keys, … + cột mở rộng (signature, provenance flags, manifest digest, idempotency_key, multipart_upload_id…).
  - Metrics: counters/gauges/histograms bao phủ upload lifecycle, multipart, quotas, HTTP; background tasks GC pending/failed deployments và cập nhật gauge coverage.

- Operator (Rust + kube)
  - CRD AetherApp (spec.image, spec.replicas, status), tool crd-gen sinh YAML.
  - Chưa có controller reconciliation đầy đủ (tương lai).

- K8s manifests
  - control-plane: Deployment + Service (namespace aether-system).
  - CRD AetherApp, ví dụ secret pubkey dev-hot; còn thiếu SA/RBAC cho serviceAccountName "aether-dev-hot".

## 3) Kết quả build/lint/test (tại môi trường local hiện tại)

- Build: PASS
  - `cargo build --workspace` thành công.
- Lint/Clippy: PASS
  - `cargo clippy --workspace --all-targets --all-features` không lỗi.
- Tests: PARTIAL FAIL
  - CLI: PASS (nhiều test đóng gói/stream/SBOM/JSON output xanh).
  - Control Plane: FAIL trong môi trường này do PoolTimedOut (DATABASE_URL trỏ Postgres không tồn tại). Theo README, nếu không đặt DATABASE_URL và có Docker, test harness sẽ spin-up Postgres qua testcontainers và dự kiến xanh. Các test S3/MinIO được feature-gated và cần môi trường MinIO để chạy.

Ghi chú chạy test Control Plane:
- Cách A: Bật Docker và bỏ DATABASE_URL (harness dùng testcontainers Postgres).
- Cách B: Tự cấp Postgres local (Makefile `make db-start`) rồi `DATABASE_URL=... cargo test -p control-plane`.

## 4) Tính năng đã hoàn thiện

- CLI
  - Detect NodeJS, install/prune production, pack artifact, manifest, SBOM (legacy/CycloneDX), ký Ed25519 (optional), upload 2 pha + multipart, tạo deployment.
  - JSON output ổn định (deploy --format json), cache node_modules, benches và baseline.
  - SBOM/Provenance: CycloneDX mặc định; `--legacy-sbom` để dùng định dạng nội bộ; hỗ trợ tạo provenance khi bật `AETHER_REQUIRE_PROVENANCE`, timeout hiển thị qua `AETHER_PROVENANCE_TIMEOUT_MS` (dry-run JSON có field `note`).
- Control Plane
  - Artifact ingestion (legacy + presign/complete + multipart), idempotent, quota/retention; HEAD existence; meta.
  - Verification: size/metadata digest; remote full hash (small object, optional, có giới hạn bytes và retry/backoff).
  - Deployments: create/list/get/patch; trích digest từ URL/stored artifacts; verify chữ ký nếu cung cấp; enforce SBOM/provenance (qua env flags).
  - K8s apply (mock-kube cho test) bao gồm dev-hot sidecar khá chi tiết (checksum, backoff, anomaly guard, readiness drain).
  - OpenAPI + Swagger; nhiều metrics Prometheus sẵn sàng scrape.
- Storage
  - Mock backend (không cần mạng) và S3 backend đầy đủ presign/multipart.
- DB/Migrations
  - Migrations nhiều lần, khớp tài liệu; có cột mở rộng cho idempotency/multipart/provenance.

## 5) Hạng mục còn thiếu/đang dở

- Logs end-to-end
  - API `GET /apps/{app}/logs` hiện là stub; chưa tích hợp log aggregator hoặc stream từ Kubernetes.
- Helm/Kustomize & RBAC/SA
  - Thiếu chart/kustomize để triển khai control-plane, khai báo SA/RBAC "aether-dev-hot", secrets (DB URL, tokens, pubkey), ingress.
- Operator
  - Mới có CRD; thiếu controller reconcile logic để quản lý tài nguyên AetherApp.
- Base image runtime
  - `aether-nodejs:20-slim` đang được tham chiếu; cần pipeline build/publish + security patching.
- CI/CD số liệu
  - Bench có baseline, nhưng cần thiết lập CI chạy bench so sánh, và tạo báo cáo E2E deploy latency.
- Ổn định test Control Plane
  - Bảo đảm môi trường CI có Docker (testcontainers) hoặc Postgres dịch vụ; tránh PoolTimedOut.
- TLS/Ingress
  - Cần cấu hình ingress/gateway (sản xuất) để đảm bảo HTTPS cho control-plane và luồng artifact.

## 6) Rủi ro và nợ kỹ thuật

- Phụ thuộc môi trường test
  - Control-plane tests dễ fail nếu không có Postgres hoặc Docker; cần quy ước rõ trong CI.
- Thiếu log streaming thực tế
  - Trải nghiệm "aether logs" chưa trọn vẹn; ảnh hưởng DX.
- SA/RBAC dev-hot
  - Manifest tham chiếu serviceAccountName nhưng chưa có YAML + policy → rủi ro apply fail.
- Base image & supply chain
  - Cần quy trình build/publish, vuln scan định kỳ, auto patch.
- Chi phí S3 remote hash
  - Tải object để băm có thể tốn băng thông; đã kiểm soát bằng cờ và ngưỡng, nhưng cần monitor chi phí.

## 7) Next steps đề xuất để “MVP-ready”

1) Hoàn thiện “logs”
- Tích hợp lấy logs từ Kubernetes bằng label selector; hỗ trợ stream (WebSocket/chunked) + test mock-kube.

2) Helm/Kustomize + RBAC/SA
- Tạo chart/kustomize cho control-plane (Deployment/Service/Ingress), ServiceAccount "aether-dev-hot", Role/RoleBinding, secrets (DB URL, tokens, pubkey), config map.

3) Base image runtime
- Hoàn thiện Dockerfile aether-nodejs:20-slim, pipeline build/publish, lịch patch bảo mật.

4) E2E script và số liệu
- Kịch bản “smoke deploy” (apps/sample-node) chạy trên microk8s/minikube; đo E2E deploy latency; so sánh baseline để chứng minh 80% giảm.

5) Ổn định test harness
- Trong CI: nếu runner có Docker → ưu tiên testcontainers; nếu không → spin Postgres dịch vụ trước job. Điều phối biến DATABASE_URL.

6) Bảo mật/TLS
- Chuẩn hóa ingress TLS; chuẩn hóa xác thực (token rotate), RBAC chi tiết; policy quotas/retention theo môi trường.

## 8) Cách chạy nhanh (tham khảo)

- Build workspace:
  - `cargo build --workspace`
- Lint:
  - `cargo clippy --workspace --all-targets --all-features`
- Test CLI:
  - `cargo test -p aether-cli`
- Test Control Plane (cần DB):
  - Có Docker, không đặt DATABASE_URL → harness dùng testcontainers Postgres.
  - Hoặc bật Postgres local rồi chạy: `DATABASE_URL=postgres://aether:postgres@localhost:5432/aether_dev cargo test -p control-plane`.
- Makefile tiện ích:
  - `make db-start` (bật Postgres local container), `make test`, `make schema-drift`.

---
Tài liệu liên quan: `README.md`, `DEVELOPMENT.md`, `crates/control-plane/README.md`, `k8s/`, `crates/aether-cli/benches/`.