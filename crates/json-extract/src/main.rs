use anyhow::{bail, Result};
use clap::Parser;
use std::io::Read;

/// Minimal JSON annotation field extractor: reads full stdin, outputs value for given key if found.
#[derive(Parser, Debug)]
struct Args {
    /// Annotation key to extract (e.g. aether.dev/digest)
    key: String,
}

fn main() -> Result<()> {
    if let Err(e) = real_main() { eprintln!("{e}"); std::process::exit(1); } Ok(())
}

fn real_main() -> Result<()> {
    let args = Args::parse();
    let mut raw = Vec::new();
    std::io::stdin().read_to_end(&mut raw)?;
    let buf = String::from_utf8_lossy(&raw);
    // Fast path: find annotations object substring then scan for key
    if let Some(idx) = buf.find("\"annotations\"") {
        if let Some(rest) = buf[idx..].find('{').map(|o| &buf[idx+o+1..]) {
            // naive scan for "key":"value"
            let pattern = format!("\"{}\"", args.key);
            if let Some(kpos) = rest.find(&pattern) {
                let after = &rest[kpos+pattern.len()..];
                if let Some(colon) = after.find(':') {
                    let after_colon = &after[colon+1..];
                    if let Some(first_quote) = after_colon.find('"') {
                        let s = &after_colon[first_quote+1..];
                        if let Some(end) = s.find('"') { println!("{}", &s[..end]); return Ok(()); }
                    }
                }
            }
        }
    }
    bail!("key not found")
}
