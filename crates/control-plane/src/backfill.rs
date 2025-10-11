use anyhow::Result;
use sha2::{Sha256, Digest};
use crate::provenance::write_provenance;

/// Backfill SBOM & provenance for legacy artifacts missing them.
/// For SBOM we generate a minimal placeholder CycloneDX with only top-level component referencing digest.
pub async fn backfill_legacy(pool: &sqlx::Pool<sqlx::Postgres>) -> Result<u64> {
    let rows: Vec<(String, Option<String>)> = sqlx::query_as("SELECT digest, sbom_url FROM artifacts WHERE sbom_url IS NULL AND status='stored' LIMIT 100")
        .fetch_all(pool).await?;
    if rows.is_empty() { return Ok(0); }
    let sbom_dir = std::env::var("AETHER_SBOM_DIR").unwrap_or_else(|_| "./".into());
    tokio::fs::create_dir_all(&sbom_dir).await.ok();
    let mut count = 0u64;
    for (digest, _url) in rows {
        // Generate minimal SBOM
        let doc = serde_json::json!({
            "bomFormat":"CycloneDX","specVersion":"1.5","components":[{"type":"container","name":digest}],"metadata": {"backfill": true}
        });
        let bytes = serde_json::to_vec_pretty(&doc)?;
        // size guard reuse logic
        if bytes.len() > 2*1024*1024 { continue; }
        let path = std::path::Path::new(&sbom_dir).join(format!("{digest}.sbom.json"));
        if tokio::fs::write(&path, &bytes).await.is_ok() {
            let url = format!("/artifacts/{digest}/sbom");
            let _ = sqlx::query("UPDATE artifacts SET sbom_url=$1, sbom_validated=TRUE WHERE digest=$2")
                .bind(&url).bind(&digest).execute(pool).await;
            // compute hash and provenance
            let mut h = Sha256::new(); h.update(&bytes); let _hash = format!("{:x}", h.finalize());
            let _ = write_provenance("backfill", &digest, false);
            count += 1;
        }
    }
    Ok(count)
}
