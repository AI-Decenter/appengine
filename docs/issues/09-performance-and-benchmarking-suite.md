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

## Tasks (checklist)

- [ ] Inventory existing benches
	- Scan `crates/aether-cli/benches` (và liên quan) để xác nhận benchmark packaging hiện có, định danh output hiện tại và khoảng trống cho throughput bench.
- [ ] Define JSON baseline schema
	- Tối giản: `{ bench_id, metric, unit, p50, p95, n, timestamp, notes }`.
	- Baseline commit trong repo: `crates/aether-cli/benches/baseline/bench-pack.json`.
	- Runtime outputs: `target/benchmarks/*.json`.
- [ ] Emit baseline from packaging bench
	- Cập nhật benchmark packaging để ghi JSON summary vào `target/benchmarks/bench-pack.json` với input xác định (seed/size cố định).
- [ ] Add streaming upload benchmark
	- Criterion bench spin up mock HTTP server (tokio + axum/hyper), client stream chunked bytes; đo MB/s; ghi JSON `bench-stream.json`.
- [ ] Regression check script
	- `scripts/check-bench-regression.sh` so sánh p95 hiện tại với baseline; exit non‑zero nếu regression > 20%. In diff rõ ràng và phát `::warning::` khi chạy trong GitHub Actions.
- [ ] CI wiring for benches
	- Workflow job chạy benches, upload JSON artifact và gọi regression script. Ổn định runtime: giới hạn thread, warm-up Criterion, tắt log ồn.
- [ ] Docs: how to run/update
	- README: cách chạy benches cục bộ, nơi JSON được tạo, cách cập nhật baseline, giải thích ngưỡng regression.
- [ ] Stabilization guardrails
	- Cố định input/lần warm-up, pin thread (ví dụ `RAYON_NUM_THREADS=2`), hướng dẫn governor CPU cho runner tự host (tùy chọn).
- [ ] Deliver acceptance artifacts
	- B1: commit `bench-pack.json` (baseline). B2: script trả exit non‑zero khi p95 giảm >20%.

## Plan & timeline (1 sprint ~ 1 tuần)

- Ngày 1: Inventory + Baseline schema (Tasks 1–2)
- Ngày 2: Packaging bench xuất JSON (Task 3)
- Ngày 3–4: Streaming upload benchmark (Task 4)
- Ngày 5: Regression script (Task 5)
- Ngày 6: CI wiring + Stabilization guardrails (Tasks 6, 8)
- Ngày 7: Docs + Deliverables (Tasks 7, 9)

## Acceptance mapping

- B1 → Tasks 2, 3, 9 (có `bench-pack.json` được commit)
- B2 → Tasks 5, 6, 9 (script fail >20% p95 regression; CI hiển thị cảnh báo)

````