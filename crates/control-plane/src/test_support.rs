//! Test harness utilities for integration & unit tests.
//! Centralizes database pool initialization, migrations, and table cleanup to
//! reduce per-test boilerplate and speed up suites by reusing a shared pool.
use crate::AppState;
use sqlx::{Pool, Postgres};

static TEST_DB_URL_ENV: &str = "DATABASE_URL";
static DEFAULT_TEST_DB: &str = "postgres://postgres:postgres@localhost:5432/aether_test";

/// Get (or initialize) a shared migrated Postgres pool for tests.
async fn shared_pool() -> Pool<Postgres> {
    use tokio::sync::OnceCell;
    static POOL: OnceCell<Pool<Postgres>> = OnceCell::const_new();
    POOL.get_or_init(|| async {
        let url = std::env::var(TEST_DB_URL_ENV).unwrap_or_else(|_| DEFAULT_TEST_DB.into());
        ensure_database(&url).await;
        let pool = sqlx::postgres::PgPoolOptions::new()
            .max_connections(10)
            .connect(&url)
            .await
            .expect("connect test db");
        sqlx::migrate!().run(&pool).await.expect("migrations");
        pool
    }).await.clone()
}

/// Produce a fresh `AppState` for a test, cleaning mutable tables first.
pub async fn test_state() -> AppState {
    let pool = shared_pool().await;
    let _ = sqlx::query("DELETE FROM deployments").execute(&pool).await;
    let _ = sqlx::query("DELETE FROM artifacts").execute(&pool).await;
    let _ = sqlx::query("DELETE FROM public_keys").execute(&pool).await;
    let _ = sqlx::query("DELETE FROM applications").execute(&pool).await;
    AppState { db: pool }
}

/// Ensure the test database exists (idempotent best-effort).
async fn ensure_database(url: &str) {
    use reqwest::Url; // already available in dev-dependencies
    let parsed = match Url::parse(url) { Ok(p)=>p, Err(_)=> return };
    let db_name = parsed.path().trim_start_matches('/').to_string();
    if db_name.is_empty() { return; }
    let mut admin = parsed.clone();
    admin.set_path("/postgres");
    if let Ok(admin_pool) = sqlx::postgres::PgPoolOptions::new().max_connections(1).connect(admin.as_str()).await {
        let exists: Option<String> = sqlx::query_scalar("SELECT datname FROM pg_database WHERE datname=$1")
            .bind(&db_name)
            .fetch_optional(&admin_pool).await.ok().flatten();
        if exists.is_none() && db_name.chars().all(|c| c.is_ascii_alphanumeric() || c=='_') {
            let _ = sqlx::query(&format!("CREATE DATABASE {}", db_name)).execute(&admin_pool).await;
        }
    }
}
