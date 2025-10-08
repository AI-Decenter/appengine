use anyhow::Result;
use serde::{Serialize, Deserialize};
use std::fs;
use std::path::PathBuf;
use sha2::{Digest, Sha256};
use base64::Engine;
use ed25519_dalek::{SigningKey,Signer};
use crate::telemetry::{ATTESTATION_SIGNED_TOTAL, PROVENANCE_EMITTED_TOTAL};
use std::time::SystemTime;
use std::collections::HashSet;

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
struct MaterialRef { r#type: &'static str, name: String, digest: String }

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
    materials: Vec<MaterialRef>,
    #[serde(skip_serializing_if="Option::is_none")] builder: Option<Builder>,
    #[serde(skip_serializing_if="Option::is_none")] buildType: Option<String>,
    #[serde(skip_serializing_if="Option::is_none")] invocation: Option<Invocation>,
    #[serde(skip_serializing_if="Option::is_none")] completeness: Option<Completeness>,
    #[serde(skip_serializing_if="Option::is_none")] metadata: Option<BuildMetadata>,
}

#[derive(Serialize, Deserialize, Clone)]
struct Builder { id: String }
#[derive(Serialize, Deserialize, Clone)]
struct InvocationEnv { os: String, rustc: String, ci: bool }
#[derive(Serialize, Deserialize, Clone)]
struct Invocation { environment: InvocationEnv, #[serde(default)] parameters: serde_json::Value }
#[derive(Serialize, Deserialize, Clone)]
struct Completeness { parameters: bool, environment: bool, materials: bool }
#[derive(Serialize, Deserialize, Clone)]
struct BuildMetadata { buildStartedOn: String, buildFinishedOn: String, reproducible: bool }

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
    // Build materials enrichment
    let mut materials: Vec<MaterialRef> = Vec::new();
    let mut seen: HashSet<(String,String)> = HashSet::new();
    if let Some(ref h) = sbom_hash { materials.push(MaterialRef { r#type: "sbom", name: "cyclonedx@1.5".into(), digest: h.clone() }); seen.insert(("sbom".into(), "cyclonedx@1.5".into())); }
    // manifest file (if present)
    if let Ok(manifest_dir) = std::env::var("AETHER_MANIFEST_DIR") {
        let manifest_path = PathBuf::from(&manifest_dir).join(format!("{digest}.manifest.json"));
    if manifest_path.exists() { if let Some(h) = compute_sha256_file(&manifest_path) { materials.push(MaterialRef { r#type: "manifest", name: "app-manifest".into(), digest: h }); } }
    }
    // lockfile (package-lock.json) colocated with sbom dir or current
    if let Ok(root_dir) = std::env::var("AETHER_BUILD_ROOT") { // optional build root path passed by deploy pipeline
        let lock = PathBuf::from(&root_dir).join("package-lock.json");
    if lock.exists() { if let Some(h) = compute_sha256_file(&lock) { materials.push(MaterialRef { r#type: "lockfile", name: "package-lock.json".into(), digest: h }); } }
    }
    // derive builder/invocation metadata
    let builder_id = std::env::var("AETHER_BUILDER_ID").unwrap_or_else(|_| "aether://builder/default".into());
    let build_type = std::env::var("AETHER_BUILD_TYPE").unwrap_or_else(|_| "aether.app.bundle.v1".into());
    let started = SystemTime::now();
    // Basic env capture (stable small set)
    let os = std::env::consts::OS.to_string();
    let rustc = option_env!("RUSTC_VERSION").unwrap_or("unknown").to_string();
    let ci = std::env::var("CI").ok().is_some();
    let invocation = Invocation { environment: InvocationEnv { os, rustc, ci }, parameters: serde_json::json!({}) };
    let completeness = Completeness { parameters: true, environment: true, materials: true };
    let finished = SystemTime::now();
    let started_rfc3339 = chrono::DateTime::<chrono::Utc>::from(started).to_rfc3339();
    let finished_rfc3339 = chrono::DateTime::<chrono::Utc>::from(finished).to_rfc3339();
    let metadata = BuildMetadata { buildStartedOn: started_rfc3339, buildFinishedOn: finished_rfc3339, reproducible: false };
    let v2_raw = ProvenanceV2 { schema: "aether.provenance.v2", app, artifact_digest: digest, signature_present, commit: commit.clone(), timestamp: ts.clone(), sbom_sha256: sbom_hash.clone(), sbom_url: if sbom_path.exists() { Some(format!("/artifacts/{digest}/sbom")) } else { None }, materials, builder: Some(Builder { id: builder_id }), buildType: Some(build_type), invocation: Some(invocation), completeness: Some(completeness), metadata: Some(metadata) };
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
    // Support multiple rotation keys AETHER_ATTESTATION_SK, AETHER_ATTESTATION_SK_ROTATE2
    let mut key_specs: Vec<(String,String)> = Vec::new();
    if let Ok(main) = std::env::var("AETHER_ATTESTATION_SK") { key_specs.push((main, std::env::var("AETHER_ATTESTATION_KEY_ID").unwrap_or_else(|_| "attestation-default".into()))); }
    if let Ok(rot) = std::env::var("AETHER_ATTESTATION_SK_ROTATE2") { key_specs.push((rot, std::env::var("AETHER_ATTESTATION_KEY_ID_ROTATE2").unwrap_or_else(|_| "attestation-rotated".into()))); }
    for (hex_key, keyid) in key_specs.into_iter() {
        if let Ok(bytes) = hex::decode(hex_key.trim()) { if bytes.len()==32 { let sk = SigningKey::from_bytes(&bytes.clone().try_into().unwrap()); let sig = sk.sign(&payload_bytes); let sig_hex = hex::encode(sig.to_bytes()); signatures.push(DsseSignature { keyid: keyid.clone(), sig: sig_hex }); ATTESTATION_SIGNED_TOTAL.with_label_values(&[app]).inc(); } }
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