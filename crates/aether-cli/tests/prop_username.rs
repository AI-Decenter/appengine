use proptest::prelude::*;
use assert_cmd::Command;

fn bin() -> Command { Command::cargo_bin("aether-cli").unwrap() }

proptest! {
    #[test]
    fn login_accepts_various_usernames(user in "[a-zA-Z0-9_]{1,16}") {
        let tmp = tempfile::tempdir().unwrap();
        std::env::set_var("XDG_CACHE_HOME", tmp.path());
        std::env::set_var("XDG_CONFIG_HOME", tmp.path());
        bin().args(["login","--username", &user]).assert().success();
    }
}
