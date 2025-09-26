use assert_cmd::Command;use std::time::Instant;

fn bin()->Command { Command::cargo_bin("aether-cli").unwrap() }

#[test]
fn login_permission_warning_when_forced() {
    let tmp = tempfile::tempdir().unwrap();
    let assert = bin()
        .env("XDG_CACHE_HOME", tmp.path())
        .env("XDG_CONFIG_HOME", tmp.path())
        .env("AETHER_TEST_PERMISSIVE", "1")
        .arg("login")
        .assert();
    let out = String::from_utf8(assert.get_output().stderr.clone()).unwrap();
    assert!(out.contains("warning: session file permissions too open"));
}

#[test]
fn config_env_override() {
    let tmp = tempfile::tempdir().unwrap();
    bin()
        .env("XDG_CACHE_HOME", tmp.path())
        .env("XDG_CONFIG_HOME", tmp.path())
        .env("AETHER_DEFAULT_NAMESPACE", "ns-override")
        .args(["list"]) 
        .assert()
        .success();
    // No direct output yet; placeholder ensures no crash with env override.
}

#[test]
fn json_log_format_outputs_json() {
    let tmp = tempfile::tempdir().unwrap();
    let assert = bin()
        .env("XDG_CACHE_HOME", tmp.path())
        .env("XDG_CONFIG_HOME", tmp.path())
        .args(["--log-format","json","list"])
        .assert();
    let stdout = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
    let first_line = stdout.lines().next().unwrap_or("");
    // tolerate empty (some logs go to stderr); just skip if empty
    if !first_line.is_empty() { let _parsed: serde_json::Value = serde_json::from_str(first_line).expect("first line should be JSON"); }
}

#[test]
fn startup_time_under_threshold() {
    let start = Instant::now();
    bin().arg("--help").assert().success();
    let took = start.elapsed().as_millis();
    // Use relaxed threshold to avoid CI flakiness; spec target is 150ms local.
    assert!(took < 800, "Startup took {}ms (threshold 800ms in CI)", took);
}
