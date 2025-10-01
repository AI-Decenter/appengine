````markdown
# Issue 06: SBOM & Supply Chain Security mở rộng

## Mục tiêu
Nâng nền tảng supply chain: chuẩn hóa SBOM theo CycloneDX, phục vụ phân phối minh bạch, kiểm soát chữ ký server-side, và chuẩn bị provenance mở rộng.

## Scope (Planned vs Implemented)
| Hạng mục | Trạng thái | Ghi chú |
|----------|-----------|---------|
| Xuất SBOM CycloneDX JSON 1.5 | CHƯA (hiện custom `aether-sbom-v1`) | Cần chuyển schema + bomFormat, specVersion, component graph |
| Gắn SBOM URL vào artifact record | CHƯA | DB chưa lưu đường dẫn SBOM; hiện file chỉ sinh local client side |
| Endpoint `GET /artifacts/{digest}/sbom` | DONE | Trả file `<digest>.sbom.json` từ `AETHER_SBOM_DIR` (simple static read) |
| Server verify chữ ký artifact (env gated) | DONE | `AETHER_REQUIRE_SIGNATURE=1` -> bắt buộc chữ ký & verify pubkey(s) trước deploy |
| Provenance document emission | PARTIAL | Ghi file JSON basic (digest, commit, signature_present) – chưa chuẩn in-toto/Slsa |
| Dedicated signature failure metric | DONE (Issue 05) | `dev_hot_signature_fail_total` |
| SBOM validation server-side | CHƯA | Chưa parse/validate schema khi nhận upload |
| Attach provenance link vào metadata | CHƯA | Chưa expose endpoint / provenance index |

## Hiện tại (Current Implementation)
1. CLI sinh SBOM JSON tùy biến `aether-sbom-v1` (files, dependencies, manifest digest).
2. File SBOM lưu cạnh artifact nội bộ phía client (không tự động upload). 
3. Server có endpoint `GET /artifacts/{digest}/sbom` (simple file server) – cần pipeline upload SBOM vào `AETHER_SBOM_DIR` để phục vụ được.
4. Chữ ký client-side Ed25519: CLI ký digest nếu `AETHER_SIGNING_KEY` tồn tại.
5. Server: nếu `AETHER_REQUIRE_SIGNATURE=1` và request thiếu signature -> HTTP 400. Có verify public key (đã tồn tại key mgmt logic từ Issue 05).
6. Provenance cơ bản: ghi JSON `aether.provenance.v1` với trường (app, digest, signature_present, commit, timestamp) vào `AETHER_PROVENANCE_DIR`.
7. Multi-namespace ingest & signature metrics hỗ trợ quan sát bất thường.

## Acceptance Mapping
| ID | Mô tả | Trạng thái | Ghi chú |
|----|------|-----------|--------|
| S1 | SBOM hợp lệ validator | CHƯA | Cần library hoặc schema validation CycloneDX 1.5 |
| S2 | Chữ ký sai | PASS | Trả về 400 khi signature không hợp lệ / thiếu (flag bật) |

## Thiếu / Gaps
* Chưa chuyển sang định dạng CycloneDX (bomFormat, specVersion, components, hashes, dependencies graph).
* Chưa có upload SBOM & lưu đường dẫn / storage key trong DB artifacts.
* Endpoint SBOM chỉ phục vụ file local – không fallback object storage.
* Chưa thực hiện validation SBOM server-side (structure & hash alignment).
* Provenance chưa liên kết SBOM + signature + build metadata đầy đủ (SLSA provenance / in-toto statements).
* Chưa ghi metric coverage % artifact có SBOM / signature.
* Chưa enforce hash match giữa SBOM manifest_digest và artifact digest server-side.

## Next-Up / Roadmap
1. CycloneDX migration: generator module tạo JSON 1.5 (fields: bomFormat, specVersion, serialNumber (UUID), metadata.component, components[], hashes (SHA-256), dependencies graph).
2. SBOM upload phase: CLI POST `/artifacts/{digest}/sbom` (new endpoint) + server lưu storage (S3 or FS) + DB column `sbom_url`.
3. SBOM validation server-side: parse CycloneDX, xác thực schema & đối chiếu file list/hash bloom or deterministic manifest digest.
4. Integrity binding: Lưu hash SBOM vào provenance doc; add field `sbom_sha256`.
5. Provenance v2 (in-toto style): subject (artifact digest), materials (dependency lockfiles), builder info, invocation parameters.
6. Policy enforcement layer: flag `AETHER_ENFORCE_SBOM=1` -> reject deploy nếu thiếu hoặc invalid SBOM.
7. Metrics: `sbom_artifacts_total`, `sbom_valid_total`, `signed_artifacts_total`, `provenance_emitted_total`.
8. CLI: tùy chọn `--cyclonedx` chuyển mới, fallback legacy until cutover.
9. Backfill job: scan artifacts không SBOM -> cảnh báo / tạo SBOM if reproducible build.
10. Attestation bundling: produce DSSE envelope (JSON) chứa signature + SBOM digest + provenance.
11. Public key rotation policy & expiry metadata.
12. Cache-control headers cho SBOM endpoint + ETag.

## Phân Công Gợi Ý (Optional)
| Task | Độ ưu tiên | Effort |
|------|-----------|--------|
| CycloneDX generator | Cao | Trung |
| SBOM upload + DB field | Cao | Trung |
| Validation & policy flag | Cao | Trung |
| Provenance v2 (in-toto lite) | Trung | Cao |
| Metrics coverage | Trung | Thấp |
| DSSE Attestation | Thấp | Trung |

## Checklist Chi Tiết
- [x] Endpoint phục vụ SBOM `/artifacts/{digest}/sbom`
- [x] Server-side signature enforcement flag
- [x] Chữ ký verify trước deploy
- [x] Provenance tài liệu cơ bản
- [ ] SBOM CycloneDX 1.5 output
- [ ] SBOM upload & storage integration
- [ ] DB schema: cột `sbom_url`
- [ ] Server SBOM validation logic
- [ ] Policy `AETHER_ENFORCE_SBOM`
- [ ] Metrics coverage (SBOM & signature)
- [ ] In-toto style provenance nâng cao
- [ ] DSSE Attestation bundling
- [ ] Cache headers / ETag SBOM endpoint
- [ ] Public key rotation metadata

## Ghi Chú Thực Thi
* Giữ backward compatibility bằng flag chuyển đổi dần CycloneDX.
* Validation nên fail-fast trước khi áp dụng Deployment để tránh drift giữa cluster và metadata.
* Có thể tái sử dụng manifest file hash list để xây component hashes nhanh.
* Mở rộng signing: sign CBOR hoặc JSON canonicalized để ổn định chữ ký.

## Rủi Ro & Mitigation
| Rủi ro | Ảnh hưởng | Giảm thiểu |
|--------|-----------|------------|
| SBOM lớn gây chậm upload | Độ trễ deploy | Nén + gzip serving |
| CycloneDX schema updates | Incompatibility | Pin specVersion 1.5 & test validation |
| Key compromise | Giả mạo artifact | Key rotation + revoke list |
| Thiếu SBOM khi enforce | Block pipeline | Soft warn phase trước hard fail |

## Trạng Thái Tổng Quan
Nền tảng chữ ký & phục vụ SBOM bước đầu đã có; CycloneDX + policy + provenance nâng cao là chặng tiếp theo để đạt chuẩn supply chain minh bạch.

````