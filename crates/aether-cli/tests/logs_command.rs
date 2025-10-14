use assert_cmd::Command;
use predicates::str::contains;
use std::fs;

fn bin() -> Command { Command::cargo_bin("aether-cli").unwrap() }

#[test]
fn logs_help_and_flags() {
    bin().arg("logs").arg("--help").assert().success().stdout(contains("--app")).stdout(contains("--follow")).stdout(contains("--since")).stdout(contains("--container")).stdout(contains("--format"));
}

#[test]
fn logs_mock_text() {
    let tmp = tempfile::tempdir().unwrap();
    bin()
        .env("XDG_CONFIG_HOME", tmp.path())
        .env("XDG_CACHE_HOME", tmp.path())
        .env("AETHER_API_BASE", "http://127.0.0.1:0")
        .env("AETHER_LOGS_FOLLOW", "0")
        .env("AETHER_LOGS_FORMAT", "text")
        .args(["logs", "--app", "demo", "--format", "text"])
        .assert()
        .success()
        .stdout(contains("mock line 1"));
}

#[test]
fn logs_mock_json() {
    let tmp = tempfile::tempdir().unwrap();
    bin()
        .env("XDG_CONFIG_HOME", tmp.path())
        .env("XDG_CACHE_HOME", tmp.path())
        .env("AETHER_API_BASE", "http://127.0.0.1:0")
        .env("AETHER_LOGS_FOLLOW", "0")
        .env("AETHER_LOGS_FORMAT", "json")
        .args(["logs", "--app", "demo", "--format", "json"])
        .assert()
        .success()
        .stdout(contains("\"message\":\"mock line 1\""));
}

#[test]
fn logs_follow_reconnect() {
    let tmp = tempfile::tempdir().unwrap();
    // Simulate reconnect by setting max reconnects to 2
    bin()
        .env("XDG_CONFIG_HOME", tmp.path())
        .env("XDG_CACHE_HOME", tmp.path())
        .env("AETHER_API_BASE", "http://127.0.0.1:0")
        .env("AETHER_LOGS_FOLLOW", "1")
        .env("AETHER_LOGS_MAX_RECONNECTS", "2")
        .args(["logs", "--app", "demo", "--follow"])
        .assert()
        .success();
}

#[test]
fn logs_container_and_since_flags() {
    let tmp = tempfile::tempdir().unwrap();
    bin()
        .env("XDG_CONFIG_HOME", tmp.path())
        .env("XDG_CACHE_HOME", tmp.path())
        .env("AETHER_API_BASE", "http://127.0.0.1:0")
        .env("AETHER_LOGS_FOLLOW", "0")
        .args(["logs", "--app", "demo", "--container", "worker", "--since", "5m"])
        .assert()
        .success();
}
