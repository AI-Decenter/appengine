use assert_cmd::Command;

fn bin() -> Command { Command::cargo_bin("aether-cli").unwrap() }

#[test]
fn network_error_exit_code() {
    let tmp = tempfile::tempdir().unwrap();
    std::env::set_var("XDG_CACHE_HOME", tmp.path());
    std::env::set_var("XDG_CONFIG_HOME", tmp.path());
    let assert = bin().arg("netfail").assert().failure();
    let code = assert.get_output().status.code().unwrap();
    assert_eq!(code, 40, "expected network error exit code 40, got {code}");
}

#[test]
fn usage_error_bad_subcommand() {
    let assert = Command::cargo_bin("aether-cli").unwrap().arg("--nonexistent").assert().failure();
    // clap uses code 2 for usage errors typically
    let code = assert.get_output().status.code().unwrap();
    assert_eq!(code, 2);
}

#[test]
fn io_error_simulated_via_command() {
    let assert = bin().arg("iofail").assert().failure();
    let code = assert.get_output().status.code().unwrap();
    assert_eq!(code, 30, "expected IO exit code 30, got {code}");
}

#[test]
fn config_error_invalid_toml() {
    let tmp = tempfile::tempdir().unwrap();
    let cfg_dir = tmp.path().join("aether");
    std::fs::create_dir_all(&cfg_dir).unwrap();
    std::fs::write(cfg_dir.join("config.toml"), "***").unwrap();
    std::env::set_var("XDG_CONFIG_HOME", tmp.path());
    std::env::set_var("XDG_CACHE_HOME", tmp.path());
    let assert = bin().arg("list").assert().failure();
    let code = assert.get_output().status.code().unwrap();
    assert_eq!(code, 10, "expected config code 10 got {code}");
}
