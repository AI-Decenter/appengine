use criterion::{criterion_group, criterion_main, Criterion};
use std::fs;use std::time::Duration;
use std::time::Instant;

fn write_json_once(path:&std::path::Path, json:&str){
    if let Some(parent)=path.parent(){ let _=std::fs::create_dir_all(parent); }
    let _ = std::fs::write(path, json);
}

fn setup_temp(n:usize)->tempfile::TempDir { let dir = tempfile::tempdir().unwrap(); for i in 0..n { fs::write(dir.path().join(format!("file{i}.txt")), "x".repeat(1024)).unwrap(); } fs::write(dir.path().join("package.json"), "{}" ).unwrap(); dir }

fn bench_pack(c:&mut Criterion) {
    // Keep CI-friendly: short measurement but deterministic inputs
    let mut g = c.benchmark_group("packaging");
    g.measurement_time(Duration::from_secs(3));
    // Fixed case: 100 files of 1KiB for determinism
    let n:usize = 100;
    let mut times: Vec<f64> = Vec::new();
    g.bench_function("files_100", |b| {
        b.iter_custom(|iters| {
            let mut total = std::time::Duration::ZERO;
            for _ in 0..iters {
                let tmp = setup_temp(n);
                let start = Instant::now();
                let root = tmp.path(); let patterns:Vec<glob::Pattern>=Vec::new();
                let (_paths,_d,_m)= aether_cli::commands::deploy::collect_for_bench(root, &patterns);
                total += start.elapsed();
                // collect_for_bench does I/O; tmp dropped here
            }
            total
        });
    });
    // Criterion report JSON is separate; we also emit a stable summary for CI
    // We can't grab per-iter times from Criterion directly without custom measurement; approximate with a single run here
    // Perform a quick local sampling to compute a p50/p95 proxy
    for _ in 0..20 { // small sample
        let tmp = setup_temp(n);
        let start = Instant::now();
        let root = tmp.path(); let patterns:Vec<glob::Pattern>=Vec::new();
        let (_paths,_d,_m)= aether_cli::commands::deploy::collect_for_bench(root, &patterns);
        times.push(start.elapsed().as_secs_f64() * 1000.0);
    }
    times.sort_by(|a,b| a.partial_cmp(b).unwrap());
    let p50 = times[((times.len() as f64 * 0.50).floor() as usize).min(times.len()-1)];
    let p95 = times[((times.len() as f64 * 0.95).floor() as usize).min(times.len()-1)];
    let bench = serde_json::json!({
        "bench_id": "packaging",
        "metric": "duration_ms",
        "unit": "ms",
        "p50": p50,
        "p95": p95,
        "n": times.len(),
        "timestamp": chrono::Utc::now().to_rfc3339(),
        "notes": "aether-cli pack bench (files=100)"
    });
    let out = serde_json::to_string_pretty(&bench).unwrap();
    write_json_once(std::path::Path::new("target/benchmarks/bench-pack.json"), &out);
    g.finish();
}

criterion_group!(benches, bench_pack);criterion_main!(benches);
