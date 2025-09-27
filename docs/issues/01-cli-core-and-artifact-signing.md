````markdown
# Issue 01: CLI Core mở rộng & Xác thực Artifact (Ký + Streaming Upload)

**Loại:** `feat`  
**Ưu tiên:** Cao  
**Phụ thuộc:** Hiện trạng CLI, endpoint `/artifacts` hiện có

## 1. Mục tiêu
Hoàn thiện pipeline deploy phía client: streaming upload artifact lớn, ký & nhúng metadata (SBOM + manifest + chữ ký) và chuẩn bị verify phía server.

## 2. Scope
### Bao gồm (Status)
* [x] Thay upload hiện tại (đọc toàn bộ file) → streaming body (`reqwest::multipart::Part::stream_with_length`) với fallback buffer cho file nhỏ.
* [x] Thêm header `X-Aether-Artifact-Digest` & `X-Aether-Signature` khi upload.
* [x] Chuẩn hóa output `aether deploy --format json` (artifact, digest, manifest, sbom, signature?).
* [x] Exit code phân biệt chữ ký local không hợp lệ (đã có cơ chế trả `Runtime` -> 20 nếu key invalid).

### Không gồm
* Server verify (nằm Issue 02 Control Plane mở rộng).
* Compress algorithm alternative (zstd) – future.

## 3. Acceptance Criteria (Updated)
| ID | Điều kiện | Kết quả | Status |
|----|-----------|---------|--------|
| A1 | Deploy artifact > 200MB | Không OOM, upload thành công | Pending large-file manual test (follow-up) |
| A2 | Có `AETHER_SIGNING_KEY` | Sinh `.sig` + header | Done (test `deploy_sbom_and_sig.rs`) |
| A3 | `--format json` | In JSON hợp lệ parse được | Done (test `deploy_json_output.rs`) |
| A4 | Không key | Log skip ký, không panic | Done (existing tests) |
| A5 | Clippy + tests | Pass, thêm test streaming giả lập | Partially (streaming path covered indirectly; add >512KB test follow-up) |

## 4. Thiết kế tóm tắt
* Thay `fs::read` → `tokio::fs::File` + `framed` reader, implement struct StreamPart.
* Multipart: sử dụng `reqwest::multipart::Part::stream(Body)`.
* Digest: đã có (sha256); giữ pipeline không đổi.
* JSON output: serde struct + `println!` JSON khi flag.

## 5. Test Plan (Updated)
* Unit: JSON output parsing (added `deploy_json_output.rs`).
* Signature presence (existing `deploy_sbom_and_sig.rs`).
* Large artifact streaming path: FOLLOW-UP add sparse 220MB test (ignored by default / feature gated).
* Invalid signing key length already returns exit code 20 (covered in exit code tests indirectly).

## 6. Rủi ro & Mitigation
| Rủi ro | Mitigation |
|--------|------------|
| Upload bị nghẽn | Tune chunk 64K–256K benchmark |
| JSON flag phá output log | Chỉ in JSON stdout, logs -> stderr |

## 7. Definition of Done
* [x] Streaming upload implemented.
* [x] Headers digest + signature gửi kèm.
* [x] JSON output flag hoạt động.
* [x] Ký & tạo `.sig` khi có key.
* [x] Tests cập nhật / bổ sung.
* [ ] Large artifact stress test (follow-up).
* [ ] README bổ sung mô tả flag `--format json` (follow-up doc update).

## 8. Follow-ups
* Thêm test artifact >512KB để ép nhánh streaming.
* Benchmark chunk size 64K vs 256K.
* README: bảng output JSON fields.
* Option: `--no-sbom` flag cho trường hợp tối ưu tốc độ.
* Chuẩn hóa error JSON khi `--format json` (thay vì in text trước).

````