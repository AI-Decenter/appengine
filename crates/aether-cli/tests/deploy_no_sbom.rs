use assert_cmd::Command;use std::fs;

fn bin()->Command { Command::cargo_bin("aether-cli").unwrap() }

#[test]
fn deploy_skips_sbom_when_flag_set() {
    let tmp = tempfile::tempdir().unwrap(); let root = tmp.path();
    fs::write(root.join("package.json"), "{}" ).unwrap();
    fs::write(root.join("index.js"), "console.log('x')" ).unwrap();
    bin().current_dir(root)
        .env("XDG_CACHE_HOME", root)
        .env("XDG_CONFIG_HOME", root)
        .args(["deploy","--pack-only","--no-sbom","--format","json"]) 
        .assert()
        .success();
    // ensure there is an artifact
    let mut artifact: Option<std::path::PathBuf> = None;
    for e in fs::read_dir(root).unwrap() { let p = e.unwrap().path(); if p.extension().and_then(|s| s.to_str())==Some("gz") { artifact = Some(p); break; } }
    let artifact = artifact.expect("artifact missing");
    let stem = artifact.file_name().unwrap().to_string_lossy().to_string();
    let sbom = root.join(format!("{}.sbom.json", stem));
    assert!(!sbom.exists(), "SBOM should be skipped");
}
