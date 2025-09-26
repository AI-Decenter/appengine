````markdown
# Issue 02: Control Plane – Verify Artifact + DB Hardening + Auth sơ bộ

**Loại:** `feat`, `security`  
**Ưu tiên:** Cao  
**Phụ thuộc:** Issue 01 (CLI gửi signature/digest)

## 1. Mục tiêu
Xác thực integrity (digest + chữ ký), lưu metadata artifact & thêm auth token tối giản.

## 2. Scope
* Thêm bảng `artifacts` (digest UNIQUE, size_bytes, created_at, signature, sbom_url, manifest_url).
* Endpoint upload: verify digest khớp nội dung (server recompute), nếu cung cấp header signature – lưu kèm & trạng thái verified (FALSE nếu chưa có public key).
* Thêm header `Authorization: Bearer <token>` kiểm tra token khớp danh sách tĩnh trong env `AETHER_API_TOKENS` (danh sách CSV).
* Middleware auth + reject 401.

## 3. Acceptance Criteria
| ID | Mô tả | Kết quả |
|----|-------|---------|
| V1 | Upload thiếu digest header | 400 + error code `missing_digest` |
| V2 | Digest sai | 400 `digest_mismatch` |
| V3 | Token sai | 401 `unauthorized` |
| V4 | Upload hợp lệ | 200 + JSON chứa artifact_url & digest |
| V5 | Duplicate digest | 200 idempotent (không ghi lại file) |

## 4. Thiết kế
* Hash recompute: stream multipart field -> tee vào file + hasher.
* Signature: lưu raw hex, verify deferred (Issue supply chain nâng cao).
* DB migrations thêm bảng mới + foreign key (artifact->application optional).

## 5. Test Plan
* Integration: missing headers, mismatch digest, duplicate upload, auth fail/succeed.
* Benchmark nhẹ: hash throughput (log only).

## 6. Risks
| Rủi ro | Giảm thiểu |
|--------|-----------|
| Recompute hash tốn RAM | Stream từng chunk 64K |
| Token lộ trong log | Không log header raw |

````