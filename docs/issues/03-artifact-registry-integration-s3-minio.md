````markdown
# Issue 03: Tích hợp Artifact Registry (MinIO/S3 Presigned URL)

**Loại:** `feat`  
**Phụ thuộc:** 02 (DB lưu artifact)

## 1. Mục tiêu
Chuyển lưu trữ local → MinIO (hoặc S3) dùng presigned upload URL, tách IO khỏi Control Plane.

## 2. Scope
Checklist (✅ = done, ⏳ = in progress, 🔜 = planned)

| Mục | Trạng thái | Ghi chú |
|-----|------------|---------|
| Endpoint `POST /artifacts/presign` (real S3 presign) | ✅ | AWS SDK V4 presign + metadata sha256 |
| Endpoint `POST /artifacts/complete` | ✅ | Pending → stored; remote size & metadata digest verify |
| Trạng thái artifact (`pending`/`stored`) | ✅ | Cột `status`, HEAD chỉ 200 khi `stored` |
| Cấu trúc key `artifacts/<app>/<digest>/app.tar.gz` | ✅ | Chuẩn hoá key layout |
| Idempotent presign (stored → method NONE) | ✅ | Pending: cấp lại URL để retry |
| Idempotent complete (duplicate nếu stored) | ✅ | `duplicate=true` |
| Signature verification ở complete | ✅ | Reuse public keys DB |
| Metrics tổng số artifacts | ✅ | Gauge init + increment |
| Remote metadata + optional hash verify | ✅ | Metadata luôn, hash optional nhỏ (<= threshold) |
| GC pending TTL | ✅ | Background loop + on-demand function |
| Max artifact size enforcement | ✅ | Env `AETHER_MAX_ARTIFACT_SIZE_BYTES` |
| Digest mismatch metric | ✅ | `artifact_digest_mismatch_total` |
| Retry S3 HEAD/GET | ✅ | 3 attempts w/ backoff |
| CLI tích hợp presign/complete | ⏳ | Chưa chuyển CLI, vẫn dùng upload cũ |
| Thay thế hẳn upload multipart trực tiếp | 🔜 | Deprecate sau khi CLI đổi |

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
* Remote hash verify path chưa test riêng (follow-up) ⏳

## 5. Thiết kế trạng thái
`pending` – tạo lúc presign, `size_bytes=0`, chưa có chữ ký.
`stored` – sau complete: cập nhật size, signature, verified.
HEAD chỉ phản ánh `stored` giúp client phân biệt upload chưa finalize.

## 6. Kiến trúc & luồng
1. Client: POST /artifacts/presign (nhận URL + headers, status=pending).
2. Client: PUT file → MinIO/S3 (ngoài Control Plane).
3. Client: POST /artifacts/complete (gửi digest + size + signature optional).
4. Control Plane: cập nhật row, verify chữ ký, metrics, trả về kết quả.

## 7. Giới hạn hiện tại
* Chưa ký URL thật (mock base URL + header ACL).
* Không kiểm tra kích thước object thực tế so với `size_bytes` client gửi.
* Không xác thực remote checksum/etag.
* Không có TTL / expiration cho bản ghi `pending` (có thể rác nếu client bỏ).
* Không revoke / rotate URL (stateless mock link).
* CLI chưa chuyển sang quy trình 2-phase.
* Chưa có quota / rate-limit per app.

## 8. Enhancements (Planned)
| Nhãn | Mô tả | Ưu tiên |
|------|------|---------|
| E1 | AWS / MinIO real presign (SDK hoặc chữ ký V4 thủ công) | High |
| E2 | TTL + GC bản ghi `pending` quá hạn | Medium |
| E3 | Validate kích thước object (HEAD / stat) so với `size_bytes` | ✅ (S3 HEAD) |
| E4 | Optional server SHA256 re-hash bằng streaming từ remote (small objects) | ⏳ (threshold-based) |
## 13. Env mới / cập nhật
```
AETHER_MAX_ARTIFACT_SIZE_BYTES=52428800          # Giới hạn kích thước (ví dụ 50MB)
AETHER_PENDING_GC_INTERVAL_SECS=300              # Chu kỳ chạy GC pending
AETHER_PENDING_GC_TTL_SECS=3600                 # TTL xoá pending > TTL
AETHER_S3_ENDPOINT_URL=http://minio:9000        # Override endpoint (MinIO)
AETHER_REQUIRE_PRESIGN=true                     # Ép buộc presign trước complete
AETHER_VERIFY_REMOTE_SIZE=true                  # Bật HEAD size check (default true)
AETHER_VERIFY_REMOTE_DIGEST=true                # Bật metadata digest check (default true)
AETHER_VERIFY_REMOTE_HASH=false                 # Bật hash streaming nhỏ
AETHER_REMOTE_HASH_MAX_BYTES=8000000            # Ngưỡng tối đa hash (8MB)
AETHER_PRESIGN_EXPIRE_SECS=900                  # Thời gian hết hạn URL
AETHER_ARTIFACT_BUCKET=artifacts                # Tên bucket
AETHER_STORAGE_MODE=s3                          # Backend: s3 hoặc mock
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
Core flow (mock) HOÀN THÀNH – chuyển sang giai đoạn triển khai presign thực và CLI migration.

## 12. Next Steps Actionable
1. E1: Tích hợp AWS SDK (hoặc rusoto/minio client) tạo URL có expiry.
2. E3: HEAD object validate size & optional digest.
3. E7: Bật flag cấu hình `AETHER_REQUIRE_PRESIGN` để ép buộc quy trình.
4. CLI refactor: `aether deploy` → presign + streaming PUT + complete.
5. E15: Tạo trait `StorageBackend` + implementation `S3Backend` & `MockBackend`.
6. Bổ sung test MinIO thực (docker service) cho CI optional stage.

---
Updated: 2025-09-27

````