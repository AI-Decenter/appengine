````markdown
# Issue 07: Observability & Metrics mở rộng

## Scope
* Tracing ID propagation CLI → server (header `X-Trace-Id`).
* Metrics: 
  - Gauge số deployment running (`deployments_running_total`).
  - Counter artifact verify failures (`artifact_verify_failure_total{app,reason}`).
  - Histogram deploy latency receipt→Running (`deployment_time_to_running_seconds`).
  - Existing HTTP metrics enriched with normalized path + outcome.
* Logging: thêm `request_id`, `trace_id`, chuẩn hoá field `digest`.

## Implementation Details (Completed)
1. Middleware `trace_layer` (Axum) tạo `trace_id` nếu client không gửi và luôn tạo `request_id`; thêm vào span fields & response headers.
2. CLI `deploy` tạo một UUID per-run và gửi trong tất cả các request upload / presign / complete / deployment tạo bằng header `X-Trace-Id`.
3. Metric mới:
	- `deployments_running_total` (IntGauge) cập nhật khi transition `running` hoặc `failed` (recalc COUNT(*) WHERE status='running').
	- `artifact_verify_failure_total{app,reason}` tăng khi signature verification thất bại.
4. Reused / existing:
	- `deployment_time_to_running_seconds` histogram đã có (Issue 07 yêu cầu) ghi lại thời gian từ insert → running.
	- `deployment_status_total{status}` counter cho transitions.
5. Request logging chuẩn hoá: mỗi request log có span `http.req` với: method, path (normalized), raw_path, trace_id, request_id, status, outcome, took_ms.
6. HTTP latency & count metrics được cập nhật trong middleware (thay vì rải rác handlers) để tránh trùng logic.
7. Normalization rule (UUID & digits → :id; `/apps/<app>/...` → app token) tái sử dụng từ Issue 06 cho cardinality control.
8. Propagation: server echo lại `X-Trace-Id` & `X-Request-Id` trong response → dễ correlate ở CLI / logs.

## Example Log Line (Structured)
```
{"level":"INFO","span":"http.req","method":"POST","path":"/deployments","trace_id":"c8e0...","request_id":"6c2f...","status":201,"took_ms":42,"outcome":"success","message":"request.complete"}
```

## Metrics Summary
| Metric | Type | Labels | Description |
|--------|------|--------|-------------|
| http_requests_total | counter | method,path,status,outcome | Request volume |
| http_request_duration_seconds | histogram | method,path | Request latency |
| deployments_running_total | gauge | - | Active running deployments |
| deployment_status_total | counter | status | Transition counts (running, failed) |
| deployment_time_to_running_seconds | histogram | - | Time creation→running |
| artifact_verify_failure_total | counter | app,reason | Signature / artifact verification errors |

## Follow-ups / Enhancements (Future)
* Add gauge for pending deployments & derived saturation ratio.
* Add Prometheus rule for error budget: 5xx rate from http_requests_total (status >=500) / total.
* Correlate provenance wait time with deploy latency (composite histogram or exemplars with trace_id).
* Export OpenTelemetry trace context (propagate W3C traceparent) alongside custom trace id.
* Add `deploy_blocked_total` counter (Issue 06 pending) for policy enforcement failures and integrate into dashboards.

## Testing Notes
* Existing integration tests (`create_deployment_201`) still pass with middleware in place.
* Middleware safe for tests lacking headers (auto-generate IDs).
* Signature failure path covered indirectly; recommend adding a targeted test to assert `artifact_verify_failure_total` increments (future work).

## Acceptance
| ID | Mô tả | Kết quả |
|----|------|---------|
| O1 | Trace id log cả hai phía | Có |
| O2 | Histogram xuất Prometheus | Có buckets |
| O3 | Gauge running deployments | Có |
| O4 | Trace id propagation end-to-end | Có |
| O5 | Artifact verify failure counter | Có |
| O6 | Request/response IDs in logs | Có |

````