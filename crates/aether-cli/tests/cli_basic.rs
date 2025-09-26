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
    bin()
        .env("XDG_CACHE_HOME", tmp.path())
        .env("XDG_CONFIG_HOME", tmp.path())
        .arg("login")
        .assert()
        .success();
    let session_path = tmp.path().join("aether/session.json");
    assert!(session_path.exists(), "expected session file at {:?}", session_path);
    let data = fs::read_to_string(session_path).unwrap();
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
