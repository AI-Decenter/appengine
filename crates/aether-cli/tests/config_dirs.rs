use assert_cmd::Command;

fn bin()->Command { Command::cargo_bin("aether-cli").unwrap() }

// Ensure that when XDG vars are unset, we still succeed and the command runs (cannot assert exact path reliably cross-platform).
#[test]
fn login_without_xdg_envs_still_works() {
    let td = tempfile::tempdir().unwrap();
    // Simulate a clean HOME so that dirs::config_dir resolves inside temp space.
    let mut cmd = bin();
    cmd.env_remove("XDG_CONFIG_HOME")
        .env_remove("XDG_CACHE_HOME")
        .env("HOME", td.path())
        .arg("login")
        .assert()
        .success();
}
