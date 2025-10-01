use anyhow::Result;
use serde::Serialize;
use std::fs;
use std::path::PathBuf;

#[derive(Serialize)]
struct ProvenanceDoc<'a> {
    schema: &'static str,
    app: &'a str,
    digest: &'a str,
    signature_present: bool,
    commit: Option<String>,
    timestamp: String,
}

pub fn write_provenance(app: &str, digest: &str, signature_present: bool) -> Result<()> {
    if digest.is_empty() { return Ok(()); }
    let dir = std::env::var("AETHER_PROVENANCE_DIR").unwrap_or_else(|_| "/tmp/provenance".into());
    fs::create_dir_all(&dir).ok();
    let commit = std::env::var("GIT_COMMIT_SHA").ok();
    let ts = chrono::Utc::now().to_rfc3339();
    let doc = ProvenanceDoc { schema: "aether.provenance.v1", app, digest, signature_present, commit, timestamp: ts };
    let path = PathBuf::from(dir).join(format!("{app}-{digest}.json"));
    fs::write(path, serde_json::to_vec_pretty(&doc)?)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::write_provenance;
    #[test]
    fn provenance_file_written() {
        let tmp = tempfile::tempdir().unwrap();
        std::env::set_var("AETHER_PROVENANCE_DIR", tmp.path());
        write_provenance("app","0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef", true).unwrap();
        let files: Vec<_> = std::fs::read_dir(tmp.path()).unwrap().collect();
        assert!(!files.is_empty());
    }
}