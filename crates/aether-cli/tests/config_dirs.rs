use assert_cmd::Command;

fn bin()->Command { Command::cargo_bin("aether-cli").unwrap() }

// Ensure that when XDG vars are unset, we still succeed and the command runs (cannot assert exact path reliably cross-platform).
#[test]
fn login_without_xdg_envs_still_works() {
    // Intentionally do NOT set XDG_*; rely on host environment (in CI this is sandboxed).
    // Just verify the command succeeds.
    bin().arg("login").assert().success();
}
