use assert_cmd::Command;use std::{fs};

fn bin()->Command { Command::cargo_bin("aether-cli").unwrap() }

#[test]
fn deploy_generates_sbom_and_signature_when_key_present() {
    // create minimal project
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    fs::write(root.join("package.json"), "{\n  \"name\": \"demo\", \n  \"version\": \"1.2.3\"\n}").unwrap();
    fs::write(root.join("index.js"), "console.log('hi')").unwrap();
    // 32-byte (64 hex chars) deterministic key (all 0xaa)
    let key = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"; // 32 bytes (0xaa * 32) in hex
    bin().current_dir(root)
        .env("AETHER_SIGNING_KEY", key)
        .env("XDG_CACHE_HOME", root)
        .env("XDG_CONFIG_HOME", root)
        .arg("deploy").arg("--pack-only")
        .assert().success();
    // locate artifact
    let mut artifact: Option<std::path::PathBuf> = None;
    for e in fs::read_dir(root).unwrap() { let p = e.unwrap().path(); if p.extension().and_then(|s| s.to_str())==Some("gz") { artifact = Some(p); break; } }
    let artifact = artifact.expect("artifact not found");
    let stem = artifact.file_name().unwrap().to_string_lossy().to_string();
    let sbom = root.join(format!("{}.sbom.json", stem));
    let sig = root.join(format!("{}.sig", stem));
    assert!(sbom.exists(), "SBOM file should exist");
    assert!(sig.exists(), "signature file should exist");
    let sig_content = fs::read_to_string(&sig).unwrap();
    assert_eq!(sig_content.len(), 128, "ed25519 signature hex length");
    let sbom_content = fs::read_to_string(&sbom).unwrap();
    assert!(sbom_content.contains("\"schema\":"));
    assert!(sbom_content.contains("demo"));
}
