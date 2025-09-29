````markdown
# Issue 04: Kubernetes Deployment Controller (Init + PodSpec)

## Mục tiêu
Triển khai artifact thật sự trên Kubernetes sử dụng init container fetch & extract.

## Scope
* Module mới `k8s` trong control-plane: hàm `apply_deployment(app, digest, artifact_url)`.
* PodSpec: init container busybox tải + giải nén vào EmptyDir `/workspace`.
* Container chính image `aether-nodejs:20-slim` chạy `node server.js` hoặc script từ package.json nếu có.
* Annotation `aether.dev/digest=<digest>`.

## Acceptance
| ID | Mô tả | Kết quả |
|----|------|---------|
| K1 | Tạo deployment mới | PARTIAL: Deployment apply + poller cập nhật running khi availableReplicas>=1 |
| K2 | Cập nhật digest khác | PARTIAL: Annotation digest đổi -> rollout; chưa test tự động + DB audit |
| K3 | Artifact 404 | TODO: Chưa đánh dấu failed (cần đọc Pod init container status) |

## Test
* Unit: build manifest annotation digest (done).  
* Integration (mock / real cluster) PENDING.  
* Manual E2E: microk8s apply, logs init container (NEXT).

## Progress Notes (Phase 2)
Đã triển khai:
* `k8s::apply_deployment` dùng server-side apply tạo/cập nhật Deployment.
* Hook trong API `create_deployment` spawn apply async (fire-and-forget).
* Poller nền 15s: chuyển `pending` -> `running` khi Deployment có `availableReplicas >=1`.
* Annotation: `aether.dev/digest`, `aether.dev/artifact-url`.

## Next Up
1. Derive digest thật (truy vấn bảng artifacts bằng artifact_url hoặc mapping) thay vì parse URL.
2. Failure detection: đọc Pod list / init container state nếu thất bại download -> `failed` (K3).
3. Bổ sung label `app_name=<app>` (hiện chỉ có `app`).
4. Thêm verify SHA256 trước giải nén (sha256sum & so sánh với digest).
5. Thêm endpoint /deployments/:id/status (hoặc mở rộng list) phản ánh thời gian cập nhật gần nhất.
6. Test integration mock: feature gate để tách kube real client (e.g. trait abstraction).
7. Rollout audit: ghi sự kiện vào bảng phụ (optional) khi digest thay đổi.
8. Tune poller: exponential backoff hoặc watch API (stream) thay vì polling thô.

````