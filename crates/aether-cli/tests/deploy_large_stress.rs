use assert_cmd::Command;use std::fs;

fn bin()->Command { Command::cargo_bin("aether-cli").unwrap() }

// Ignored heavy test ~ creates sparse-like large file to simulate >200MB packaging
#[test]
#[ignore]
fn deploy_large_stress_over_200mb() {
    let tmp = tempfile::tempdir().unwrap(); let root = tmp.path();
    fs::write(root.join("package.json"), "{}" ).unwrap();
    // create one large file ~210MB (uncompressed) of repetitive data for speed
    let big_path = root.join("big.dat");
    // allocate 210 * 1024 * 1024 bytes (~220MB) quickly
    let data = vec![0u8; 210 * 1024 * 1024];
    fs::write(&big_path, data).unwrap();
    bin().current_dir(root)
        .env("XDG_CACHE_HOME", root)
        .env("XDG_CONFIG_HOME", root)
        .args(["deploy","--pack-only","--format","json"]) 
        .assert()
        .success();
}
