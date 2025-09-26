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
| K1 | Tạo deployment mới | Pod Running, label `app_name` |
| K2 | Cập nhật digest khác | Rollout mới (recreate) |
| K3 | Artifact 404 | Status deployment DB `failed` |

## Test
* Integration (mock k8s client optional).  
* Manual E2E: microk8s apply, logs init container.

````