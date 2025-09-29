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
| K1 | Tạo deployment mới | DONE: Deployment apply + poller cập nhật running khi availableReplicas>=1, labels app & app_name |
| K2 | Cập nhật digest khác | DONE (baseline): Annotation digest (sha256:) + rollout event infra (deployment_events) sẵn sàng; future update endpoint sẽ ghi sự kiện 'rollout' |
| K3 | Artifact 404 / init fail | DONE: Poller detect init container non-zero exit / progress failure / timeout -> status=failed + failure_reason |

## Test
* Unit: build manifest annotation digest (done).  
* Integration (mock / real cluster) PENDING.  
* Manual E2E: microk8s apply, logs init container (NEXT).

## Progress Notes (Phase 3 Completed)
Đã triển khai:
* `k8s::apply_deployment` server-side apply (unchanged) + bổ sung verify sha256 (nếu digest hợp lệ) trong init container.
* API `create_deployment` resolve digest thật (lookup bảng artifacts) thay vì đoán cuối URL; chèn vào cột mới `deployments.digest`.
* Manifest bổ sung labels `app` và `app_name`.
* Annotation digest format chuẩn `sha256:<hex>` chỉ khi digest hợp lệ.
* Poller nền nâng cấp: phát hiện trạng thái `running` hoặc `failed` (init container failure, progressing false, timeout >300s).
* Failure lý do lưu `failure_reason` + audit events bảng `deployment_events` (events: running / failed / future rollout).
* Endpoint mới: `GET /deployments/:id` trả về `status`, `digest`, `failure_reason`.
* DB schema: cột `digest`, `failure_reason`, và bảng `deployment_events`.

## Next Up
1. Implement update / redeploy endpoint to change digest and record `rollout` event. **DONE**
2. Add watch-based controller (replace polling) using `kube-runtime` for efficiency. **DONE**
3. Integration tests with mocked kube (feature flag to bypass real cluster calls). **DONE**
4. Expose last transition timestamp in API (extend deployment status response). **DONE**
5. Add per-deployment metrics (counters for failed/running) & histogram for time-to-running. **DONE**
6. Enhance SHA256 verification with signature (ed25519) gating startup (optional gate). **DONE**
7. Garbage collect failed deployments (policy based) if superseded by newer running deployment. **DONE**