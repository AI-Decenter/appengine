````markdown
# Issue 03: Tích hợp Artifact Registry (MinIO/S3 Presigned URL)

**Loại:** `feat`  
**Phụ thuộc:** 02 (DB lưu artifact)

> CẬP NHẬT 2025-09-28: Issue đã được mở rộng vượt phạm vi ban đầu (two-phase + multipart, quotas, retention, idempotency, SSE, audit events, metrics nâng cao). Tài liệu này phản ánh trạng thái thực thi hiện tại và liệt kê các bước tiếp theo mới.

## 1. Mục tiêu
Chuyển lưu trữ local → MinIO/S3 qua presigned URL để tách IO khỏi Control Plane, đảm bảo tính toàn vẹn (digest, kích thước, optional remote hash) và mở đường cho mở rộng multipart uploads.

## 2. Scope
Checklist (✅ done, ⏳ in progress, 🆕 newly added scope, � deprecated, 🔜 planned)

| Mục | Trạng thái | Ghi chú cập nhật |
|-----|------------|------------------|
| Endpoint `POST /artifacts/presign` | ✅ | AWS SDK V4 presign + sha256 metadata + method `NONE` nếu duplicate |
| Endpoint `POST /artifacts/complete` | ✅ | Verify size (HEAD), metadata digest, optional remote hash; idempotency key hỗ trợ |
| Trạng thái artifact `pending` / `stored` | ✅ | HEAD chỉ 200 khi stored (nay có endpoint meta mới) |
| Key layout `artifacts/<app>/<digest>/app.tar.gz` | ✅ | Chuẩn hoá phục vụ GC / phân tích |
| Idempotent presign | ✅ | Duplicate trả method `NONE` |
| Idempotent complete | ✅ | Duplicate trả `duplicate=true` / status stored |
| Signature verification | ✅ | Reuse bảng public_keys (Ed25519) |
| Max artifact size enforcement | ✅ | `AETHER_MAX_ARTIFACT_SIZE_BYTES` |
| Remote metadata digest verify | ✅ | Luôn bật (có thể tắt qua env) |
| Optional remote hash (small objects) | ✅ | `AETHER_VERIFY_REMOTE_HASH` + threshold |
| Retry HEAD/GET S3 | ✅ | 3 attempts exponential backoff |
| Pending TTL GC (manual helper) | ✅ | Hàm `run_pending_gc` + metrics GC |
| Metrics cơ bản (count, duration) | ✅ | Được mở rộng (xem danh sách dưới) |
| Digest mismatch metric | ✅ | `artifact_digest_mismatch_total` |
| CLI two-phase upload | ✅ | Mặc định; legacy chỉ qua `--legacy-upload` |
| Deprecation legacy multipart endpoint | ✅ | Header `X-Aether-Deprecated`, metric đếm |
| Quota per app (count / bytes) | ✅ | `AETHER_MAX_ARTIFACTS_PER_APP`, `AETHER_MAX_TOTAL_BYTES_PER_APP` + metric quota reject |
| Retention keep latest N | ✅ | `AETHER_RETAIN_LATEST_PER_APP` + event `retention_delete` |
| Idempotency key (complete + multipart) | ✅ | `idempotency_key` cột unique, conflict 409 |
| Audit events table | ✅ | `artifact_events` + metric `artifact_events_total` |
| Multipart S3 upload (init/presign-part/complete) | ✅ | CLI tự động khi vượt `AETHER_MULTIPART_THRESHOLD_BYTES` |
| Multipart part metrics (count, size histogram) | ✅ | Approx part size estimation at completion |
| SSE (AES256 / KMS) hỗ trợ presign | ✅ | Env `AETHER_S3_SSE`, `AETHER_S3_SSE_KMS_KEY` |
| OpenAPI mô tả multipart & two-phase | ✅ | Annotations cập nhật |
| CLI progress bar (PUT / multipart) | ✅ | Chỉ hiện khi TTY + size > threshold |
| Upload PUT duration metric (client provided) | ✅ | Header `X-Aether-Upload-Duration` => histogram |
| Storage abstraction trait mở rộng | ✅ | Trait + mock + s3 backend |
| Artifact meta endpoint `GET /artifacts/{digest}/meta` | ✅ | Trả đầy đủ trường mới |
| Histogram multipart part sizes chính xác | 🔜 | Hiện ước lượng; cần gửi part size thực từ client |
| Resume multipart (retry part ETAG reuse) | 🔜 | Chưa lưu part list tạm thời |
| Background scheduled pending GC | 🔜 | Hiện manual helper, chưa cron nội bộ |
| Webhook / event streaming | 🔜 | Chưa triển khai (event table đã sẵn) |
| HEAD giàu thông tin (thay meta endpoint) | 🔜 | Có meta endpoint thay thế; HEAD hiện vẫn minimal |
| Rate limit per app | 🔜 | Chưa thiết kế chi tiết |
| ETag integrity cross-check (multipart) | 🔜 | Hiện dựa vào S3 complete; không so khớp manifest cục bộ |
| Server SBOM / manifest lưu trữ | 🔜 | CLI tạo local, chưa upload & reference |
| Encryption enforcement policy | 🔜 | Chưa ép buộc SSE theo app policy |
| Event bus integration (Kafka/NATS) | 🔜 | Chưa triển khai |
| Retention theo tuổi (age-based) | 🔜 | Chỉ keep-latest N |
| Structured error taxonomy final | 🔜 | Một số mã mới nhưng chưa chuẩn hoá đầy đủ |

## 3. Acceptance (Giữ nguyên + mở rộng test đã có)

## 3. Acceptance
| ID | Điều kiện | Kết quả | Trạng thái |
|----|-----------|---------|-----------|
| P1 | Presign request không app | 400 | ✅ Test `presign` validate app_name |
| P2 | Upload xong notify | 200, artifact trạng thái `stored` | ✅ Test `presign_creates_pending_and_head_not_found_until_complete` + `presign_complete_idempotent` |
| P3 | Upload lại digest | 200 idempotent | ✅ Duplicate complete + presign method NONE |
| P4 | HEAD trước khi complete | 404 | ✅ Chỉ `stored` mới trả 200 |
| P5 | Pending retry presign | Cấp lại PUT URL | ✅ Logic branch status='pending' |

## 4. Test
Đã có:
* `presign_complete_idempotent`: verify flow & duplicate.
* `presign_creates_pending_and_head_not_found_until_complete`: trạng thái chuyển `pending` → `stored`.
* Signature + duplicate + integrity tests tái sử dụng schema & upload tests trước.

Thiếu / cần thêm (follow-up):
* PUT thực tế (integration với MinIO) – ĐÃ có test S3 (skips nếu không bật env) ✅
* Negative: complete khi chưa presign – đã hỗ trợ flag bắt buộc (`AETHER_REQUIRE_PRESIGN`) ✅
* Negative: presign digest không hợp lệ – validation hiện có ✅
* Remote hash verify path chưa test riêng (follow-up) ✅ Test `s3_presign_complete_with_remote_hash` (MinIO gated)

## 5. Thiết kế trạng thái
`pending` – tạo lúc presign, `size_bytes=0`, chưa có chữ ký.
`stored` – sau complete: cập nhật size, signature, verified.
HEAD chỉ phản ánh `stored` giúp client phân biệt upload chưa finalize.

## 6. Kiến trúc & luồng
1. Client: POST /artifacts/presign (nhận URL + headers, status=pending).
2. Client: PUT file → MinIO/S3 (ngoài Control Plane).
3. Client: POST /artifacts/complete (gửi digest + size + signature optional).
4. Control Plane: cập nhật row, verify chữ ký, metrics, trả về kết quả.

## 7. Giới hạn còn lại (Updated)
* Multipart: chưa hỗ trợ resume một phần (phải re-init nếu gián đoạn trước khi complete).
* Part size histogram: dùng ước lượng (chia đều) – cần gửi thực tế để tăng độ chính xác khi phân tích phân mảnh.
* Pending GC chưa có scheduler nội bộ định kỳ (chỉ helper + có thể operator gọi thủ công / cron job bên ngoài).
* Rate limiting chưa áp dụng (áp dụng quotas trước, throttling sau).
* Webhook / streaming events chưa tích hợp message bus – chỉ lưu DB.
* Age-based retention chưa có (mới keep-latest N).
* Manifest / SBOM chưa đồng bộ server (client side only).
* Error taxonomy chưa “locked”; cần formal schema + tài liệu mapping.
* HEAD vẫn tối giản – meta endpoint mới đáp ứng nhu cầu giàu dữ liệu nhưng HEAD tiêu chuẩn có thể mở rộng trả ETag/verified.
* Resume multipart: thiếu lưu trữ trạng thái các part đã tải (tối thiểu cần bảng tạm hoặc JSON column cho future resume).

## 8. Enhancements (Historical vs. Current)
| Nhãn | Mô tả | Trạng thái |
|------|------|-----------|
| E1 | Real presign (SDK) | ✅ |
| E2 | TTL + GC pending | ✅ (helper) – Scheduler 🔜 |
| E3 | HEAD size validate | ✅ |
| E4 | Optional remote re-hash | ✅ |
| E5 | Thêm `completed_at` | ✅ |
| E6 | Metrics presign/complete histograms | ✅ (nhiều metrics bổ sung) |
| E7 | Policy require presign | ✅ (`AETHER_REQUIRE_PRESIGN`) |
| E8 | Quota per app | ✅ |
| E9 | Multipart S3 upload | ✅ (CLI + server) |
| E10 | SSE encryption flags | ✅ |
| E11 | Retention last N | ✅ |
| E12 | Webhook/event emit | 🔜 (event rows only) |
| E13 | CLI fallback legacy | ✅ |
| E14 | Idempotency key complete | ✅ |
| E15 | Storage abstraction trait | ✅ |
| E16 | OpenAPI transitions | ✅ (annotations enriched) |
| E17 | HEAD rich metadata | ✅ (meta endpoint alt) |
| E18 | Audit log transitions | ✅ (artifact_events) |
| E19 | Multipart metrics histogram (parts/size) | ✅ (approx) |
| E20 | Precise part size reporting | 🔜 |
| E21 | Multipart resume | 🔜 |
| E22 | Age-based retention | 🔜 |
| E23 | Event streaming outbound | 🔜 |
| E24 | Upload anomaly detection (latency outliers) | 🔜 |
| E25 | Manifest/SBOM upload + link | 🔜 |
## 13. Env cập nhật (Đồng bộ với mã nguồn hiện tại)
```
# Core upload
AETHER_MAX_ARTIFACT_SIZE_BYTES=52428800      # (Optional) Giới hạn kích thước artifact
AETHER_PRESIGN_EXPIRE_SECS=900               # Expiry presigned URL
AETHER_REQUIRE_PRESIGN=true                  # Buộc presign trước complete

# Verification
AETHER_VERIFY_REMOTE_SIZE=true               # HEAD size check
AETHER_VERIFY_REMOTE_DIGEST=true             # Metadata sha256 check
AETHER_VERIFY_REMOTE_HASH=false              # Hash streaming nhỏ
AETHER_REMOTE_HASH_MAX_BYTES=8000000         # Ngưỡng tối đa hash remote

# Multipart
AETHER_MULTIPART_THRESHOLD_BYTES=134217728   # (VD 128MB) Bật multipart khi >= threshold
AETHER_MULTIPART_PART_SIZE_BYTES=8388608     # (8MB) Kích thước part mục tiêu

# Quota & retention
AETHER_MAX_ARTIFACTS_PER_APP=5               # Giới hạn số artifact / app
AETHER_MAX_TOTAL_BYTES_PER_APP=1073741824    # (1GB) Giới hạn dung lượng / app
AETHER_RETAIN_LATEST_PER_APP=3               # Giữ N artifact mới nhất

# Pending GC (helper)
AETHER_PENDING_TTL_SECS=3600                 # TTL pending trước khi bị xoá
AETHER_PENDING_GC_INTERVAL_SECS=300          # Gợi ý chu kỳ chạy GC (chưa scheduler nội bộ)

# Concurrency
AETHER_MAX_CONCURRENT_UPLOADS=32             # Semaphore legacy endpoint

# S3 / Storage
AETHER_STORAGE_MODE=s3                       # s3 | mock
AETHER_ARTIFACT_BUCKET=artifacts             # Tên bucket
AETHER_S3_ENDPOINT_URL=http://minio:9000     # Endpoint MinIO
AETHER_S3_SSE=AES256                         # AES256 | aws:kms (optional)
AETHER_S3_SSE_KMS_KEY=...                    # Khi dùng aws:kms

# CLI / Internal
AETHER_API_BASE=http://localhost:8080        # Cấu hình CLI
```
| E5 | Thêm cột `completed_at` cho audit | Medium |
| E6 | Metrics: presign count, complete latency histogram riêng | Medium |
| E7 | Policy: bắt buộc presign (reject complete nếu không `pending`) | Medium |
| E8 | Quota theo app (số artifact / dung lượng) | Medium |
| E9 | Multi-part S3 upload support (threshold > size) | Low |
| E10 | Encryption at rest (SSE-S3 / SSE-KMS flags) | Low |
| E11 | Artifact retention / GC theo policy (last N / age) | Medium |
| E12 | Webhook / event emit khi artifact stored | Medium |
| E13 | CLI fallback nếu MinIO down (tạm dùng direct upload) | Low |
| E14 | Idempotency key cho complete để tránh double-update | Low |
| E15 | Storage abstraction trait (S3, GCS, filesystem) | High |
| E16 | OpenAPI mô tả trạng thái / transitions | Medium |
| E17 | HEAD trả metadata (verified, size) thay vì chỉ 200 | Medium |
| E18 | Audit log cho tất cả status transitions | Low |

## 9. Env đề xuất (tương lai)
```
AETHER_S3_BASE_URL=http://minio:9000
AETHER_S3_BUCKET=artifacts
AETHER_S3_REGION=us-east-1
AETHER_S3_ACCESS_KEY=...
AETHER_S3_SECRET_KEY=...
AETHER_PRESIGN_EXPIRE_SECONDS=900
```

## 10. Risk / Mitigation
| Rủi ro | Ảnh hưởng | Giảm thiểu |
|--------|-----------|-----------|
| Pending bị bỏ | Rác DB | TTL + cleanup job (E2) |
| Digest collision giả mạo | Ghi đè dữ liệu | Digest UNIQUE + verify signature (nếu yêu cầu) |
| Size giả mạo | Sai quan sát / billing | HEAD remote & etag (E3) |
| URL lộ ra ngoài | Upload trái phép | Presign exp + scoped policy (E1) |

## 11. Trạng thái tổng quan
Core two-phase + multipart + quotas + retention + idempotency + SSE + audit events: HOÀN THÀNH.

Legacy direct multipart endpoint đã được đánh dấu deprecated (header + metric). Tập trung tiếp theo: nâng độ chính xác observability (real part sizes), tăng tính phục hồi (resume multipart), streaming events, và tightening policy (rate limits, encryption enforcement).

## 12. Next Steps Actionable (Updated)
1. Multipart resume: lưu metadata part (số part đã up, etag) để retry không mất tiến độ.
2. Chính xác hoá metrics part size: CLI gửi kích thước part thực tế (mảng {part_number,size_bytes}).
3. Scheduled pending GC worker: interval task nội bộ thay vì manual trigger.
4. Event streaming: publish artifact events (stored, retention_delete) ra Kafka/NATS.
5. Age-based retention (song song keep-latest N tuỳ chọn).
6. Manifest & SBOM server-side storage + integrity link (cột sbom_url / manifest_url hiện đang NULL).
7. Rate limiting per app (token bucket hoặc leaky bucket) bổ sung ngoài quota.
8. Encryption policy enforcement (bắt buộc SSE bật khi flag compliance).
9. Extended HEAD: enrich hoặc alias HEAD -> meta (backwards safe) / ETag propagate.
10. Error taxonomy v2: tài liệu hóa + mã hoá enum ổn định (client friendly).
11. Integration test: multipart negative cases (wrong ETag, missing part).
12. Alerting rules: quota_exceeded, digest_mismatch spike, multipart_complete_failures.
13. Observability: exemplars / tracing spans (upload lifecycle id).
14. CLI optimization: parallel presign-part + upload pipeline (prefetch presign for next part).
15. Storage abstraction: extend to GCS backend (signing v4) & local FS for dev fallback.
16. Security: detect stale pending > TTL automatically w/ background deletion + metric anomalies.
17. Documentation: OpenAPI artifact state machine diagram.
18. Performance: streaming hashing for multipart final verification (optional full re-hash for compliance).
19. Policy: per-app max concurrent multipart sessions.
20. Testing: fuzz invalid idempotency_key collisions and concurrency race conditions.

---
Updated: 2025-09-28 (post enhancement pass)

````