use assert_cmd::Command;
use std::fs;

fn bin() -> Command { Command::cargo_bin("aether-cli").unwrap() }

#[test]
fn help_works() { bin().arg("--help").assert().success(); }

#[test]
fn version_works() { bin().arg("--version").assert().success(); }

#[test]
fn login_creates_session() {
    let tmp = tempfile::tempdir().unwrap();
    let assert = bin()
        .env("XDG_CACHE_HOME", tmp.path())
        .env("XDG_CONFIG_HOME", tmp.path())
        .arg("login")
        .assert()
        .success();
    // Primary expected path
    let primary = tmp.path().join("aether/session.json");
    let mut target_path = None;
    if primary.exists() { target_path = Some(primary); }
    if target_path.is_none() {
        // Fallback: search recursively under temp root in case dirs crate resolved differently
        for entry in walkdir::WalkDir::new(tmp.path()).into_iter().filter_map(|e| e.ok()) {
            if entry.file_name() == "session.json" { target_path = Some(entry.path().to_path_buf()); break; }
        }
    }
    if target_path.is_none() {
        let stdout = String::from_utf8(assert.get_output().stdout.clone()).unwrap_or_default();
        let stderr = String::from_utf8(assert.get_output().stderr.clone()).unwrap_or_default();
        panic!("session.json not found; stdout=\n{stdout}\n---- stderr=\n{stderr}");
    }
    let data = fs::read_to_string(target_path.unwrap()).unwrap();
    assert!(data.contains("dev-mock-token"));
}

#[test]
fn deploy_dry_run() {
    let tmp = tempfile::tempdir().unwrap();
    // create a minimal NodeJS project marker inside temp dir
    fs::write(tmp.path().join("package.json"), "{}" ).unwrap();
    let mut cmd = bin();
    cmd.current_dir(tmp.path())
        .env("XDG_CACHE_HOME", tmp.path())
        .env("XDG_CONFIG_HOME", tmp.path())
        .args(["deploy","--dry-run"]) 
        .assert()
        .success();
}

#[test]
fn logs_mock() { bin().args(["logs"]).assert().success(); }

#[test]
fn list_mock() { bin().args(["list"]).assert().success(); }

#[test]
fn completions_bash() { bin().args(["completions","--shell","bash"]).assert().success(); }

#[test]
fn json_log_format() { bin().args(["--log-format","json","list"]).assert().success(); }
