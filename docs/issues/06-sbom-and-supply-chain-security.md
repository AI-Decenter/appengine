````markdown
# Issue 06: SBOM & Supply Chain Security mở rộng

## Mục tiêu
Nâng nền tảng supply chain: chuẩn hóa SBOM theo CycloneDX, phục vụ phân phối minh bạch, kiểm soát chữ ký server-side, và chuẩn bị provenance mở rộng.

## Scope (Planned vs Implemented)
| Hạng mục | Trạng thái | Ghi chú |
|----------|-----------|---------|
| Xuất SBOM CycloneDX JSON 1.5 | DONE (enriched) | CLI flag `--cyclonedx`, bomFormat/specVersion, manifest hash, dependency graph + per-dep hashes |
| Gắn SBOM URL vào artifact record | DONE | `upload_sbom` cập nhật cột sbom_url (/artifacts/{digest}/sbom) |
| Endpoint `GET /artifacts/{digest}/sbom` | DONE | Trả file `<digest>.sbom.json` từ `AETHER_SBOM_DIR` (simple static read) |
| Server verify chữ ký artifact (env gated) | DONE | `AETHER_REQUIRE_SIGNATURE=1` -> bắt buộc chữ ký & verify pubkey(s) trước deploy |
| Provenance document emission | PARTIAL (v1+v2) | v1 basic + v2 (sbom_sha256, materials, dsse envelope, provenance_emitted_total metric) – still not full in-toto/SLSA |
| Dedicated signature failure metric | DONE (Issue 05) | `dev_hot_signature_fail_total` |
| SBOM validation server-side | DONE (subset + strict deploy check) | jsonschema subset/full + size limits + metrics + deploy-time validated flag |
| Full CycloneDX schema validation (env gated) | DONE (AETHER_CYCLONEDX_FULL_SCHEMA) | Extended schema sections (components, dependencies) |
| Provenance list/fetch endpoints | DONE | /provenance, /provenance/{digest}, /provenance/{digest}/attestation |
| DSSE real signing (attestation key) | DONE | ed25519 dedicated key (AETHER_ATTESTATION_SK) canonical JSON |
| Lockfile integrity ingestion | DONE (npm) | Parse package-lock.json integrity -> per-dep hashes |
| Manifest upload + digest cross-check | DONE (Phase 3) | /artifacts/{digest}/manifest + manifest_digest ↔ SBOM x-manifest-digest enforcement |
| Strict SBOM deploy enforcement | DONE (Phase 3) | Enforce sbom_validated & manifest_digest match when AETHER_ENFORCE_SBOM=1 |
| Extended metrics (provenance_emitted_total, sbom_invalid_total) | DONE (Phase 3) | Added new counters |
| Attach provenance link vào metadata | PARTIAL | Stored files + provenance_present DB flag (no listing endpoint yet) |

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
* Advanced CycloneDX sections (services, compositions, vulnerabilities) vẫn chưa parse.
* Per-file content hashing for dependencies (only aggregated + integrity) chưa đầy đủ reproducibility proof.
* Per-file content hashing for dependencies (only aggregated + integrity) chưa đầy đủ reproducibility proof.
* Advanced CycloneDX sections (services, compositions, vulnerabilities) vẫn chưa parse.
* Gzip / content negotiation cho SBOM & provenance chưa có.
* Lockfile materials ingestion sâu (as materials list) chưa thực hiện.
* Public key rotation metadata chưa.
* Chưa nén (gzip) / content negotiation cho SBOM & provenance.
* Lockfile materials ingestion chưa thực hiện.

## Next-Up / Roadmap (Phase 3)
1. Per-file dependency hash listing or nested components for deeper provenance.
2. Extended CycloneDX sections (services, compositions, vulnerabilities) opt-in parsing.
3. In-toto/SLSA enrichment: builder.id, buildType, invocation/environment, completeness attestations.
4. Backfill job for legacy artifacts (generate SBOM + provenance v2) + dry-run.
5. Public key rotation & expiry metadata + rotation policy doc.
6. Optional gzip + conditional negotiation cho SBOM/provenance.
7. Lockfile materials as provenance materials entries.
8. Ghi nhận tỷ lệ sbom_invalid_total qua PromQL recording rules.
9. (Optional) Per-file reproducibility proofs (component hashes nested) beyond current aggregated approach.

## Phân Công Gợi Ý (Optional)
| Task | Độ ưu tiên | Effort |
|------|-----------|--------|
| CycloneDX generator | Cao | Trung |
| SBOM upload + DB field | Cao | Trung |
| Validation & policy flag | Cao | Trung |
| Provenance v2 (in-toto lite) | Trung | Cao |
| Metrics coverage | Trung | Thấp |
| DSSE Attestation | Thấp | Trung |

## Checklist Chi Tiết (Cập nhật)
- [x] Endpoint phục vụ SBOM `/artifacts/{digest}/sbom`
- [x] Server-side signature enforcement flag
- [x] Chữ ký verify trước deploy
- [x] Provenance tài liệu cơ bản
- [x] SBOM CycloneDX 1.5 output (subset)
- [x] SBOM upload & storage integration
- [x] DB schema: cột `sbom_url`
- [x] Server SBOM validation logic (subset schema + metrics)
- [x] Policy `AETHER_ENFORCE_SBOM` (basic presence)
- [x] Strict deploy enforcement (validated + digest match)
- [x] Metrics coverage (SBOM, signature, provenance gauges)
- [ ] In-toto style provenance nâng cao (v2 partial: materials placeholder only)
- [x] DSSE Attestation bundling (signed if AETHER_ATTESTATION_SK provided)
- [x] Cache headers / ETag SBOM endpoint
- [ ] Public key rotation metadata
- [x] Manifest upload + digest cross-check
- [x] provenance_emitted_total metric
- [x] sbom_invalid_total metric
- [x] Full CycloneDX extended schema (env toggle)
- [x] Provenance fetch/list endpoints
- [x] Lockfile integrity ingestion (npm)

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
Hoàn thành vòng nâng cấp thứ hai: CycloneDX enriched (dependency graph + hashes), SBOM validation (subset schema), provenance v2 + DSSE envelope, coverage metrics & caching. Tiếp theo: full schema integrity, manifest cross-check, dedicated DSSE signing & in-toto/SLSA enrichment.

````