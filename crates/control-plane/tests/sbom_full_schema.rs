use axum::{http::{Request, StatusCode}, body::Body};
use tower::util::ServiceExt;
use control_plane::{build_router, AppState};

#[tokio::test]
#[serial_test::serial]
async fn cyclonedx_full_schema_rejects_wrong_dep_structure() {
    std::env::set_var("AETHER_CYCLONEDX_FULL_SCHEMA", "1");
    let state = control_plane::test_support::test_state().await;
    // Prepare artifact
    let digest = "dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd";
    sqlx::query("DELETE FROM artifacts").execute(&state.db).await.ok();
    sqlx::query("DELETE FROM applications").execute(&state.db).await.ok();
    sqlx::query("INSERT INTO applications (name) VALUES ($1)").bind("fullschema").execute(&state.db).await.unwrap();
    let router = build_router(state.clone());
    let presign_body = serde_json::json!({"app_name":"fullschema","digest":digest}).to_string();
    let presign_req = Request::builder().method("POST").uri("/artifacts/presign").header("content-type","application/json").body(Body::from(presign_body)).unwrap();
    assert_eq!(router.clone().oneshot(presign_req).await.unwrap().status(), StatusCode::OK);
    let complete_body = serde_json::json!({"app_name":"fullschema","digest":digest,"size_bytes":0,"signature":null}).to_string();
    let comp_req = Request::builder().method("POST").uri("/artifacts/complete").header("content-type","application/json").body(Body::from(complete_body)).unwrap();
    assert_eq!(router.clone().oneshot(comp_req).await.unwrap().status(), StatusCode::OK);
    // Invalid: specVersion pattern 1.5 required but giving 1.4
    let bad = serde_json::json!({
        "bomFormat":"CycloneDX","specVersion":"1.4","components":[{"type":"container","name":"x"}],
        "dependencies":[{"ref":"x","dependsOn":["y"]}]
    });
    let bad_req = Request::builder().method("POST").uri(format!("/artifacts/{digest}/sbom")).header("content-type","application/json").body(Body::from(bad.to_string())).unwrap();
    let bad_resp = router.clone().oneshot(bad_req).await.unwrap();
    assert_eq!(bad_resp.status(), StatusCode::BAD_REQUEST, "expected schema rejection for specVersion 1.4 in full schema mode");
    // Valid: specVersion 1.5
    let good = serde_json::json!({
        "bomFormat":"CycloneDX","specVersion":"1.5","components":[{"type":"container","name":"x"}],
        "dependencies":[{"ref":"x","dependsOn":[]}]
    });
    let good_req = Request::builder().method("POST").uri(format!("/artifacts/{digest}/sbom")).header("content-type","application/json").body(Body::from(good.to_string())).unwrap();
    let good_resp = router.clone().oneshot(good_req).await.unwrap();
    assert_eq!(good_resp.status(), StatusCode::CREATED, "valid SBOM should pass full schema mode");
    std::env::remove_var("AETHER_CYCLONEDX_FULL_SCHEMA");
}
