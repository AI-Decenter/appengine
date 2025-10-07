use axum::{http::{Request, StatusCode}, body::Body};
use tower::util::ServiceExt;
use control_plane::{build_router, AppState};
use sha2::{Sha256, Digest};

fn manifest_digest(files: &[(&str,&str)]) -> String {
    let mut v: Vec<(&str,&str)> = files.to_vec();
    v.sort_by(|a,b| a.0.cmp(b.0));
    let mut h = Sha256::new();
    for (p,d) in v { h.update(p.as_bytes()); h.update(d.as_bytes()); }
    format!("{:x}", h.finalize())
}

async fn prepare_artifact(app: &str, digest: &str, app_state: &AppState) -> axum::Router {
    sqlx::query("DELETE FROM artifacts").execute(&app_state.db).await.ok();
    sqlx::query("DELETE FROM applications").execute(&app_state.db).await.ok();
    sqlx::query("INSERT INTO applications (name) VALUES ($1)").bind(app).execute(&app_state.db).await.unwrap();
    let router = build_router(app_state.clone());
    // presign + complete
    let presign_body = serde_json::json!({"app_name":app, "digest":digest}).to_string();
    let presign_req = Request::builder().method("POST").uri("/artifacts/presign").header("content-type","application/json").body(Body::from(presign_body)).unwrap();
    let presign_resp = router.clone().oneshot(presign_req).await.unwrap();
    assert_eq!(presign_resp.status(), StatusCode::OK);
    let complete_body = serde_json::json!({"app_name":app,"digest":digest,"size_bytes":0,"signature":null}).to_string();
    let comp_req = Request::builder().method("POST").uri("/artifacts/complete").header("content-type","application/json").body(Body::from(complete_body)).unwrap();
    let comp_resp = router.clone().oneshot(comp_req).await.unwrap();
    assert_eq!(comp_resp.status(), StatusCode::OK);
    router
}

#[tokio::test]
#[serial_test::serial]
async fn manifest_then_valid_sbom_and_deployment() {
    std::env::set_var("AETHER_ENFORCE_SBOM", "1");
    let state = control_plane::test_support::test_state().await;
    let digest = "1111111111111111111111111111111111111111111111111111111111111111"; // 64 hex
    let app = "enforceapp";
    let router = prepare_artifact(app, digest, &state).await;
    // Upload manifest
    let files = [("/bin/app","deadbeef"),("/lib/a.so","beadfeed")];
    let m_digest = manifest_digest(&files);
    let manifest_body = serde_json::json!({"files": files.iter().map(|(p,d)| serde_json::json!({"path":p, "sha256":d})).collect::<Vec<_>>()}).to_string();
    let m_req = Request::builder().method("POST").uri(format!("/artifacts/{digest}/manifest")).header("content-type","application/json").body(Body::from(manifest_body)).unwrap();
    let m_resp = router.clone().oneshot(m_req).await.unwrap();
    assert_eq!(m_resp.status(), StatusCode::CREATED);
    let m_bytes = axum::body::to_bytes(m_resp.into_body(), 1024).await.unwrap();
    let m_json: serde_json::Value = serde_json::from_slice(&m_bytes).unwrap();
    assert_eq!(m_json["manifest_digest"].as_str().unwrap(), m_digest);
    // Upload SBOM (CycloneDX) with matching x-manifest-digest
    let sbom_doc = serde_json::json!({
        "bomFormat":"CycloneDX","specVersion":"1.5","components":[{"type":"container","name":"artifact","version":"1.0.0"}],
        "x-manifest-digest": m_digest
    });
    let sbom_req = Request::builder().method("POST").uri(format!("/artifacts/{digest}/sbom")).header("content-type","application/json").body(Body::from(sbom_doc.to_string())).unwrap();
    let sbom_resp = router.clone().oneshot(sbom_req).await.unwrap();
    assert_eq!(sbom_resp.status(), StatusCode::CREATED, "valid SBOM upload should succeed");
    // Attempt deployment (should succeed now)
    let dep_body = serde_json::json!({"app_name":app, "artifact_url": format!("/artifacts/{digest}"), "signature": null}).to_string();
    let dep_req = Request::builder().method("POST").uri("/deployments").header("content-type","application/json").body(Body::from(dep_body)).unwrap();
    let dep_resp = router.clone().oneshot(dep_req).await.unwrap();
    assert_eq!(dep_resp.status(), StatusCode::CREATED, "deployment should pass with validated SBOM & manifest digest match");
    std::env::remove_var("AETHER_ENFORCE_SBOM");
}

#[tokio::test]
#[serial_test::serial]
async fn deployment_blocked_without_sbom() {
    std::env::set_var("AETHER_ENFORCE_SBOM", "1");
    let state = control_plane::test_support::test_state().await;
    let digest = "2222222222222222222222222222222222222222222222222222222222222222";
    let app = "needsbom";
    let router = prepare_artifact(app, digest, &state).await;
    // No manifest/SBOM yet -> deployment must fail
    let dep_body = serde_json::json!({"app_name":app, "artifact_url": format!("/artifacts/{digest}"), "signature": null}).to_string();
    let dep_req = Request::builder().method("POST").uri("/deployments").header("content-type","application/json").body(Body::from(dep_body)).unwrap();
    let dep_resp = router.clone().oneshot(dep_req).await.unwrap();
    assert_eq!(dep_resp.status(), StatusCode::BAD_REQUEST);
    let msg = axum::body::to_bytes(dep_resp.into_body(), 1024).await.unwrap();
    let v: serde_json::Value = serde_json::from_slice(&msg).unwrap();
    assert!(v["message"].as_str().unwrap().to_lowercase().contains("sbom"));
    std::env::remove_var("AETHER_ENFORCE_SBOM");
}

#[tokio::test]
#[serial_test::serial]
async fn manifest_sbom_mismatch_blocks() {
    std::env::set_var("AETHER_ENFORCE_SBOM", "1");
    let state = control_plane::test_support::test_state().await;
    let digest = "3333333333333333333333333333333333333333333333333333333333333333"; let app="mismatch";
    let router = prepare_artifact(app, digest, &state).await;
    // Upload manifest
    let files = [("/bin/a","aaaa"),("/bin/b","bbbb")];
    let m_digest = manifest_digest(&files);
    let manifest_body = serde_json::json!({"files": files.iter().map(|(p,d)| serde_json::json!({"path":p, "sha256":d})).collect::<Vec<_>>()}).to_string();
    let m_req = Request::builder().method("POST").uri(format!("/artifacts/{digest}/manifest")).header("content-type","application/json").body(Body::from(manifest_body)).unwrap();
    let m_resp = router.clone().oneshot(m_req).await.unwrap();
    assert_eq!(m_resp.status(), StatusCode::CREATED);
    // SBOM with DIFFERENT x-manifest-digest
    let sbom_doc = serde_json::json!({"bomFormat":"CycloneDX","specVersion":"1.5","components":[{"type":"container","name":"artifact"}],"x-manifest-digest": format!("{m_digest}bad")});
    let sbom_req = Request::builder().method("POST").uri(format!("/artifacts/{digest}/sbom")).header("content-type","application/json").body(Body::from(sbom_doc.to_string())).unwrap();
    let sbom_resp = router.clone().oneshot(sbom_req).await.unwrap();
    assert_eq!(sbom_resp.status(), StatusCode::BAD_REQUEST, "mismatched manifest digest should 400");
    let body = axum::body::to_bytes(sbom_resp.into_body(), 1024).await.unwrap();
    let v: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(v["message"].as_str().unwrap().contains("manifest digest mismatch"));
    std::env::remove_var("AETHER_ENFORCE_SBOM");
}

#[tokio::test]
#[serial_test::serial]
async fn sbom_then_manifest_mismatch_blocks() {
    std::env::set_var("AETHER_ENFORCE_SBOM", "1");
    let state = control_plane::test_support::test_state().await;
    let digest = "4444444444444444444444444444444444444444444444444444444444444444"; let app="order";
    let router = prepare_artifact(app, digest, &state).await;
    // First SBOM with x-manifest-digest X (no manifest yet so accepted)
    let bogus = "ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff"; // random 64 hex
    let sbom_doc = serde_json::json!({"bomFormat":"CycloneDX","specVersion":"1.5","components":[{"type":"container","name":"artifact"}],"x-manifest-digest": bogus});
    let sbom_req = Request::builder().method("POST").uri(format!("/artifacts/{digest}/sbom")).header("content-type","application/json").body(Body::from(sbom_doc.to_string())).unwrap();
    let sbom_resp = router.clone().oneshot(sbom_req).await.unwrap();
    assert_eq!(sbom_resp.status(), StatusCode::CREATED);
    // Now upload manifest with DIFFERENT digest -> should 400
    let files = [("/bin/x","1111"),("/bin/y","2222")];
    let correct_manifest_digest = manifest_digest(&files);
    assert_ne!(correct_manifest_digest, bogus);
    let manifest_body = serde_json::json!({"files": files.iter().map(|(p,d)| serde_json::json!({"path":p, "sha256":d})).collect::<Vec<_>>()}).to_string();
    let m_req = Request::builder().method("POST").uri(format!("/artifacts/{digest}/manifest")).header("content-type","application/json").body(Body::from(manifest_body)).unwrap();
    let m_resp = router.clone().oneshot(m_req).await.unwrap();
    assert_eq!(m_resp.status(), StatusCode::BAD_REQUEST, "manifest digest mismatch should 400 when SBOM already declares x-manifest-digest");
    std::env::remove_var("AETHER_ENFORCE_SBOM");
}

#[tokio::test]
#[serial_test::serial]
async fn metrics_increment_on_invalid_sbom() {
    let state = control_plane::test_support::test_state().await;
    let digest = "5555555555555555555555555555555555555555555555555555555555555555"; let app="metrics";
    let router = prepare_artifact(app, digest, &state).await;
    // Invalid SBOM (wrong bomFormat)
    let bad = serde_json::json!({"bomFormat":"NotCyclone","specVersion":"1.5","components":[]});
    let req = Request::builder().method("POST").uri(format!("/artifacts/{digest}/sbom")).header("content-type","application/json").body(Body::from(bad.to_string())).unwrap();
    let resp = router.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    // Fetch metrics and ensure sbom_invalid_total increased
    let metrics_req = Request::builder().method("GET").uri("/metrics").body(Body::empty()).unwrap();
    let metrics_resp = router.clone().oneshot(metrics_req).await.unwrap();
    assert_eq!(metrics_resp.status(), StatusCode::OK);
    let body = axum::body::to_bytes(metrics_resp.into_body(), 16 * 1024).await.unwrap();
    let text = String::from_utf8(body.to_vec()).unwrap();
    assert!(text.contains("sbom_invalid_total"), "metrics exposition missing sbom_invalid_total\n{text}");
}
