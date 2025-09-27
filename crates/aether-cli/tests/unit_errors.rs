use aether_cli::errors::CliErrorKind;
use aether_cli::errors::CliError;

#[test]
fn cli_error_kind_codes() {
    assert_eq!(CliErrorKind::Usage("u".into()).code(), 2);
    assert_eq!(CliErrorKind::Config("c".into()).code(), 10);
    assert_eq!(CliErrorKind::Runtime("r".into()).code(), 20);
    assert_eq!(CliErrorKind::Io("i".into()).code(), 30);
    assert_eq!(CliErrorKind::Network("n".into()).code(), 40);
}

#[test]
fn cli_error_display() {
    let e = CliError::new(CliErrorKind::Runtime("boom".into()));
    let s = format!("{e}");
    assert!(s.contains("boom"));
    assert!(s.contains("runtime error"));
}

#[test]
fn cli_error_from_io() {
    let ioe = std::io::Error::other("iofail");
    let e: CliError = ioe.into();
    assert_eq!(e.kind.code(), 30);
    let s = format!("{e}");
    assert!(s.contains("iofail"));
}
