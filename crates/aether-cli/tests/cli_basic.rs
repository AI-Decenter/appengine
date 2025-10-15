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
    let v: serde_json::Value = serde_json::from_str(&data).expect("valid JSON session");
    assert_eq!(v["token"], "dev-mock-token");
    assert!(v.get("user").is_some(), "session should contain user field");
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
fn logs_mock() {
    let tmp = tempfile::tempdir().unwrap();
    bin()
        .env("XDG_CONFIG_HOME", tmp.path())
        .env("XDG_CACHE_HOME", tmp.path())
        .env("AETHER_API_BASE", "http://127.0.0.1:0")
        .env("AETHER_LOGS_FOLLOW", "0")
        .env("AETHER_LOGS_FORMAT", "text")
        .args(["logs"]).assert().success();
}

#[test]
fn list_mock() {
    let tmp = tempfile::tempdir().unwrap();
    bin().env("XDG_CONFIG_HOME", tmp.path()).env("XDG_CACHE_HOME", tmp.path()).args(["list"]).assert().success();
}

#[test]
fn completions_bash() {
    let tmp = tempfile::tempdir().unwrap();
    bin().env("XDG_CONFIG_HOME", tmp.path()).env("XDG_CACHE_HOME", tmp.path()).args(["completions","--shell","bash"]).assert().success();
}

#[test]
fn json_log_format() {
    let tmp = tempfile::tempdir().unwrap();
    bin().env("XDG_CONFIG_HOME", tmp.path()).env("XDG_CACHE_HOME", tmp.path()).args(["--log-format","json","list"]).assert().success();
}

#[test]
fn login_session_permissions_restrictive() {
    #[cfg(unix)] {
        use std::os::unix::fs::PermissionsExt;
        let tmp = tempfile::tempdir().unwrap();
        bin()
            .env("XDG_CACHE_HOME", tmp.path())
            .env("XDG_CONFIG_HOME", tmp.path())
            .arg("login")
            .assert()
            .success();
        let session_path = tmp.path().join("aether/session.json");
        let meta = std::fs::metadata(&session_path).unwrap();
        let mode = meta.permissions().mode() & 0o777;
        assert_eq!(mode, 0o600, "expected session file mode 600, got {:o}", mode);
    }
}
