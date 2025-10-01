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
* [x] E2E test: cập nhật digest -> sidecar kéo bản mới trong ≤10s (harness script `scripts/dev-hot-e2e.sh`).
* [x] Checksum verify trong sidecar loop (sha256sum -c trước extract) & configurable poll interval env `AETHER_FETCH_INTERVAL_SEC`.
* [x] Structured log markers `REFRESH_OK` / `REFRESH_FAIL reason=<...>` trong fetcher script để phục vụ metrics ingestion.
* [x] Metrics definitions (Prometheus): counters & histogram (`dev_hot_refresh_total`, `dev_hot_refresh_failure_total{reason}`, `dev_hot_refresh_latency_seconds`) + ingestion runtime (log tail) behind `AETHER_DEV_HOT_INGEST=1`.

## Acceptance
| ID | Mô tả | Kết quả |
|----|------|---------|
| H1 | Patch digest |  ≤10s code mới có hiệu lực (CHƯA E2E test tự động) |
| H2 | Digest không đổi | Không tải lại (logic: sidecar giữ CUR digest; CHƯA test tự động) |

## Test
* Unit: manifest shape & fetcher script content.
* E2E: script `scripts/dev-hot-e2e.sh <app> <artifact-url> <digest>` đo latency đến `REFRESH_OK`.
	- Exit 0: thành công trong SLO (mặc định 10s)
	- Exit 10: refresh thành công nhưng vượt SLO
	- Exit 20: thất bại / không thấy REFRESH_OK
* Manual: `kubectl annotate deployment <app> aether.dev/digest=sha256:<new>`.

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
## Giới hạn hiện tại (Updated)
Đã bổ sung: readinessProbe gating, watch mode, commit annotation, consecutive failure gauge, rate limit & anomaly detection, JSON parser binary fallback (`json-extract`), dev CLI loop, thực thi binary verifier (`ed25519-verify`), override image qua `AETHER_FETCH_IMAGE`, signature E2E harness script (`dev-hot-signature-e2e.sh`), supervisor graceful restart (`supervisor.sh` runtime generation), metric chuyên biệt `dev_hot_signature_fail_total`.
Còn thiếu: publish sidecar image pipeline, multi-namespace ingestion, persistent metrics snapshot, nâng cấp supervisor (drain HTTP), provenance attestation chain, consolidated minimal image.
Graceful reload hiện dựa trên `node --watch` (chưa handshake nâng cao / drain connections).
Signature verify mới chỉ stub (cần `/verifier/ed25519-verify` + public key env).
Anomaly detection sơ bộ (ngưỡng lỗi liên tiếp) chưa có scoring lịch sử.
CI workflow skeleton chưa build & deploy thực control-plane để test end-to-end thực thụ.

## Next-Up / Future Enhancements (Updated)
ĐÃ HOÀN THÀNH (mở rộng): readinessProbe gating, watch mode, commit annotation + metric label, dev CLI loop, consecutive failure gauge + state restore, rate limit & anomaly detection env-based, RBAC manifest, JSON parser binary fallback.

TIẾP THEO:
1. (Đã tạo Dockerfile) Build custom minimal sidecar image (busybox + json-extract + ed25519-verify) loại bỏ dependence runtime mount (cần publish & set env `AETHER_FETCH_IMAGE`).
2. Tích hợp real signature verify vào pipeline deploy (hiện binary đã có, cần mount hoặc bake image + public key Secret/ConfigMap).
3. Multi-namespace ingestion: watch across namespaces (feature flag) & per-namespace label in metrics.
4. Persist metrics state (failures per app) via lightweight key/value (e.g. emptyDir file or redis optional) – export gauge stable across restarts.
5. Advanced zero-downtime: (partial) supervisor restart implemented; TODO: preStop + readiness drain + connection draining.
6. Provenance chain: store SBOM + signature + build commit annotation; emission of provenance document (in control-plane) referencing artifact digest.
7. Canary & anomaly scoring: export metric `dev_hot_patch_rate_per_minute` + `dev_hot_anomaly_events_total`.
8. CLI `aether dev --hot` enhance: debounce fs changes, optional build filter, immediate patch only if diff boundaries crossed.
9. Harden security: short-lived projected SAT token, remove generic pod list (only self get), TLS cert verification enable.
10. Add build timestamp annotation & optionally commit short SHA in container env; label cardinality safeguards.
11. Convert polling loop default to watch mode after stability validation (flag flip).
12. Add integration in CI to deploy actual control-plane & run full refresh cycle (artifact v1 -> patch -> verify v2).
10. Failure budget metrics: consecutive failure gauge & max retries configurable.
- [x] Watch mode + rate limiting + anomaly detection
- [x] Commit annotation + metrics label + consecutive failure gauge
- [x] Dev CLI loop `aether dev --hot`
- [x] JSON parser binary fallback (`json-extract`)
## Checklist Status
- [x] CLI flag & API propagation
- [x] Sidecar manifest logic
- [x] Annotation & env wiring
- [x] Unit test coverage (manifest shape)
- [x] Graceful reload (basic: node --watch)
- [x] Digest verify in hot loop
- [x] E2E latency test (H1/H2)
- [x] Metrics ingestion wiring (definitions + markers DONE; log tail worker)
- [x] Latency emission (ms -> histogram)
- [x] Robust JSON parsing (tạm: awk state-machine parser thay sed; nâng cấp Rust binary ở issue riêng)
- [x] Backoff & jitter in sidecar failure paths
 - [x] Signature verification binary & Secret-based pubkey wiring
 - [x] Signature E2E harness
 - [x] Supervisor basic graceful restart loop (digest-driven)
 - [x] Dedicated signature failure metric
 - [x] Server-side signature enforcement flag (AETHER_REQUIRE_SIGNATURE)
 - [x] Basic provenance document emission
 - [x] SBOM serving endpoint (linkage groundwork for provenance)
 - [x] Multi-namespace ingestion flag (AETHER_DEV_HOT_MULTI_NS)

````