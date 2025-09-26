````markdown
# Issue 01: CLI Core mở rộng & Xác thực Artifact (Ký + Streaming Upload)

**Loại:** `feat`  
**Ưu tiên:** Cao  
**Phụ thuộc:** Hiện trạng CLI, endpoint `/artifacts` hiện có

## 1. Mục tiêu
Hoàn thiện pipeline deploy phía client: streaming upload artifact lớn, ký & nhúng metadata (SBOM + manifest + chữ ký) và chuẩn bị verify phía server.

## 2. Scope
### Bao gồm
* Thay upload hiện tại (đọc toàn bộ file) → streaming body (`reqwest::Body` từ `tokio::fs::File` + reader async chunks).
* Thêm header `X-Aether-Artifact-Digest` & `X-Aether-Signature` gửi kèm upload.
* Chuẩn hóa output `aether deploy --format json` (artifact_path, digest, sbom_path, signature_path).
* Exit code phân biệt: verify local chữ ký fail -> code 20.

### Không gồm
* Server verify (nằm Issue 02 Control Plane mở rộng).
* Compress algorithm alternative (zstd) – future.

## 3. Acceptance Criteria
| ID | Điều kiện | Kết quả |
|----|-----------|---------|
| A1 | Deploy artifact > 200MB | Không OOM, upload thành công |
| A2 | Có `AETHER_SIGNING_KEY` | Sinh `.sig` + thêm header khi upload |
| A3 | `--format json` | In JSON hợp lệ parse được |
| A4 | Không key | Log skip ký, không panic |
| A5 | Clippy + tests | Pass, thêm test streaming giả lập |

## 4. Thiết kế tóm tắt
* Thay `fs::read` → `tokio::fs::File` + `framed` reader, implement struct StreamPart.
* Multipart: sử dụng `reqwest::multipart::Part::stream(Body)`.
* Digest: đã có (sha256); giữ pipeline không đổi.
* JSON output: serde struct + `println!` JSON khi flag.

## 5. Test Plan
* Unit: streaming chunk size (mô phỏng file lớn bằng write temp 220MB sparse).  
* Integration: deploy pack-only + format json parse.  
* Property: chữ ký 64 hex -> 32 bytes, invalid length → exit code 20.

## 6. Rủi ro & Mitigation
| Rủi ro | Mitigation |
|--------|------------|
| Upload bị nghẽn | Tune chunk 64K–256K benchmark |
| JSON flag phá output log | Chỉ in JSON stdout, logs -> stderr |

## 7. Definition of Done
* Tất cả Acceptance pass; docs README cập nhật flag `--format json`.

````