use axum::{body::Body, http::{Request, StatusCode}, middleware};
use control_plane::{build_router, AppState, db::init_db};
use tower::util::ServiceExt; // for oneshot
use sha2::{Sha256, Digest};
use ed25519_dalek::{SigningKey, Signature, Signer};

// Helper to skip if no DATABASE_URL
async fn maybe_pool() -> Option<sqlx::Pool<sqlx::Postgres>> {
    let url = std::env::var("DATABASE_URL").ok()?;
    init_db(&url).await.ok()
}

fn multipart_body(fields: Vec<(&str, &str)>, file: Option<(&str, Vec<u8>)>) -> (Vec<u8>, String) {
    let boundary = format!("----BOUNDARYTEST{}", uuid::Uuid::new_v4());
    let mut body: Vec<u8> = Vec::new();
    for (name, val) in fields {
        body.extend(format!("--{}\r\n", boundary).as_bytes());
        body.extend(format!("Content-Disposition: form-data; name=\"{}\"\r\n\r\n{}\r\n", name, val).as_bytes());
    }
    if let Some((name, bytes)) = file {
        body.extend(format!("--{}\r\n", boundary).as_bytes());
        body.extend(format!("Content-Disposition: form-data; name=\"{}\"; filename=\"f.tar.gz\"\r\nContent-Type: application/gzip\r\n\r\n", name).as_bytes());
        body.extend(&bytes);
        body.extend(b"\r\n");
    }
    body.extend(format!("--{}--\r\n", boundary).as_bytes());
    (body, boundary)
}

#[tokio::test]
async fn upload_missing_digest() {
    let Some(pool) = maybe_pool().await else { eprintln!("skipping (no db)"); return; };
    let app = build_router(AppState { db: pool });
    let (artifact_bytes, boundary) = multipart_body(vec![("app_name","demo")], Some(("artifact", b"data".to_vec())));
    let req = Request::builder().method("POST").uri("/artifacts")
        .header("content-type", format!("multipart/form-data; boundary={}", boundary))
        .body(Body::from(artifact_bytes)).unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn upload_digest_mismatch() {
    let Some(pool) = maybe_pool().await else { eprintln!("skipping (no db)"); return; };
    let app = build_router(AppState { db: pool });
    let data = b"abcdef".to_vec();
    let (artifact_bytes, boundary) = multipart_body(vec![("app_name","demo")], Some(("artifact", data.clone())));
    // compute wrong digest
    let mut h = Sha256::new(); h.update(b"zzz"); let wrong = format!("{:x}", h.finalize());
    let req = Request::builder().method("POST").uri("/artifacts")
        .header("content-type", format!("multipart/form-data; boundary={}", boundary))
        .header("x-aether-artifact-digest", wrong)
        .body(Body::from(artifact_bytes)).unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn upload_ok_and_duplicate() {
    let Some(pool) = maybe_pool().await else { eprintln!("skipping (no db)"); return; };
    // Clean artifacts
    sqlx::query("DELETE FROM artifacts").execute(&pool).await.ok();
    let app = build_router(AppState { db: pool });
    let data = b"hello-world-data".to_vec();
    let mut h = Sha256::new(); h.update(&data); let dgst = format!("{:x}", h.finalize());
    let (artifact_bytes, boundary) = multipart_body(vec![("app_name","demo")], Some(("artifact", data.clone())));
    let req = Request::builder().method("POST").uri("/artifacts")
        .header("content-type", format!("multipart/form-data; boundary={}", boundary))
        .header("x-aether-artifact-digest", dgst.clone())
        .body(Body::from(artifact_bytes)).unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = axum::body::to_bytes(resp.into_body(), 1024).await.unwrap();
    let v: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(v["digest"].as_str().unwrap(), dgst);
    assert_eq!(v["duplicate"], false);
    // Duplicate
    let (artifact_bytes2, boundary2) = multipart_body(vec![("app_name","demo")], Some(("artifact", data.clone())));
    let req2 = Request::builder().method("POST").uri("/artifacts")
        .header("content-type", format!("multipart/form-data; boundary={}", boundary2))
        .header("x-aether-artifact-digest", dgst.clone())
        .body(Body::from(artifact_bytes2)).unwrap();
    let resp2 = app.clone().oneshot(req2).await.unwrap();
    assert_eq!(resp2.status(), StatusCode::OK);
    let body2 = axum::body::to_bytes(resp2.into_body(), 1024).await.unwrap();
    let v2: serde_json::Value = serde_json::from_slice(&body2).unwrap();
    assert!(v2["duplicate"].as_bool().unwrap());
}

#[tokio::test]
async fn upload_with_verification_true() {
    let Some(pool) = maybe_pool().await else { eprintln!("skipping (no db)"); return; };
    sqlx::query("DELETE FROM artifacts").execute(&pool).await.ok();
    sqlx::query("DELETE FROM applications").execute(&pool).await.ok();
    sqlx::query("INSERT INTO applications (name) VALUES ($1)").bind("verifapp").execute(&pool).await.unwrap();
    let app = build_router(AppState { db: pool.clone() });
    // Generate key
    let sk = SigningKey::generate(&mut rand::rngs::OsRng);
    let pk = sk.verifying_key();
    let pk_hex = hex::encode(pk.to_bytes());
    // Register key
    let body = serde_json::json!({"public_key_hex": pk_hex}).to_string();
    let req_reg = Request::builder().method("POST").uri("/apps/verifapp/public-keys")
        .header("content-type","application/json")
        .body(Body::from(body)).unwrap();
    let resp_reg = app.clone().oneshot(req_reg).await.unwrap();
    assert_eq!(resp_reg.status(), StatusCode::CREATED);
    // Prepare artifact
    let data = b"verifiable-bytes".to_vec();
    let mut h = Sha256::new(); h.update(&data); let dgst = format!("{:x}", h.finalize());
    let sig: Signature = sk.sign(dgst.as_bytes());
    let sig_hex = hex::encode(sig.to_bytes());
    let (artifact_bytes, boundary) = multipart_body(vec![("app_name","verifapp")], Some(("artifact", data)));
    let req = Request::builder().method("POST").uri("/artifacts")
        .header("content-type", format!("multipart/form-data; boundary={}", boundary))
        .header("x-aether-artifact-digest", dgst.clone())
        .header("x-aether-signature", sig_hex)
        .body(Body::from(artifact_bytes)).unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body_bytes = axum::body::to_bytes(resp.into_body(), 1024).await.unwrap();
    let v: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
    assert_eq!(v["verified"], true);
    // HEAD existence
    let head_req = Request::builder().method("HEAD").uri(format!("/artifacts/{}", dgst)).body(Body::empty()).unwrap();
    let head_resp = app.clone().oneshot(head_req).await.unwrap();
    assert_eq!(head_resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn upload_unauthorized() {
    // Preserve old value
    let prev = std::env::var("AETHER_API_TOKENS").ok();
    std::env::set_var("AETHER_API_TOKENS", "tok1,tok2");
    let Some(pool) = maybe_pool().await else { eprintln!("skipping (no db)"); if let Some(v)=prev { std::env::set_var("AETHER_API_TOKENS", v); } else { std::env::remove_var("AETHER_API_TOKENS"); } return; };
    // Build secured router (replicating auth logic from main)
    let tokens: Vec<String> = std::env::var("AETHER_API_TOKENS").unwrap().split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect();
    let secured = build_router(AppState { db: pool })
        .layer(middleware::from_fn(move |req: Request<Body>, next: axum::middleware::Next| {
            let tokens = tokens.clone();
            async move {
                let path = req.uri().path();
                let exempt = matches!(path, "/health"|"/readyz"|"/startupz"|"/metrics"|"/openapi.json"|"/swagger");
                if !exempt && !tokens.is_empty() {
                    let provided = req.headers().get("authorization").and_then(|v| v.to_str().ok()).unwrap_or("");
                    let ok = tokens.iter().any(|t| provided == format!("Bearer {t}"));
                    if !ok { return axum::response::Response::builder().status(401).body(Body::from("unauthorized")).unwrap(); }
                }
                next.run(req).await
            }
        }));
    let (body_bytes, boundary) = multipart_body(vec![("app_name","demo")], Some(("artifact", b"abc".to_vec())));
    let mut h = Sha256::new(); h.update(b"abc"); let dg = format!("{:x}", h.finalize());
    let req = Request::builder().method("POST").uri("/artifacts")
        .header("content-type", format!("multipart/form-data; boundary={}", boundary))
        .header("x-aether-artifact-digest", dg)
        .body(Body::from(body_bytes)).unwrap();
    let resp = secured.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    // restore env
    if let Some(v)=prev { std::env::set_var("AETHER_API_TOKENS", v); } else { std::env::remove_var("AETHER_API_TOKENS"); }
}
