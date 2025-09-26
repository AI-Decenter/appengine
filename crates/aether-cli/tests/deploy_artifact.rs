use assert_cmd::Command;use std::{fs};use flate2::read::GzDecoder;use tar::Archive;use std::path::PathBuf;

fn bin()->Command { Command::cargo_bin("aether-cli").unwrap() }

#[test]
fn deploy_creates_artifact_and_respects_ignore() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    fs::write(root.join("package.json"), "{}").unwrap();
    fs::write(root.join("include.txt"), "hello").unwrap();
    fs::write(root.join("secret.env"), "PASSWORD=123").unwrap();
    fs::write(root.join(".aetherignore"), "secret.env\n").unwrap();
    bin().current_dir(root)
        .env("XDG_CACHE_HOME", tmp.path())
        .env("XDG_CONFIG_HOME", tmp.path())
        .args(["deploy"]) 
        .assert()
        .success();
    // find artifact file
    let mut artifact: Option<PathBuf> = None;
    for e in fs::read_dir(root).unwrap() { let p = e.unwrap().path(); if p.file_name().unwrap().to_string_lossy().starts_with("artifact-") { artifact = Some(p); break; } }
    let artifact = artifact.expect("artifact not created");
    let f = fs::File::open(&artifact).unwrap();
    let dec = GzDecoder::new(f); let mut ar = Archive::new(dec);
    let mut has_secret = false; let mut has_include = false;
    for entry in ar.entries().unwrap() {
        let entry = entry.unwrap();
        if let Ok(path) = entry.path() {
            let s = path.to_string_lossy().to_string();
            if s.ends_with("secret.env") { has_secret = true; }
            if s.ends_with("include.txt") { has_include = true; }
        }
    }
    assert!(has_include, "expected included file present");
    assert!(!has_secret, "ignored file should not be in artifact");
}
