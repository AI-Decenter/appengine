````markdown
# Issue 07: Observability & Metrics mở rộng

## Scope
* Thêm tracing ID propagation CLI → server (header `X-Trace-Id`).
* Metrics: gauge số deployment running; counter artifact verify failures; histogram deploy latency (receipt→PodReady).
* Logging: thêm request_id, digest.

## Acceptance
| ID | Mô tả | Kết quả |
|----|------|---------|
| O1 | Trace id log cả hai phía | Có |
| O2 | Histogram xuất Prometheus | Có buckets |

````