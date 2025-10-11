use axum::{body::Body, http::{Request, StatusCode}};
use tower::util::ServiceExt;

use control_plane::{build_router, AppState};

fn set_env(k: &str, v: &str) { std::env::set_var(k, v); }

#[tokio::test]
#[serial_test::serial]
async fn auth_env_mode_basic_rbac() {
    // Enable auth and set tokens
    set_env("AETHER_AUTH_ENABLED", "1");
    set_env("AETHER_AUTH_MODE", "env");
    set_env("AETHER_ADMIN_TOKEN", "admin_secret");
    set_env("AETHER_USER_TOKEN", "user_secret");
    // Disable background workers for determinism
    set_env("AETHER_DISABLE_BACKGROUND", "1");
    set_env("AETHER_DISABLE_WATCH", "1");
    set_env("AETHER_DISABLE_K8S", "1");

    let pool = control_plane::test_support::test_pool().await;
    // minimal DB state for POST /deployments: need an app row
    sqlx::query("DELETE FROM deployments").execute(&pool).await.ok();
    sqlx::query("DELETE FROM applications").execute(&pool).await.ok();
    sqlx::query("INSERT INTO applications (name) VALUES ($1)").bind("authapp").execute(&pool).await.unwrap();

    let app = build_router(AppState { db: pool });

    // Public endpoint is open
    let res = app.clone().oneshot(Request::builder().uri("/health").body(Body::empty()).unwrap()).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    // Protected endpoint without auth -> 401
    let res = app.clone().oneshot(Request::builder().uri("/deployments").body(Body::empty()).unwrap()).await.unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);

    // GET with user token -> 200
    let res = app.clone().oneshot(Request::builder().uri("/deployments")
        .header("authorization", "Bearer user_secret")
        .body(Body::empty()).unwrap()).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    // POST with user token -> 403 (admin only)
    let body = serde_json::json!({"app_name":"authapp","artifact_url":"file://x"}).to_string();
    let res = app.clone().oneshot(Request::builder().method("POST").uri("/deployments")
        .header("content-type","application/json")
        .header("authorization", "Bearer user_secret")
        .body(Body::from(body)).unwrap()).await.unwrap();
    assert_eq!(res.status(), StatusCode::FORBIDDEN);

    // POST with admin token -> 201
    let body = serde_json::json!({"app_name":"authapp","artifact_url":"file://y"}).to_string();
    let res = app.clone().oneshot(Request::builder().method("POST").uri("/deployments")
        .header("content-type","application/json")
        .header("authorization", "Bearer admin_secret")
        .body(Body::from(body)).unwrap()).await.unwrap();
    assert_eq!(res.status(), StatusCode::CREATED);
}

#[tokio::test]
#[serial_test::serial]
async fn auth_db_mode_allows_known_token() {
    // Enable auth and DB mode
    set_env("AETHER_AUTH_ENABLED", "1");
    set_env("AETHER_AUTH_MODE", "db");
    set_env("AETHER_DISABLE_BACKGROUND", "1");
    set_env("AETHER_DISABLE_WATCH", "1");
    set_env("AETHER_DISABLE_K8S", "1");

    let pool = control_plane::test_support::test_pool().await;
    // Ensure users table exists (migration will add it in implementation)
    // Prepare app row
    sqlx::query("DELETE FROM deployments").execute(&pool).await.ok();
    sqlx::query("DELETE FROM applications").execute(&pool).await.ok();
    sqlx::query("INSERT INTO applications (name) VALUES ($1)").bind("dbapp").execute(&pool).await.unwrap();

    // Insert a user with SHA-256 token hash and role admin
    let token_plain = "topsecret";
    let token_hash = {
        use sha2::{Sha256, Digest};
        let mut hasher = Sha256::new();
        hasher.update(token_plain.as_bytes());
        let out = hasher.finalize();
        hex::encode(out)
    };
    // Best-effort create table (idempotent for sqlite or PG)
    let _ = sqlx::query("CREATE TABLE IF NOT EXISTS users (id UUID PRIMARY KEY DEFAULT gen_random_uuid(), email TEXT, token_hash TEXT UNIQUE NOT NULL, role TEXT NOT NULL, created_at TIMESTAMPTZ DEFAULT now())").execute(&pool).await;
    sqlx::query("INSERT INTO users (email, token_hash, role) VALUES ($1,$2,$3) ON CONFLICT (token_hash) DO UPDATE SET role=excluded.role")
        .bind("a@b.c").bind(&token_hash).bind("admin").execute(&pool).await.unwrap();

    let app = build_router(AppState { db: pool.clone() });
    // GET without auth -> 401
    let res = app.clone().oneshot(Request::builder().uri("/deployments").body(Body::empty()).unwrap()).await.unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
    // POST with valid token -> 201
    let body = serde_json::json!({"app_name":"dbapp","artifact_url":"file://z"}).to_string();
    let res = app.clone().oneshot(Request::builder().method("POST").uri("/deployments")
        .header("content-type","application/json")
        .header("authorization", format!("Bearer {}", token_plain))
        .body(Body::from(body)).unwrap()).await.unwrap();
    assert_eq!(res.status(), StatusCode::CREATED);
}
