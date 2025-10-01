````markdown
# Issue 05: Dev Hot Reload (Sidecar Fetch Loop)

## Ý tưởng
Cho phép lập trình viên cập nhật code mà không rebuild image: sidecar fetch định kỳ artifact mới nếu digest annotation thay đổi.

## Scope
* [x] Sidecar container `fetcher` (busybox wget + tar) loop: nếu annotation digest khác local -> tải & giải nén vào EmptyDir.
* [x] Lệnh CLI: `aether deploy --dev-hot` -> gửi `dev_hot=true` khi tạo deployment (control-plane tạo Deployment với sidecar + annotation `aether.dev/dev-hot=true`).
* [x] Graceful reload Node: dùng Node 20 `--watch` flag để tự restart khi file thay đổi (tối giản; có thể nâng cấp `nodemon` sau). 
* [x] Control-plane thêm trường `dev_hot` (transient via request) truyền xuống hàm `apply_deployment`.
* [x] Manifest builder: bỏ initContainer khi dev-hot; thay bằng sidecar fetcher polling pod own annotations 5s.
* [x] Tests: bổ sung unit test xác nhận sidecar tồn tại & annotation `aether.dev/dev-hot`.
* [ ] E2E test: cập nhật digest -> sidecar kéo bản mới trong ≤10s (cần cluster test harness).
* [x] Checksum verify trong sidecar loop (sha256sum -c trước extract) & configurable poll interval env `AETHER_FETCH_INTERVAL_SEC`.
* [x] Structured log markers `REFRESH_OK` / `REFRESH_FAIL reason=<...>` trong fetcher script để phục vụ metrics ingestion.
* [x] Metrics definitions (Prometheus): counters & histogram (`dev_hot_refresh_total`, `dev_hot_refresh_failure_total{reason}`, `dev_hot_refresh_latency_seconds`) + ingestion runtime (log tail) behind `AETHER_DEV_HOT_INGEST=1`.

## Acceptance
| ID | Mô tả | Kết quả |
|----|------|---------|
| H1 | Patch digest |  ≤10s code mới có hiệu lực (CHƯA E2E test tự động) |
| H2 | Digest không đổi | Không tải lại (logic: sidecar giữ CUR digest; CHƯA test tự động) |

## Test
* (Tương lai) Script `dev.sh` subcommand mô phỏng patch annotation hoặc dùng `kubectl annotate deployment <app> aether.dev/digest=sha256:<new>` để trigger.

## Đã triển khai (Summary)
Implemented Issue 05 foundations:
1. CLI flag `--dev-hot` (adds JSON field `dev_hot=true` in deployment create request).
2. Control-plane API chấp nhận `dev_hot` và truyền xuống `apply_deployment`.
3. K8s manifest logic: nếu `dev_hot=true` tạo sidecar `fetcher` thay cho initContainer đầu tiên. Sidecar loop:
	- Poll pod tài nguyên (self) mỗi 5s (dùng service account token + wget API server) để đọc annotations `aether.dev/digest` & `aether.dev/artifact-url`.
	- Nếu digest khác `CUR`, tải artifact, giải nén vào `/workspace`, cập nhật CUR.
4. Annotation `aether.dev/dev-hot=true` để debugging / introspection.
5. Unit test xác minh sidecar & annotation.
6. Checksum verification + configurable interval trong loop.
7. Structured log markers + metrics definitions + ingestion wiring (log tail watcher) + latency capture (ms -> histogram seconds).
	* Ghi chú: module ingestion hiện được feature-gate bằng `dev-hot-ingest` (mặc định OFF) để tránh tác động độ ổn định test; bật bằng `--features dev-hot-ingest` khi chạy control-plane.

## Giới hạn hiện tại
- Graceful reload cơ bản đã có qua `node --watch` (chưa hỗ trợ debounce tinh vi, chưa đảm bảo zero-downtime handshake).
- JSON parsing hiện cải thiện: bỏ `grep` chuỗi thô, dùng hàm `json_field` (sed) đơn giản – vẫn fragile nếu field order / escaping phức tạp; vẫn nên thay bằng helper binary.
- Backoff + jitter (exponential capped) đã thêm khi lỗi liên tiếp (download / checksum / extract / empty json).
- Metrics ingestion implemented (log tail). Remaining: resilience across pod restarts & multi-namespace support. (Hiện disabled by default qua feature flag.)
- Hạ tầng test Postgres đã chuyển sang Docker testcontainers (README 10.1); không ảnh hưởng trực tiếp nhưng cải thiện tốc độ và tính ổn định khi chạy suite với dev-hot flag.

## Next-Up / Future Enhancements
1. Upgrade reload strategy: optional switch to `nodemon` or custom wrapper for controlled graceful shutdown + readiness gating.
2. Replace sed-based `json_field` with tiny static Rust helper (proper JSON parse + error codes) to eliminate parsing fragility & escaping bugs.
3. Metrics resiliency: handle pod restarts (persist seen set) & multi-namespace ingestion; evaluate watch API instead of periodic list.
4. E2E integration test: patch digest -> assert updated content within ≤10s (automate latency measurement H1/H2 acceptance).
5. Switch sidecar polling to Kubernetes watch stream for lower latency + reduced API calls.
6. Security hardening: minimal RBAC (get pod), remove `--no-check-certificate`, projected short-lived token.
7. CLI convenience: `aether dev --hot` local incremental build + auto upload + patch digest.
8. Enhanced restart semantics: health gate (readinessProbe flip) during extract; only mark ready after REFRESH_OK.
9. Annotation enrichment: add build timestamp, commit sha; surface in metrics labels (cardinality caution).
10. Failure budget metrics: consecutive failure gauge & max retries configurable.

## Checklist Status
- [x] CLI flag & API propagation
- [x] Sidecar manifest logic
- [x] Annotation & env wiring
- [x] Unit test coverage (manifest shape)
- [x] Graceful reload (basic: node --watch)
- [x] Digest verify in hot loop
- [ ] E2E latency test (H1/H2)
- [x] Metrics ingestion wiring (definitions + markers DONE; log tail worker)
- [x] Latency emission (ms -> histogram)
- [ ] Robust JSON parsing (replace sed helper with real parser)
- [x] Backoff & jitter in sidecar failure paths

````