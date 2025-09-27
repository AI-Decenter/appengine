use criterion::{criterion_group, criterion_main, Criterion};
use std::io::{Read};
use std::fs;

// Simple benchmark to simulate different buffer sizes when hashing a large file.
fn create_large_temp()->(tempfile::TempDir, std::path::PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("large.bin");
    let data = vec![0u8; 8 * 1024 * 1024]; // 8MB
    fs::write(&path, data).unwrap();
    (dir, path)
}

fn bench_chunk(c:&mut Criterion) {
    let (dir, path) = create_large_temp();
    let mut g = c.benchmark_group("hash_chunk_sizes");
    for &size in &[64*1024usize, 128*1024, 256*1024] {
        g.bench_with_input(format!("chunk_{size}"), &size, |b,&sz| {
            b.iter(|| {
                use sha2::{Sha256, Digest};
                let mut f = fs::File::open(&path).unwrap();
                let mut buf = vec![0u8; sz];
                let mut h = Sha256::new();
                loop { match f.read(&mut buf) { Ok(0)=>break, Ok(n)=> h.update(&buf[..n]), Err(_)=>break } }
                let _dgst = h.finalize();
            });
        });
    }
    drop(dir);
    g.finish();
}

criterion_group!(benches, bench_chunk);
criterion_main!(benches);
