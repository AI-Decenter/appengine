use assert_cmd::Command;use std::fs;
fn bin()->Command { Command::cargo_bin("aether-cli").unwrap() }

#[test]
fn sbom_includes_dependencies_and_metadata() {
    let tmp = tempfile::tempdir().unwrap(); let root = tmp.path();
    fs::write(root.join("package.json"), "{\n  \"name\": \"dep-demo\", \n  \"version\": \"1.0.0\", \"dependencies\": { \"leftpad\": \"1.0.0\", \"lodash\": \"^4.17.0\" }\n}").unwrap();
    fs::write(root.join("index.js"), "console.log('hi')").unwrap();
    bin().current_dir(root)
        .env("XDG_CACHE_HOME", root)
        .env("XDG_CONFIG_HOME", root)
        .args(["deploy","--pack-only"])
        .assert().success();
    // find sbom file
    let mut sbom: Option<std::path::PathBuf> = None; for e in fs::read_dir(root).unwrap() { let p=e.unwrap().path(); if let Some(name)=p.file_name().and_then(|s| s.to_str()) { if name.starts_with("app-") && name.ends_with(".tar.gz.sbom.json") { sbom=Some(p); break; } } }
    let sbom_path = sbom.expect("sbom missing");
    let content = fs::read_to_string(&sbom_path).unwrap();
    assert!(content.contains("aether-sbom-v1"));
    assert!(content.contains("leftpad"));
    assert!(content.contains("manifest_digest"));
    assert!(content.contains("total_files"));
}
