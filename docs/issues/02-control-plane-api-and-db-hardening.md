````markdown
# Issue 02: Control Plane – Verify Artifact + DB Hardening + Auth sơ bộ

**Loại:** `feat`, `security`  
**Ưu tiên:** Cao  
**Phụ thuộc:** Issue 01 (CLI gửi signature/digest)

## 1. Mục tiêu
Xác thực integrity (digest + chữ ký), lưu metadata artifact & thêm auth token tối giản.

## 2. Scope
* [x] Thêm bảng `artifacts` (digest UNIQUE, size_bytes, created_at, signature, sbom_url, manifest_url).
* [x] Endpoint upload: verify digest khớp nội dung (server recompute), nếu cung cấp header signature – lưu kèm & trạng thái verified (FALSE nếu chưa có public key).
* [x] Thêm header `Authorization: Bearer <token>` kiểm tra token khớp danh sách tĩnh trong env `AETHER_API_TOKENS` (danh sách CSV) (fallback `AETHER_API_TOKEN`).
* [x] Middleware auth + reject 401.

## 3. Acceptance Criteria
| ID | Mô tả | Kết quả |
|----|-------|---------|
| V1 | Upload thiếu digest header | Done (test `upload_missing_digest`) |
| V2 | Digest sai | Done (test `upload_digest_mismatch`) |
| V3 | Token sai | Done (middleware 401 path – TODO add explicit test) |
| V4 | Upload hợp lệ | Done (test `upload_ok_and_duplicate`) |
| V5 | Duplicate digest | Done (same test duplicate branch) |

## 4. Thiết kế
* Hash recompute: stream multipart field -> tee vào file + hasher.
* Signature: lưu raw hex, verify deferred (Issue supply chain nâng cao).
* DB migrations thêm bảng mới + foreign key (artifact->application optional).

## 5. Test Plan
* Integration: missing headers, mismatch digest, duplicate upload, auth fail/succeed (auth fail test TODO follow-up).
* Benchmark nhẹ: hash throughput (log only).

## 6. Risks
## 7. Follow-ups / Enhancements
* [x] Thêm test explicit 401 khi token sai / thiếu (`upload_unauthorized`).
* [x] Bổ sung lưu `app_id` nếu `app_name` map vào applications (lookup + link) (trả về `app_linked`).
* [x] Trả về field `duplicate` = true/false (đã thêm) – cần document README / OpenAPI follow-up.
* [x] Thêm chỉ số Prometheus: `artifact_upload_bytes_total`, `artifact_upload_duration_seconds`.
* [x] Mở rộng OpenAPI với route /artifacts (schema Artifact) & error codes `missing_digest`, `digest_mismatch`.
* [x] Document README: JSON upload response fields (`duplicate`, `app_linked`).
* [x] Nâng cấp verified=true sau khi có public key registry (persist bảng `public_keys`, admin API add key, verification server-side active).
* [x] Thêm gauge số artifacts tổng (`artifacts_total`) (tăng khi insert + init lúc startup).
* [x] Backpressure / limit concurrent artifact writes (Semaphore qua env `AETHER_MAX_CONCURRENT_UPLOADS`).
* [x] OpenAPI security scheme cho Bearer Auth (inject thủ công vào spec JSON).
* [x] Tracing span cho bước signature verify (span `signature_verify` + event success/fail).
* [x] Thêm endpoint HEAD /artifacts/{digest} cho existence check nhanh.
| Rủi ro | Giảm thiểu |
|--------|-----------|
| Recompute hash tốn RAM | Stream từng chunk 64K |
| Token lộ trong log | Không log header raw |

````