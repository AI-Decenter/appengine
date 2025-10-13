# Issue 09 – Performance & Benchmark Suite: Test-Driven Development (TDD)

This document drives Issue 09 using a failing-first approach, stabilizing performance measurements and preventing regressions via automated checks.

## Goals and scope

- Benchmarks covered: artifact packaging (existing) and streaming upload throughput (new, mock server).
- Artifacts: machine-readable JSON summaries in target/benchmarks/*.json, plus a committed baseline for packaging.
- Regression policy: warn and optionally fail CI if p95 performance regresses by more than 20% versus the committed baseline.

## Contract (inputs/outputs)

- Inputs
  - Bench targets: cargo bench -p aether-cli (specific functions/selectors).
  - Fixed inputs for determinism: payload size, RNG seed, chunk size, warm-up count.
- Outputs
  - JSON per bench: { bench_id, metric, unit, p50, p95, n, timestamp, notes? }.
  - Files
    - Baseline (committed): crates/aether-cli/benches/baseline/bench-pack.json
    - Runtime: target/benchmarks/bench-pack.json, target/benchmarks/bench-stream.json
- Error modes
  - Missing/invalid baseline/current JSON → exit non-zero with clear message
  - Regression threshold exceeded (>20% p95 worse) → exit non-zero; print ::warning:: in CI
- Success criteria
  - All tests pass locally and in CI; regression script behavior locked by fixtures; JSON schema validation enforced in tests.

## Schema and fixtures

- JSON schema (lightweight)
  - Required: bench_id (string), metric ("duration_ms"|"throughput_mbs"), unit ("ms"|"MB/s"), p50 (number), p95 (number), n (integer ≥ 1), timestamp (ISO8601)
  - Optional: notes (string)
- Fixture set (tests/bench-fixtures/)
  - baseline_pack.json, current_pack_{better|+10|+25}.json
  - baseline_stream.json, current_stream_{better|+10|+25}.json

## Test matrix

- T1 Schema validity ✅
  - Given a JSON file, validate required keys and types; fail on missing/invalid (implemented in scripts/check-bench-regression.sh)
- T2 Packaging emit ✅
  - After running the packaging bench, file target/benchmarks/bench-pack.json exists and parses (implemented in crates/aether-cli/benches/pack_bench.rs)
- T3 Packaging metrics ✅
  - p95 ≥ p50, n ≥ 1; metric=duration_ms; unit=ms (validated by schema check and bench output)
- T4 Streaming emit ✅
  - After running the streaming bench, file target/benchmarks/bench-stream.json exists and parses (implemented in crates/aether-cli/benches/stream_bench.rs)
- T5 Streaming metrics ✅
  - throughput_mbs > 0, p95 ≥ p50; metric=throughput_mbs; unit=MB/s (validated by schema check and bench output)
- T6 Regression ok (no-regress) ✅
  - current p95 ≤ baseline p95 × 1.2 → exit code 0 (fixtures covered)
- T7 Regression hard (fail) ✅
  - current p95 > baseline p95 × 1.2 → exit code ≠ 0; diff percentage printed (::warning:: emitted)
- T8 GitHub Actions warning ✅
  - When regression hard, emit ::warning:: lines with details (script emits warnings)
- T9 Missing files ✅
  - Baseline or current file missing → exit code ≠ 0; message lists missing path(s) (script checks presence)
- T10 Aggregate multi-bench ✅
  - When comparing multiple files, exit according to worst-case; print a per-bench summary (script aggregates and prints overall status)

## Failing-first roadmap

1) Write tests for regression script (T6–T10) using static fixtures; ensure failures are explicit and informative
2) Implement scripts/check-bench-regression.sh minimally to pass T6–T10 (no need to run real benches yet)
3) Write tests for packaging bench emission (T2–T3): run selective bench target, assert file exists and schema validates
4) Update packaging bench to emit JSON with fixed inputs (seed/size) and adequate warm-up to reduce noise
5) Write tests for streaming bench (T4–T5): run bench, assert file exists, schema and values are plausible
6) Implement streaming bench (tokio + axum/hyper mock server; client streams chunked payload); tune guardrails
7) CI wiring: run script against fixtures first to lock behavior; then run real benches and compare to baseline; upload artifacts on failure

## Local run cheatsheet

```bash
# 1) Validate regression script behavior with fixtures
bash scripts/check-bench-regression.sh \
  tests/bench-fixtures/baseline_pack.json \
  tests/bench-fixtures/current_pack.json

# 2) Run packaging bench and check its output
cargo bench -p aether-cli -- bench_packaging --quiet
[ -f target/benchmarks/bench-pack.json ]

# 3) Run streaming bench and check its output
cargo bench -p aether-cli -- bench_streaming --quiet
[ -f target/benchmarks/bench-stream.json ]
```

Notes
- Keep criterion warm-up and sample sizes modest on CI; longer locally for stable estimates
- Pin thread counts for reproducibility (e.g., RAYON_NUM_THREADS=2)
- Disable noisy logs during benches

## CI verification plan

- Step 1: Run regression script with fixture pairs to exercise thresholds and missing-file paths (T6–T10) ✅ (benches job)
- Step 2: Run benches with CI profile, produce JSON outputs, compare to baseline; print ::warning:: on regressions ✅ (benches job)
- Always upload target/benchmarks/*.json when job fails to aid debugging ✅ (benches job uploads artifacts unconditionally)
- Consider continue-on-error: true for PRs; enforce on main ✅ (job uses continue-on-error)

## Completion status

- Regression checker implemented: `scripts/check-bench-regression.sh` (schema validation, thresholds, warnings)
- Packaging bench JSON output: `crates/aether-cli/benches/pack_bench.rs`
- Streaming bench JSON output: `crates/aether-cli/benches/stream_bench.rs`
- Baseline committed: `crates/aether-cli/benches/baseline/bench-pack.json`
- Fixtures present under `tests/bench-fixtures/`
- CI wired in `.github/workflows/feature-ci.yml` job “Benchmarks & Regression Guard”

## Definition of Done

- Tests T1–T10 green locally and in CI
- Committed baseline crates/aether-cli/benches/baseline/bench-pack.json present
- When p95 worsens by >20% vs baseline, the check script exits non-zero and CI shows a clear warning message
