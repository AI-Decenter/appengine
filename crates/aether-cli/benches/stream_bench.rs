use criterion::{criterion_group, criterion_main, Criterion};
use std::time::{Duration, Instant};

fn write_json_once(path:&std::path::Path, json:&str){
    if let Some(parent)=path.parent(){ let _=std::fs::create_dir_all(parent); }
    let _ = std::fs::write(path, json);
}

async fn start_server() -> (tokio::task::JoinHandle<()>, std::net::SocketAddr) {
    use axum::{routing::post, Router};
    use axum::http::{Request, StatusCode};
    use axum::body::{Body, to_bytes};
    async fn upload(req: Request<Body>) -> StatusCode {
        // Drain the streamed body (buffers up to payload size)
        let _ = to_bytes(req.into_body(), usize::MAX).await;
        StatusCode::OK
    }
    let app = Router::new().route("/upload", post(upload));
    let listener = tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0)).await.unwrap();
    let addr = listener.local_addr().unwrap();
    let handle = tokio::spawn(async move {
        let _ = axum::serve(listener, app.into_make_service()).await;
    });
    (handle, addr)
}

fn bench_stream(c:&mut Criterion) {
    let mut g = c.benchmark_group("streaming");
    g.measurement_time(Duration::from_secs(3));
    // Fixed payload: 8 MiB in 128 KiB chunks
    let size_bytes: usize = 8 * 1024 * 1024;
    let part: usize = 128 * 1024;
    let rt = tokio::runtime::Builder::new_multi_thread().worker_threads(2).enable_all().build().unwrap();
    // Start mock server once
    let (_jh, addr) = rt.block_on(start_server());
    let url = format!("http://{}/upload", addr);
    let client = reqwest::Client::new();
    let mut thr_samples: Vec<f64> = Vec::new();

    g.bench_function("8MiB_stream_128KiB", |b| {
        b.iter_custom(|iters| {
            let mut total = Duration::ZERO;
            for _ in 0..iters {
                let start = Instant::now();
                rt.block_on(async {
                    use async_stream::stream;
                    let mut sent: usize = 0;
                    let s = stream! {
                        while sent < size_bytes {
                            let n = std::cmp::min(part, size_bytes - sent);
                            let buf = vec![0u8; n];
                            sent += n;
                            yield Ok::<bytes::Bytes, std::io::Error>(bytes::Bytes::from(buf));
                        }
                    };
                    let body = reqwest::Body::wrap_stream(s);
                    let _ = client.post(&url).body(body).send().await.unwrap();
                });
                total += start.elapsed();
            }
            total
        });
    });

    // Collect several runs to compute p50/p95 throughput
    for _ in 0..20 {
        let dur = {
            let start = Instant::now();
            rt.block_on(async {
                use async_stream::stream;
                let mut sent: usize = 0;
                let s = stream! {
                    while sent < size_bytes {
                        let n = std::cmp::min(part, size_bytes - sent);
                        let buf = vec![0u8; n];
                        sent += n;
                        yield Ok::<bytes::Bytes, std::io::Error>(bytes::Bytes::from(buf));
                    }
                };
                let body = reqwest::Body::wrap_stream(s);
                let _ = client.post(&url).body(body).send().await.unwrap();
            });
            start.elapsed().as_secs_f64()
        };
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
        "notes": "mock server streaming throughput (8MiB payload, 128KiB chunks)"
    });
    let out = serde_json::to_string_pretty(&bench).unwrap();
    write_json_once(std::path::Path::new("target/benchmarks/bench-stream.json"), &out);
    g.finish();
}

criterion_group!(benches, bench_stream);
criterion_main!(benches);
