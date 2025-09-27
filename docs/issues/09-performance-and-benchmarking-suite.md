````markdown
# Issue 09: Performance & Benchmark Suite

## Scope
* Benchmark artifact packaging (đã có) → lưu JSON baseline trong `target/benchmarks/`.
* Thêm benchmark: streaming upload throughput giả lập (mock server).
* CI: nếu regression >20% p95 → cảnh báo.

## Acceptance
| ID | Mô tả | Kết quả |
|----|------|---------|
| B1 | Baseline file commit | Có `bench-pack.json` |
| B2 | Regression check script | Exit non‑zero khi vượt ngưỡng |

````