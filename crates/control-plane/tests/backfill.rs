use control_plane::backfill::backfill_legacy;

#[tokio::test]
#[serial_test::serial]
async fn backfill_generates_minimal_sbom_and_provenance() {
    let state = control_plane::test_support::test_state().await;
    // Insert legacy artifact (stored, no sbom_url)
    let digest = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    sqlx::query("DELETE FROM artifacts").execute(&state.db).await.ok();
    sqlx::query("DELETE FROM applications").execute(&state.db).await.ok();
    sqlx::query("INSERT INTO applications (name) VALUES ($1)").bind("legacy").execute(&state.db).await.unwrap();
    let app_id: uuid::Uuid = sqlx::query_scalar("SELECT id FROM applications WHERE name='legacy'").fetch_one(&state.db).await.unwrap();
    sqlx::query("INSERT INTO artifacts (app_id,digest,size_bytes,signature,sbom_url,manifest_url,verified,storage_key,status,created_at) VALUES ($1,$2,0,NULL,NULL,NULL,FALSE,$3,'stored',NOW())")
        .bind(app_id).bind(digest).bind(format!("artifacts/{digest}.tar.gz")).execute(&state.db).await.unwrap();
    let count = backfill_legacy(&state.db).await.unwrap();
    assert_eq!(count, 1, "expected one artifact backfilled");
    let url: Option<String> = sqlx::query_scalar("SELECT sbom_url FROM artifacts WHERE digest=$1").bind(digest).fetch_one(&state.db).await.unwrap();
    assert!(url.is_some(), "sbom_url not set after backfill");
    // Provenance file should exist (prov2)
    let prov_dir = std::env::var("AETHER_PROVENANCE_DIR").unwrap_or_else(|_| "/tmp/provenance".into());
    let prov_path = std::path::Path::new(&prov_dir).join(format!("backfill-{digest}.prov2.json"));
    assert!(prov_path.exists(), "provenance v2 file missing");
}
