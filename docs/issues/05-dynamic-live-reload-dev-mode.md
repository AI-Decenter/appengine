````markdown
# Issue 05: Dev Hot Reload (Sidecar Fetch Loop)

## Ý tưởng
Cho phép lập trình viên cập nhật code mà không rebuild image: sidecar fetch định kỳ artifact mới nếu digest annotation thay đổi.

## Scope
* [x] Sidecar container `fetcher` (busybox wget + tar) loop: nếu annotation digest khác local -> tải & giải nén vào EmptyDir.
* [x] Lệnh CLI: `aether deploy --dev-hot` -> gửi `dev_hot=true` khi tạo deployment (control-plane tạo Deployment với sidecar + annotation `aether.dev/dev-hot=true`).
* [ ] Graceful reload Node: gửi `SIGUSR2` hoặc dùng `nodemon` (tạm thời chạy `node server.js`—chưa tự reload file change trong container, nhưng artifact refresh cập nhật code path). 
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
- Chưa có graceful reload (node process không tự restart/nodemon). Sau khi file hệ thống đổi, NodeJS không reload trừ khi code có cơ chế riêng hoặc ta dùng `nodemon` image.
- Sidecar đang grep JSON thô (simplistic); nên thay bằng jq nhỏ gọn hoặc một tiny Rust helper binary để tránh parsing fragile.
- Không có backoff jitter / exponential delay.
- Metrics ingestion implemented (log tail). Remaining: resilience across pod restarts & multi-namespace support. (Hiện disabled by default qua feature flag.)
- Hạ tầng test Postgres đã chuyển sang Docker testcontainers (README 10.1); không ảnh hưởng trực tiếp nhưng cải thiện tốc độ và tính ổn định khi chạy suite với dev-hot flag.

## Next-Up / Future Enhancements
1. Add graceful reload: đổi image `aether-nodejs:20-slim` -> layer cài `nodemon` và start `nodemon --watch /workspace server.js`.
2. Robust JSON parse: thay grep bằng tiny helper (Rust) hoặc `jq` (nếu chấp nhận kích thước) + timeout / error classification.
3. Metrics resiliency: handle pod restarts, multi-namespace, deduplicate concurrent tails, optional push mode.
4. E2E integration test: patch digest -> assert file contents phục vụ mới trong ≤10s.
5. Watcher optimization: dùng Kubernetes watch thay polling, event-driven update.
6. Security hardening: RBAC minimal (get pod), bỏ `--no-check-certificate`, short-lived projected token.
7. Backoff strategy & jitter khi download fails hoặc checksum mismatch (avoid thundering herd).
8. CLI convenience: `aether dev --hot` loop local build + upload + patch digest.
9. Graceful restart semantics: send signal / health gating so traffic only after refresh complete.
10. Annotation enrichment: `aether.dev/build=<ts>` + optional commit sha.
11. Configurable max retries & metrics for consecutive failures.

## Checklist Status
- [x] CLI flag & API propagation
- [x] Sidecar manifest logic
- [x] Annotation & env wiring
- [x] Unit test coverage (manifest shape)
- [ ] Graceful reload (nodemon / signal)
- [x] Digest verify in hot loop
- [ ] E2E latency test (H1/H2)
- [x] Metrics ingestion wiring (definitions + markers DONE; log tail worker)
- [x] Latency emission (ms -> histogram)
- [ ] Robust JSON parsing (no grep)

````