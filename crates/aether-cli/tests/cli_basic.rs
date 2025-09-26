use assert_cmd::Command;

#[test]
fn prints_help() {
    let mut cmd = Command::cargo_bin("aether-cli").unwrap();
    cmd.arg("--help").assert().success();
}
