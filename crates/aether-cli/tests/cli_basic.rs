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
    std::env::set_var("XDG_CACHE_HOME", tmp.path());
    std::env::set_var("XDG_CONFIG_HOME", tmp.path());
    bin().arg("login").assert().success();
    // session file existence (best effort path)
    let session_glob = tmp.path().join("aether/session.json");
    assert!(session_glob.exists());
    let data = fs::read_to_string(session_glob).unwrap();
    assert!(data.contains("dev-mock-token"));
}

#[test]
fn deploy_dry_run() {
    let tmp = tempfile::tempdir().unwrap();
    std::env::set_var("XDG_CACHE_HOME", tmp.path());
    std::env::set_var("XDG_CONFIG_HOME", tmp.path());
    // create a minimal NodeJS project marker
    fs::write("package.json", "{}" ).unwrap();
    bin().args(["deploy","--dry-run"]).assert().success();
}

#[test]
fn logs_mock() { bin().args(["logs"]).assert().success(); }

#[test]
fn list_mock() { bin().args(["list"]).assert().success(); }

#[test]
fn completions_bash() { bin().args(["completions","--shell","bash"]).assert().success(); }

#[test]
fn json_log_format() { bin().args(["--log-format","json","list"]).assert().success(); }
