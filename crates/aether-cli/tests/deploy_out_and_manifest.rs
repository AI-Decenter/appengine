use assert_cmd::prelude::*;
use std::process::Command;
use tempfile::tempdir;
use std::fs;

#[test]
fn deploy_out_and_manifest() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("package.json"), "{\n  \"name\": \"demo\", \n  \"version\": \"1.0.0\"\n}").unwrap();
    fs::write(dir.path().join("index.js"), "console.log('hi')").unwrap();
    // Use pack-only to avoid needing npm
    let out_dir = dir.path().join("out"); fs::create_dir_all(&out_dir).unwrap();
    let mut cmd = Command::cargo_bin("aether-cli").unwrap();
    let cfg_path = dir.path().join("cfg.toml"); std::fs::write(&cfg_path, "").unwrap();
    cmd.current_dir(dir.path())
        .env("AETHER_CONFIG_FILE", &cfg_path)
        .arg("deploy").arg("--pack-only").arg("--out").arg(out_dir.to_string_lossy().to_string());
    let assert = cmd.assert();
    let status = assert.get_output().status.code().unwrap_or(1);
    assert_eq!(status, 0, "deploy command should succeed");
    // Find artifact
    let entries: Vec<_> = fs::read_dir(&out_dir).unwrap().filter_map(|e| e.ok()).collect();
    assert!(!entries.is_empty(), "expected artifact in out dir");
    let tar = entries.iter().find(|e| e.file_name().to_string_lossy().ends_with(".tar.gz")).expect("tar.gz not found");
    let manifest = out_dir.join(format!("{}", tar.file_name().to_string_lossy()) + ".manifest.json");
    assert!(manifest.exists(), "manifest should exist");
    let manifest_content = fs::read_to_string(&manifest).unwrap();
    assert!(manifest_content.contains("index.js"), "manifest should list index.js");
}
