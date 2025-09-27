use assert_cmd::Command;

fn bin() -> Command { Command::cargo_bin("aether-cli").unwrap() }

#[test]
fn deploy_in_non_node_project_fails_usage() {
    let tmp = tempfile::tempdir().unwrap();
    let assert = bin()
        .current_dir(tmp.path())
        .env("XDG_CACHE_HOME", tmp.path())
        .env("XDG_CONFIG_HOME", tmp.path())
        .arg("deploy")
        .assert()
        .failure();
    let code = assert.get_output().status.code().unwrap();
    assert_eq!(code, 2, "expected usage exit code 2 for non-node project");
}

#[test]
fn deploy_pack_only_creates_artifact() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(tmp.path().join("package.json"), "{}" ).unwrap();
    std::fs::write(tmp.path().join("index.js"), "console.log('hi')" ).unwrap();
    bin().current_dir(tmp.path())
        .env("XDG_CACHE_HOME", tmp.path())
        .env("XDG_CONFIG_HOME", tmp.path())
        .args(["deploy","--pack-only"]) 
        .assert()
        .success();
    // ensure artifact exists
    let mut found=false; for e in std::fs::read_dir(tmp.path()).unwrap() { let p=e.unwrap().path(); if p.file_name().unwrap().to_string_lossy().starts_with("app-") { found=true; break; } }
    assert!(found, "expected app-*.tar.gz artifact");
}
