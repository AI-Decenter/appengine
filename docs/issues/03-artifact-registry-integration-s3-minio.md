````markdown
# Issue 03: Tích hợp Artifact Registry (MinIO/S3 Presigned URL)

**Loại:** `feat`  
**Phụ thuộc:** 02 (DB lưu artifact)

## 1. Mục tiêu
Chuyển lưu trữ local → MinIO (hoặc S3) dùng presigned upload URL, tách IO khỏi Control Plane.

## 2. Scope
* Endpoint `POST /artifacts/presign` trả về URL + headers upload.
* CLI: gọi presign → PUT trực tiếp đến MinIO → notify Control Plane `POST /artifacts/complete`.
* Cấu trúc key: `artifacts/<app>/<digest>/app.tar.gz`.

## 3. Acceptance
| ID | Điều kiện | Kết quả |
|----|-----------|---------|
| P1 | Presign request không app | 400 |
| P2 | Upload xong notify | 200, artifact trạng thái `stored` |
| P3 | Upload lại digest | 200 idempotent |

## 4. Test
* Mock MinIO (local container).  
* Integration: presign -> put -> complete -> deployment create reference.

````