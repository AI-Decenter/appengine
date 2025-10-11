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

- [x] Inventory existing benches
	- Đã rà soát `crates/aether-cli/benches` và bổ sung output JSON còn thiếu.
- [x] Define JSON baseline schema
	- Schema tối giản: `{ bench_id, metric, unit, p50, p95, n, timestamp, notes }` (đã áp dụng trong script/benches).
	- Baseline commit: `crates/aether-cli/benches/baseline/bench-pack.json`.
	- Runtime outputs: `crates/aether-cli/target/benchmarks/*.json`.
- [x] Emit baseline from packaging bench
	- `crates/aether-cli/benches/pack_bench.rs` ghi `bench-pack.json` với input cố định.
- [x] Add streaming upload benchmark
	- `crates/aether-cli/benches/stream_bench.rs` chạy mock server (axum) + client stream; ghi `bench-stream.json`.
- [x] Regression check script
	- `scripts/check-bench-regression.sh` so sánh p95 với baseline; exit non‑zero khi >20%; in `::warning::`. Có kiểm tra schema cơ bản.
- [x] CI wiring for benches
	- Thêm job "Benchmarks & Regression Guard" trong `.github/workflows/feature-ci.yml`: chạy fixtures, chạy benches, so sánh, upload artifacts.
- [ ] Docs: how to run/update
	- README: thêm hướng dẫn chạy benches, vị trí JSON, cập nhật baseline, ngưỡng regression.
- [ ] Stabilization guardrails
	- Cố định input/warm-up, pin thread (`RAYON_NUM_THREADS=2`), note governor CPU (nếu self-hosted). Cần bổ sung docs và biến env trong job.
- [x] Deliver acceptance artifacts
	- B1: baseline `bench-pack.json` đã commit. B2: script trả exit non‑zero khi vượt ngưỡng và CI cảnh báo.

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