use criterion::{criterion_group, criterion_main, Criterion};
use std::time::{Duration, Instant};

fn write_json_once(path:&std::path::Path, json:&str){
    if let Some(parent)=path.parent(){ let _=std::fs::create_dir_all(parent); }
    let _ = std::fs::write(path, json);
}

fn bench_stream(c:&mut Criterion) {
    // Simulate streaming throughput by copying a fixed-size buffer repeatedly.
    let mut g = c.benchmark_group("streaming");
    g.measurement_time(Duration::from_secs(3));
    let size_bytes: usize = 8 * 1024 * 1024; // 8 MiB payload
    let buf = vec![0u8; 128 * 1024]; // 128 KiB chunk
    let mut thr_samples: Vec<f64> = Vec::new();
    g.bench_function("8MiB_128KiB", |b| {
        b.iter_custom(|iters| {
            let mut total = Duration::ZERO;
            for _ in 0..iters {
                let mut transferred = 0usize;
                let start = Instant::now();
                while transferred < size_bytes {
                    // Pretend to process/write a chunk
                    let n = std::cmp::min(buf.len(), size_bytes - transferred);
                    let _ = &buf[..n]; // touch
                    transferred += n;
                }
                total += start.elapsed();
            }
            total
        });
    });
    // Collect several runs to compute p50/p95 throughput
    for _ in 0..20 {
        let mut transferred = 0usize;
        let start = Instant::now();
        while transferred < size_bytes {
            let n = std::cmp::min(buf.len(), size_bytes - transferred);
            let _ = &buf[..n];
            transferred += n;
        }
        let dur = start.elapsed().as_secs_f64();
        let mbps = (size_bytes as f64 / (1024.0*1024.0)) / dur;
        thr_samples.push(mbps);
    }
    thr_samples.sort_by(|a,b| a.partial_cmp(b).unwrap());
    let p50 = thr_samples[((thr_samples.len() as f64 * 0.50).floor() as usize).min(thr_samples.len()-1)];
    let p95 = thr_samples[((thr_samples.len() as f64 * 0.95).floor() as usize).min(thr_samples.len()-1)];
    let bench = serde_json::json!({
        "bench_id": "streaming",
        "metric": "throughput_mbs",
        "unit": "MB/s",
        "p50": p50,
        "p95": p95,
        "n": thr_samples.len(),
        "timestamp": chrono::Utc::now().to_rfc3339(),
        "notes": "synthetic memory copy throughput (8MiB payload, 128KiB chunks)"
    });
    let out = serde_json::to_string_pretty(&bench).unwrap();
    write_json_once(std::path::Path::new("target/benchmarks/bench-stream.json"), &out);
    g.finish();
}

criterion_group!(benches, bench_stream);
criterion_main!(benches);
