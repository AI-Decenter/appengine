use axum::{http::{Request, StatusCode}, body::Body};
use tower::util::ServiceExt;
use control_plane::build_router;

#[tokio::test]
#[serial_test::serial]
async fn deployment_emits_provenance_and_supports_gzip_etag() {
    // Provide attestation key for DSSE signature
    let sk = [7u8;32];
    std::env::set_var("AETHER_ATTESTATION_SK", hex::encode(sk));
    std::env::set_var("AETHER_ATTESTATION_KEY_ID", "test-key");
    let state = control_plane::test_support::test_state().await;
    sqlx::query("DELETE FROM applications").execute(&state.db).await.ok();
    sqlx::query("INSERT INTO applications (name) VALUES ($1)").bind("provapp").execute(&state.db).await.unwrap();
    // Insert stored artifact manually
    let digest = "eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee";
    sqlx::query("INSERT INTO artifacts (app_id,digest,size_bytes,signature,sbom_url,manifest_url,verified,storage_key,status,created_at,provenance_present) VALUES ((SELECT id FROM applications WHERE name='provapp'),$1,0,NULL,NULL,NULL,FALSE,$2,'stored',NOW(),FALSE)")
        .bind(digest).bind(format!("artifacts/{digest}.tar.gz")).execute(&state.db).await.unwrap();
    let router = build_router(state.clone());
    let dep_body = serde_json::json!({"app_name":"provapp","artifact_url": format!("/artifacts/{digest}"), "signature": null}).to_string();
    let dep_req = Request::builder().method("POST").uri("/deployments").header("content-type","application/json").body(Body::from(dep_body)).unwrap();
    let dep_resp = router.clone().oneshot(dep_req).await.unwrap();
    assert_eq!(dep_resp.status(), StatusCode::CREATED);
    // Allow background task to run
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;
    // List provenance
    let list_req = Request::builder().method("GET").uri("/provenance").body(Body::empty()).unwrap();
    let list_resp = router.clone().oneshot(list_req).await.unwrap();
    assert_eq!(list_resp.status(), StatusCode::OK);
    let list_bytes = axum::body::to_bytes(list_resp.into_body(), 8192).await.unwrap();
    let arr: serde_json::Value = serde_json::from_slice(&list_bytes).unwrap();
    assert!(arr.as_array().unwrap().iter().any(|v| v.get("digest").and_then(|d| d.as_str())==Some(digest)), "digest not found in provenance list");
    // Fetch provenance with gzip
    let prov_req = Request::builder().method("GET").uri(format!("/provenance/{digest}")).header("accept-encoding","gzip").body(Body::empty()).unwrap();
    let prov_resp = router.clone().oneshot(prov_req).await.unwrap();
    assert_eq!(prov_resp.status(), StatusCode::OK);
    let etag = prov_resp.headers().get("ETag").cloned();
    assert!(prov_resp.headers().get("Content-Encoding").is_some(), "expected gzip encoding");
    // Conditional request (If-None-Match)
    if let Some(et) = etag {
        let cond_req = Request::builder().method("GET").uri(format!("/provenance/{digest}")).header("if-none-match", et.to_str().unwrap()).body(Body::empty()).unwrap();
        let cond_resp = router.clone().oneshot(cond_req).await.unwrap();
        assert_eq!(cond_resp.status(), StatusCode::NOT_MODIFIED);
    }
    // Attestation fetch
    let att_req = Request::builder().method("GET").uri(format!("/provenance/{digest}/attestation")).header("accept-encoding","gzip").body(Body::empty()).unwrap();
    let att_resp = router.clone().oneshot(att_req).await.unwrap();
    assert_eq!(att_resp.status(), StatusCode::OK);
    assert!(att_resp.headers().get("Content-Encoding").is_some());
    std::env::remove_var("AETHER_ATTESTATION_SK");
    std::env::remove_var("AETHER_ATTESTATION_KEY_ID");
}
