````markdown
# Issue 06: SBOM & Supply Chain Security mở rộng

## Scope
* Xuất SBOM CycloneDX JSON 1.5 (dependencies + files).
* Gắn SBOM URL vào artifact record.
* Control Plane: endpoint `GET /artifacts/{digest}/sbom` proxy / redirect.
* Server verify chữ ký artifact khi enable (env flag) – fail -> reject deployment.

## Acceptance
| ID | Mô tả | Kết quả |
|----|------|---------|
| S1 | SBOM hợp lệ validator | Pass |
| S2 | Chữ ký sai | 400 reject deploy |

````