use thiserror::Error;
use std::fmt;

#[derive(Error, Debug)]
pub enum CliErrorKind {
    #[error("usage error: {0}")] Usage(String),
    #[error("config error: {0}")] Config(String),
    #[error("runtime error: {0}")] Runtime(String),
    #[error("io error: {0}")] Io(String),
    #[error("network error: {0}")] Network(String),
}

#[derive(Debug)]
pub struct CliError { pub kind: CliErrorKind, pub source: Option<anyhow::Error> }
impl fmt::Display for CliError { fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result { self.kind.fmt(f) } }
impl std::error::Error for CliError { fn source(&self) -> Option<&(dyn std::error::Error + 'static)> { self.source.as_ref().map(|e| e.as_ref() as _) } }

impl CliErrorKind { pub fn code(&self) -> i32 { match self { Self::Usage(_) => 2, Self::Config(_) => 10, Self::Runtime(_) => 20, Self::Io(_) => 30, Self::Network(_) => 40 } } }

impl CliError {
    pub fn new(kind: CliErrorKind) -> Self { Self { kind, source: None } }
    pub fn with_source<E: Into<anyhow::Error>>(kind: CliErrorKind, err: E) -> Self { Self { kind, source: Some(err.into()) } }
}

impl From<std::io::Error> for CliError { fn from(e: std::io::Error) -> Self { Self::with_source(CliErrorKind::Io(e.to_string()), e) } }
