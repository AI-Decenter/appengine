use aether_cli::errors::CliErrorKind;

#[test]
fn cli_error_kind_codes() {
    assert_eq!(CliErrorKind::Usage("u".into()).code(), 2);
    assert_eq!(CliErrorKind::Config("c".into()).code(), 10);
    assert_eq!(CliErrorKind::Runtime("r".into()).code(), 20);
    assert_eq!(CliErrorKind::Io("i".into()).code(), 30);
    assert_eq!(CliErrorKind::Network("n".into()).code(), 40);
}
