````markdown
# Issue 08: Dev Environment Automation + Hot Reload Script

## Scope
* Mở rộng `dev.sh`: thêm subcommand: `k8s-start`, `deploy-sample`, `hot-upload`, `hot-patch`.
* Tạo sample Node app + artifact upload + deployment apply.
* Hot reload: tar thư mục sample, upload MinIO (mc), patch annotation.

## Acceptance
| ID | Mô tả | Kết quả |
|----|------|---------|
| D1 | deploy-sample chạy thành công | Pod Running |
| D2 | hot-upload + hot-patch -> digest thay đổi | Sidecar fetch loop tải mới |

````