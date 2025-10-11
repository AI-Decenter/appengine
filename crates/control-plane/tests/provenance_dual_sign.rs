use control_plane::{build_router, AppState};
use axum::{http::{Request, StatusCode}, body::Body};
use tower::util::ServiceExt;
use rand::RngCore;

fn gen_key_hex() -> String {
    let mut bytes = [0u8;32]; rand::thread_rng().fill_bytes(&mut bytes); hex::encode(bytes)
}

// Helper to read DSSE attestation file
fn read_attestation(app: &str, digest: &str) -> serde_json::Value {
    let dir = std::env::var("AETHER_PROVENANCE_DIR").unwrap_or_else(|_| "/tmp/provenance".into());
    // pattern {app}-{digest}.prov2.dsse.json
    let path = std::path::Path::new(&dir).join(format!("{app}-{digest}.prov2.dsse.json"));
    assert!(path.exists(), "expected attestation file {:?}", path);
    let data = std::fs::read_to_string(&path).unwrap();
    serde_json::from_str(&data).unwrap()
}

#[tokio::test]
#[serial_test::serial]
async fn provenance_dual_signature_then_retire() {
    // Setup keys
    let k1 = gen_key_hex();
    let k2 = gen_key_hex();
    std::env::set_var("AETHER_ATTESTATION_SK", &k1);
    std::env::set_var("AETHER_ATTESTATION_KEY_ID", "k1");
    std::env::set_var("AETHER_ATTESTATION_SK_ROTATE2", &k2);
    std::env::set_var("AETHER_ATTESTATION_KEY_ID_ROTATE2", "k2");
    let tmp = tempfile::tempdir().unwrap();
    std::env::set_var("AETHER_PROVENANCE_DIR", tmp.path());

    // Prepare digest artifact record (simulate) so deployment resolves digest
    let pool = control_plane::test_support::test_pool().await;
    sqlx::query("DELETE FROM applications").execute(&pool).await.ok();
    sqlx::query("DELETE FROM artifacts").execute(&pool).await.ok();
    sqlx::query("INSERT INTO applications (name) VALUES ($1)").bind("dual").execute(&pool).await.unwrap();
    let digest = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"; // 64 a's
    sqlx::query("INSERT INTO artifacts (app_id,digest,size_bytes,status) SELECT id,$1,0,'stored' FROM applications WHERE name='dual'")
        .bind(digest).execute(&pool).await.unwrap();

    let app = build_router(AppState { db: pool.clone() });
    let body = serde_json::json!({"app_name":"dual","artifact_url":format!("file://{digest}" )}).to_string();
    let req = Request::builder().method("POST").uri("/deployments").header("content-type","application/json").body(Body::from(body)).unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::CREATED);
    // Allow async provenance write
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;
    let att = read_attestation("dual", digest);
    let sigs = att.get("signatures").and_then(|v| v.as_array()).unwrap();
    assert_eq!(sigs.len(), 2, "expected dual signatures before retirement");

    // Write keystore file marking k1 retired
    let keystore_path = tmp.path().join("provenance_keys.json");
    std::fs::write(&keystore_path, serde_json::to_vec_pretty(&serde_json::json!([
        {"key_id":"k1","status":"retired"},
        {"key_id":"k2","status":"active"}
    ])).unwrap()).unwrap();

    // Trigger second deployment to produce new provenance
    let body2 = serde_json::json!({"app_name":"dual","artifact_url":format!("file://{digest}" )}).to_string();
    let req2 = Request::builder().method("POST").uri("/deployments").header("content-type","application/json").body(Body::from(body2)).unwrap();
    let res2 = app.oneshot(req2).await.unwrap();
    assert_eq!(res2.status(), StatusCode::CREATED);
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;
    let att2 = read_attestation("dual", digest); // overwritten same digest
    let sigs2 = att2.get("signatures").and_then(|v| v.as_array()).unwrap();
    assert_eq!(sigs2.len(), 1, "expected only active key signature after retirement");
    assert_eq!(sigs2[0].get("keyid").unwrap().as_str().unwrap(), "k2");
}
