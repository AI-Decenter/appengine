use anyhow::Result;
use serde::Serialize;
use std::fs;
use std::path::PathBuf;
use sha2::{Digest, Sha256};
use base64::Engine;
use ed25519_dalek::{SigningKey,Signer};
use crate::telemetry::{ATTESTATION_SIGNED_TOTAL, PROVENANCE_EMITTED_TOTAL};

#[derive(Serialize)]
struct ProvenanceV1<'a> {
    schema: &'static str,
    app: &'a str,
    digest: &'a str,
    signature_present: bool,
    commit: Option<String>,
    timestamp: String,
}

#[derive(Serialize)]
struct MaterialRef<'a> { r#type: &'static str, name: &'a str, digest: &'a str }

#[derive(Serialize)]
struct ProvenanceV2<'a> {
    schema: &'static str,
    app: &'a str,
    artifact_digest: &'a str,
    signature_present: bool,
    commit: Option<String>,
    timestamp: String,
    sbom_sha256: Option<String>,
    sbom_url: Option<String>,
    materials: Vec<MaterialRef<'a>>,
}

#[derive(Serialize)]
struct DsseSignature { keyid: String, sig: String }
#[allow(non_snake_case)]
#[derive(Serialize)]
struct DsseEnvelope { payloadType: &'static str, payload: String, #[serde(skip_serializing_if="Vec::is_empty")] signatures: Vec<DsseSignature> }

fn compute_sha256_file(path: &PathBuf) -> Option<String> {
    let bytes = fs::read(path).ok()?; let mut hasher = Sha256::new(); hasher.update(&bytes); Some(format!("{:x}", hasher.finalize()))
}

fn canonical_json(value: &serde_json::Value) -> serde_json::Value {
    match value {
        serde_json::Value::Object(map) => {
            let mut keys: Vec<_> = map.keys().collect(); keys.sort();
            let mut new = serde_json::Map::new();
            for k in keys { new.insert(k.clone(), canonical_json(&map[k])); }
            serde_json::Value::Object(new)
        },
        serde_json::Value::Array(arr) => serde_json::Value::Array(arr.iter().map(canonical_json).collect()),
        _ => value.clone()
    }
}

pub fn write_provenance(app: &str, digest: &str, signature_present: bool) -> Result<()> {
    if digest.is_empty() { return Ok(()); }
    let dir = std::env::var("AETHER_PROVENANCE_DIR").unwrap_or_else(|_| "/tmp/provenance".into());
    fs::create_dir_all(&dir).ok();
    let commit = std::env::var("GIT_COMMIT_SHA").ok();
    let ts = chrono::Utc::now().to_rfc3339();
    // v1 for backward compatibility
    let v1 = ProvenanceV1 { schema: "aether.provenance.v1", app, digest, signature_present, commit: commit.clone(), timestamp: ts.clone() };
    let path_v1 = PathBuf::from(&dir).join(format!("{app}-{digest}.json"));
    fs::write(&path_v1, serde_json::to_vec_pretty(&v1)?)?;
    // Attempt to locate SBOM to enrich v2
    let sbom_dir = std::env::var("AETHER_SBOM_DIR").unwrap_or_else(|_| "./".into());
    let sbom_path = PathBuf::from(&sbom_dir).join(format!("{digest}.sbom.json"));
    let sbom_hash = if sbom_path.exists() { compute_sha256_file(&sbom_path) } else { None };
    // Build materials (placeholder: reference SBOM if exists)
    let mut materials: Vec<MaterialRef> = Vec::new();
    if let Some(ref h) = sbom_hash { materials.push(MaterialRef { r#type: "sbom", name: "cyclonedx", digest: h }); }
    let v2_raw = ProvenanceV2 { schema: "aether.provenance.v2", app, artifact_digest: digest, signature_present, commit: commit.clone(), timestamp: ts.clone(), sbom_sha256: sbom_hash.clone(), sbom_url: if sbom_path.exists() { Some(format!("/artifacts/{digest}/sbom")) } else { None }, materials };
    // Canonicalize JSON (sorted keys) before signing
    let v2_value = serde_json::to_value(&v2_raw)?;
    let v2_canon = canonical_json(&v2_value);
    let path_v2 = PathBuf::from(&dir).join(format!("{app}-{digest}.prov2.json"));
    fs::write(&path_v2, serde_json::to_vec_pretty(&v2_canon)?)?;
    PROVENANCE_EMITTED_TOTAL.with_label_values(&[app]).inc();
    // DSSE signing with dedicated attestation key (AETHER_ATTESTATION_SK hex 32 bytes)
    let payload_bytes = serde_json::to_vec(&v2_canon)?;
    let payload_b64 = base64::engine::general_purpose::STANDARD.encode(&payload_bytes);
    let mut signatures: Vec<DsseSignature> = Vec::new();
    if let Ok(sk_hex) = std::env::var("AETHER_ATTESTATION_SK") {
        if let Ok(bytes) = hex::decode(sk_hex.trim()) {
            if bytes.len()==32 {
                let sk = SigningKey::from_bytes(&bytes.clone().try_into().unwrap());
                let sig = sk.sign(&payload_bytes);
                let sig_hex = hex::encode(sig.to_bytes());
                let keyid = std::env::var("AETHER_ATTESTATION_KEY_ID").unwrap_or_else(|_| "attestation-default".into());
                signatures.push(DsseSignature { keyid: keyid.clone(), sig: sig_hex });
                ATTESTATION_SIGNED_TOTAL.with_label_values(&[app]).inc();
            }
        }
    }
    let env = DsseEnvelope { payloadType: "application/vnd.aether.provenance+json", payload: payload_b64, signatures };
    let env_path = PathBuf::from(&dir).join(format!("{app}-{digest}.prov2.dsse.json"));
    fs::write(&env_path, serde_json::to_vec_pretty(&env)?)?;
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