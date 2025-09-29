use anyhow::Result;use clap_complete::{generate, shells::{Bash,Zsh,Fish}};use clap::CommandFactory;use std::io;use super::Cli;use tracing::info;

pub fn handle(shell: String) -> Result<()> { let mut cmd = Cli::command(); match shell.as_str() { "bash"=>{generate(Bash,&mut cmd,"aether",&mut io::stdout());}, "zsh"=>{generate(Zsh,&mut cmd,"aether",&mut io::stdout());}, "fish"=>{generate(Fish,&mut cmd,"aether",&mut io::stdout());}, _=>{eprintln!("Unsupported shell: {shell}");} }; info!(event="completions.generated", shell=%shell); Ok(()) }
