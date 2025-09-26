use criterion::{criterion_group, criterion_main, Criterion, black_box};
use std::fs;use std::time::Duration;

fn setup_temp(n:usize)->tempfile::TempDir { let dir = tempfile::tempdir().unwrap(); for i in 0..n { fs::write(dir.path().join(format!("file{i}.txt")), "x".repeat(1024)).unwrap(); } fs::write(dir.path().join("package.json"), "{}" ).unwrap(); dir }

fn bench_pack(c:&mut Criterion) {
    let mut g = c.benchmark_group("pack"); g.measurement_time(Duration::from_secs(5));
    for &n in &[10usize,100,500] { g.bench_with_input(format!("files_{n}"), &n, |b,&n| { let tmp = setup_temp(n); b.iter(|| { let root = tmp.path(); let patterns:Vec<glob::Pattern>=Vec::new(); let (_paths,_d,_m)= aether_cli::commands::deploy::collect_for_bench(root, &patterns); black_box(()); }); }); }
    g.finish();
}

criterion_group!(benches, bench_pack);criterion_main!(benches);
