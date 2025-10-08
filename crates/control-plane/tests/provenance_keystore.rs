use control_plane::{build_router, AppState};
use axum::{http::{Request, StatusCode}, body::Body};
use tower::util::ServiceExt;
use rand::RngCore;

fn gen_key_hex() -> String { let mut bytes=[0u8;32]; rand::thread_rng().fill_bytes(&mut bytes); hex::encode(bytes) }

fn read_attestation(app: &str, digest: &str) -> serde_json::Value {
    let dir = std::env::var("AETHER_PROVENANCE_DIR").unwrap_or_else(|_| "/tmp/provenance".into());
    let path = std::path::Path::new(&dir).join(format!("{app}-{digest}.prov2.dsse.json"));
    assert!(path.exists(), "attestation missing: {:?}", path);
    let data = std::fs::read_to_string(&path).unwrap();
    serde_json::from_str(&data).unwrap()
}

async fn seed_app_and_artifact(pool: &sqlx::Pool<sqlx::Postgres>, app: &str, digest: &str) {
    sqlx::query("DELETE FROM applications").execute(pool).await.ok();
    sqlx::query("DELETE FROM artifacts").execute(pool).await.ok();
    sqlx::query("INSERT INTO applications (name) VALUES ($1)").bind(app).execute(pool).await.unwrap();
    sqlx::query("INSERT INTO artifacts (app_id,digest,size_bytes,status) SELECT id,$1,0,'stored' FROM applications WHERE name=$2")
        .bind(digest).bind(app).execute(pool).await.unwrap();
}

#[tokio::test]
#[serial_test::serial]
async fn dual_sign_with_explicit_active_keystore() {
    let k1 = gen_key_hex(); let k2 = gen_key_hex();
    std::env::set_var("AETHER_ATTESTATION_SK", &k1);
    std::env::set_var("AETHER_ATTESTATION_KEY_ID", "k1-active");
    std::env::set_var("AETHER_ATTESTATION_SK_ROTATE2", &k2);
    std::env::set_var("AETHER_ATTESTATION_KEY_ID_ROTATE2", "k2-active");
    let tmp = tempfile::tempdir().unwrap();
    std::env::set_var("AETHER_PROVENANCE_DIR", tmp.path());
    // Explicit keystore marking both active
    std::fs::write(tmp.path().join("provenance_keys.json"), serde_json::to_vec_pretty(&serde_json::json!([
        {"key_id":"k1-active","status":"active"},
        {"key_id":"k2-active","status":"active"}
    ])).unwrap()).unwrap();
    let pool = control_plane::test_support::test_pool().await;
    let digest = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"; // 64 b
    seed_app_and_artifact(&pool, "appk", digest).await;
    let app_router = build_router(AppState { db: pool.clone() });
    let body = serde_json::json!({"app_name":"appk","artifact_url":format!("file://{digest}")}).to_string();
    let req = Request::builder().method("POST").uri("/deployments").header("content-type","application/json").body(Body::from(body)).unwrap();
    let res = app_router.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::CREATED);
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;
    let att = read_attestation("appk", digest);
    let sigs = att.get("signatures").and_then(|v| v.as_array()).unwrap();
    assert_eq!(sigs.len(), 2, "expected 2 signatures with both active");
}

#[tokio::test]
#[serial_test::serial]
async fn no_signatures_when_all_retired() {
    let k1 = gen_key_hex(); let k2 = gen_key_hex();
    std::env::set_var("AETHER_ATTESTATION_SK", &k1);
    std::env::set_var("AETHER_ATTESTATION_KEY_ID", "k1-old");
    std::env::set_var("AETHER_ATTESTATION_SK_ROTATE2", &k2);
    std::env::set_var("AETHER_ATTESTATION_KEY_ID_ROTATE2", "k2-old");
    let tmp = tempfile::tempdir().unwrap();
    std::env::set_var("AETHER_PROVENANCE_DIR", tmp.path());
    // Keystore marks both retired
    std::fs::write(tmp.path().join("provenance_keys.json"), serde_json::to_vec_pretty(&serde_json::json!([
        {"key_id":"k1-old","status":"retired"},
        {"key_id":"k2-old","status":"retired"}
    ])).unwrap()).unwrap();
    let pool = control_plane::test_support::test_pool().await;
    let digest = "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc"; // 64 c
    seed_app_and_artifact(&pool, "appn", digest).await;
    let app_router = build_router(AppState { db: pool.clone() });
    let body = serde_json::json!({"app_name":"appn","artifact_url":format!("file://{digest}")}).to_string();
    let req = Request::builder().method("POST").uri("/deployments").header("content-type","application/json").body(Body::from(body)).unwrap();
    let res = app_router.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::CREATED);
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;
    let att = read_attestation("appn", digest);
    let sigs_len = att.get("signatures").and_then(|v| v.as_array()).map(|a| a.len()).unwrap_or(0);
    assert_eq!(sigs_len, 0, "expected 0 signatures when all keys retired (field omitted or empty)");
}
