use assert_cmd::Command;use std::fs;
fn bin()->Command { Command::cargo_bin("aether-cli").unwrap() }

#[test]
fn deploy_json_output_contains_paths_and_digest() {
    let tmp = tempfile::tempdir().unwrap(); let root = tmp.path();
    fs::write(root.join("package.json"), "{\n  \"name\": \"demo\"\n}").unwrap();
    fs::write(root.join("index.js"), "console.log('hi')").unwrap();
    let mut c = bin();
    let assert = c.current_dir(root)
        .env("XDG_CACHE_HOME", root)
        .env("XDG_CONFIG_HOME", root)
        .args(["deploy","--pack-only","--format","json"]) 
        .assert();
    let output = assert.get_output().clone();
    assert.success();
    let stdout = String::from_utf8(output.stdout).unwrap();
    eprintln!("RAW_STDOUT=<<<{}>>>", stdout);
    let start = stdout.find('{').expect("json start");
    let end = stdout.rfind('}').expect("json end");
    let slice = &stdout[start..=end];
    let v: serde_json::Value = serde_json::from_str(slice).unwrap();
    assert!(v["artifact"].as_str().unwrap().ends_with(".tar.gz"));
    assert!(v["digest"].as_str().unwrap().len() == 64);
    assert!(v["sbom"].as_str().unwrap().ends_with(".sbom.json"));
    assert!(v["manifest"].as_str().unwrap().ends_with(".manifest.json"));
}