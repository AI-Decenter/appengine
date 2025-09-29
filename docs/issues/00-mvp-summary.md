````markdown
# AetherEngine MVP – Tổng Quan & Tiến Độ Hiện Tại

Phiên bản: 1.0-MVP  
Ngày cập nhật: 26/09/2025

## 1. Tầm nhìn cô đọng
Nền tảng PaaS nội bộ tối ưu tốc độ triển khai (deploy latency) cho ứng dụng Node.js bằng mô hình “client-side build + server-side artifact run”. Toàn bộ build / dependency resolution diễn ra trên máy dev/CI => Control Plane chỉ tiếp nhận artifact đã hoàn thiện → giảm thời gian triển khai từ phút xuống giây.

## 2. Các khối hệ thống
| Khối | Trạng thái | Ghi chú |
|------|------------|---------|
| CLI (Artifact build, SBOM, ký) | Hoàn chỉnh (MVP) | Còn: streaming upload, multi-runtime |
| Control Plane (API + DB + OpenAPI + Metrics) | >100% scope gốc | Thiếu: Auth/RBAC, K8s controller thực thi thật |
| Artifact Upload Endpoint | ĐÃ có (multipart local fs) | Chưa tích hợp MinIO/S3 thật |
| SBOM + Chữ ký | Cơ bản (JSON + Ed25519) | Chưa chuẩn CycloneDX / kiểm tra chữ ký server-side |
| Benchmark Packaging | Có Criterion | Chưa pipeline hóa / regression gate |
| K8s Deployment Runtime | Chưa (chỉ docs) | Sẽ thêm controller / CRD hoặc job apply manifest |

## 3. Thành tựu chính
* Gói artifact (tar+gzip) streaming hash SHA‑256 + manifest chi tiết mỗi file.
* Cache node_modules theo lockfile hash + NODE_VERSION.
* SBOM sơ bộ + file chữ ký Ed25519 (opt-in qua biến env).
* Control Plane: OpenAPI (utoipa), Swagger UI, Prometheus metrics (counter + histogram), error JSON chuẩn, pagination, readiness.
* Upload: Endpoint `/artifacts` (multipart) lưu artifact và trả `artifact_url` (file://…); CLI gọi tiếp `/deployments`.
* Benchmark criterion: packaging scaling (10 / 100 / 500 files).

## 4. Khoảng trống còn lại (Gap Analysis)
| Hạng mục | Ưu tiên | Khoảng trống | Đề xuất |
|----------|---------|--------------|---------|
| Artifact Registry thật (MinIO/S3) | Cao | Đang lưu local fs | Tích hợp presigned PUT + metadata verify |
| K8s thực thi artifact | Cao | Chưa phát hành Deployment/Pod | InitContainer tải + sidecar reload (dev) |
| Auth + RBAC | Cao | Không có | Static token -> JWT/OIDC lộ trình |
| Supply Chain nâng cao | Trung | Chưa verify chữ ký server | Lưu public key + verify digest |
| CycloneDX / SPDX SBOM | Trung | JSON tùy biến | Chuyển sang CycloneDX JSON schema 1.5 |
| Streaming upload lớn | Thấp | Đọc toàn bộ vào RAM | Chuyển sang async stream body |
| Deployment events | Trung | Chưa tồn tại | Bảng events + publish hook |
| Hot reload dev mode | Trung | Chưa có | Sidecar fetch loop + patch annotation |
| Performance guard | Thấp | Benchmark rời rạc | CI job diff threshold |

## 5. Rủi ro chính & Giảm thiểu
| Rủi ro | Ảnh hưởng | Giảm thiểu |
|--------|-----------|-----------|
| Thiếu auth sớm | Lộ API nội bộ | Thêm API key static + audit log |
| Artifact tamper | Runtime chạy mã sửa đổi | Digest + chữ ký + verify server-side |
| Phình kích thước artifact | Chi phí network / start chậm | Ngưỡng cảnh báo + prune + optional compression tuning |
| Không đo lường hiệu năng | Khó chứng minh 80% giảm thời gian | Thu thập baseline cũ + lưu benchmark lịch sử |
| Tăng độ phức tạp K8s | Chậm release | Bắt đầu đơn giản: Deployment + init container |

## 6. Roadmap rút gọn (6 tuần)
| Tuần | Trọng tâm | Kết quả mong đợi |
|------|-----------|------------------|
| 1 | MinIO integration + presigned upload | Artifact URL S3 style, metadata persist |
| 2 | K8s deploy controller (naive) | Pod chạy artifact thật |
| 3 | Auth (static keys) + verify signature | Block artifact chưa hợp lệ |
| 4 | CycloneDX SBOM + server validation | Chuỗi cung ứng minh bạch cơ bản |
| 5 | Hot reload dev loop + metrics mở rộng | DX nâng cao |
| 6 | Benchmark gating + event table + docs hoàn thiện | Sẵn sàng Beta nội bộ |

## 7. Bộ chỉ số MVP đề xuất
| Metric | Định nghĩa | Mục tiêu |
|--------|-----------|---------|
| Deploy Time p95 | CLI start → Pod Ready | < 15s nội bộ |
| Artifact Size trung vị | Bytes | < 60MB (service chuẩn) |
| Hash verify failure rate | % artifact bị từ chối | 0 (sau enable verify) |
| CrashLoop deploy mới | % trong 24h | < 2% |
| SBOM coverage | % artifact có SBOM hợp lệ | 100% |

## 8. Các Issue hoạt động (thay thế bộ cũ)
Được đặc tả lại trong các file `01-...` đến `10-...` cùng thư mục này.

## 9. Liên kết chéo chính
* `dev.sh` – script thiết lập & hot reload dev.
* `scripts/gen-ts-sdk.sh` – sinh TypeScript SDK.
* `crates/aether-cli` – logic artifact, SBOM, ký.
* `crates/control-plane` – API + metrics + OpenAPI.

---
_Tài liệu này được cập nhật cùng mỗi mốc phát hành nhỏ (minor internal tag)._  

````