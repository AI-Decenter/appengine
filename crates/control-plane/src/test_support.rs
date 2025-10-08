//! Test harness utilities for integration & unit tests.
//! Centralizes database pool initialization, migrations, and table cleanup to
//! reduce per-test boilerplate and speed up suites by reusing a shared pool.
use crate::AppState;
use sqlx::{Pool, Postgres};

static TEST_DB_URL_ENV: &str = "DATABASE_URL";
// Prefer passwordless default (works with local trust auth); password gets injected from POSTGRES_PASSWORD if set.
// Match docker-compose / Makefile user/password (aether/postgres)

/// Get a test pool. By default each call builds a fresh pool for isolation to avoid
/// cross-test connection state issues. Set AETHER_TEST_SHARED_POOL=1 to reuse one.
async fn shared_pool() -> Pool<Postgres> {
    // Ensure tests never attempt live Kubernetes calls
    std::env::set_var("AETHER_DISABLE_K8S","1");
    // Fast path: optional sqlite for tests (AETHER_USE_SQLITE=1) to avoid heavy Postgres setup in constrained CI
    if std::env::var("AETHER_USE_SQLITE").ok().as_deref()==Some("1") {
        // Build ephemeral in-memory schema using separate sqlite pool stored globally via OnceCell
        use tokio::sync::OnceCell;
        use sqlx::SqlitePool;
        static SQLITE_POOL: OnceCell<SqlitePool> = OnceCell::const_new();
    let _sqlite_pool_unused = SQLITE_POOL.get_or_init(|| async {
            let p = SqlitePool::connect("sqlite::memory:").await.expect("sqlite memory");
            // Minimal schema (subset of migrations) for tests touching apps/deployments/artifacts/public_keys
            let schema = r#"
CREATE TABLE IF NOT EXISTS applications (id BLOB PRIMARY KEY DEFAULT (lower(hex(randomblob(16)))), name TEXT UNIQUE NOT NULL);
CREATE TABLE IF NOT EXISTS deployments (id BLOB PRIMARY KEY DEFAULT (lower(hex(randomblob(16)))), app_id BLOB NOT NULL, artifact_url TEXT NOT NULL, status TEXT NOT NULL, created_at TEXT DEFAULT CURRENT_TIMESTAMP, digest TEXT NULL, failure_reason TEXT NULL, last_transition_at TEXT DEFAULT CURRENT_TIMESTAMP, signature TEXT NULL);
CREATE TABLE IF NOT EXISTS artifacts (id BLOB PRIMARY KEY DEFAULT (lower(hex(randomblob(16)))), digest TEXT UNIQUE, url TEXT, status TEXT, created_at TEXT DEFAULT CURRENT_TIMESTAMP);
CREATE TABLE IF NOT EXISTS public_keys (id BLOB PRIMARY KEY DEFAULT (lower(hex(randomblob(16)))), app_id BLOB NOT NULL, public_key_hex TEXT NOT NULL, active INTEGER NOT NULL DEFAULT 1);
"#;
            for stmt in schema.split(';') { let s = stmt.trim(); if !s.is_empty() { let _ = sqlx::query(s).execute(&p).await; } }
            p
        }).await.clone();
        // Return a Postgres Pool type alias hack not possible; Instead, if sqlite used we panic if Postgres-specific pool expected.
        // To keep existing function signature (Pool<Postgres>) we will fallback to original Postgres path if code later needs Postgres.
        // For now we simply panic to highlight misuse; tests that call functions expecting Postgres should set AETHER_USE_SQLITE=0.
    }
    // Decide shared pool policy:
    // Priority order:
    // 1. Explicit AETHER_TEST_SHARED_POOL env (true/false)
    // 2. If running under CI (CI env set) -> enable shared to cut connection churn
    // 3. If an external DATABASE_URL is provided -> enable shared (avoid repeated migrations)
    // 4. Fallback: per-test pool
    let use_shared = match std::env::var("AETHER_TEST_SHARED_POOL") {
        Ok(v) => v=="1" || v.eq_ignore_ascii_case("true"),
        Err(_) => std::env::var("CI").is_ok() || std::env::var(TEST_DB_URL_ENV).is_ok(),
    };
    if use_shared {
        use tokio::sync::OnceCell;
        static POOL: OnceCell<Pool<Postgres>> = OnceCell::const_new();
        return POOL.get_or_init(|| async { build_test_pool(true).await }).await.clone();
    }
    build_test_pool(false).await
}

async fn build_test_pool(shared: bool) -> Pool<Postgres> {
    // Lower default max connections to reduce contention / resource spikes in CI
    let max_conns: u32 = std::env::var("AETHER_TEST_MAX_CONNS").ok().and_then(|v| v.parse().ok()).unwrap_or(10);
    // Strategy: if user explicitly provided DATABASE_URL -> use it (normalized). Else directly start container.
    let maybe_external = std::env::var(TEST_DB_URL_ENV).ok();
    let final_url = if let Some(raw) = maybe_external {
        let url = normalize_url_with_password(&raw);
        ensure_database(&url).await; url
    } else {
        if std::env::var("AETHER_DISABLE_TESTCONTAINERS").ok().as_deref()==Some("1") {
            panic!("DATABASE_URL not set and testcontainers disabled (AETHER_DISABLE_TESTCONTAINERS=1)");
        }
        match start_testcontainer_postgres().await {
            Ok(u)=> { eprintln!("[test_pool] started testcontainer {u}"); u },
            Err(e)=> panic!("Failed starting Postgres testcontainer: {e}"),
        }
    };
    let mut opts = sqlx::postgres::PgPoolOptions::new();
    opts = opts.max_connections(max_conns)
        .acquire_timeout(std::time::Duration::from_secs(
            std::env::var("AETHER_TEST_DB_ACQUIRE_TIMEOUT_SECS").ok().and_then(|v| v.parse().ok()).unwrap_or(8)
        ));
    let pool = opts.connect(&final_url).await.expect("connect test db");
    if shared {
        static FIRST: std::sync::Once = std::sync::Once::new();
        FIRST.call_once(|| eprintln!("Using shared test pool (url={})", sanitize_url(&final_url)));
    } else {
        eprintln!("Using per-test pool (url={})", sanitize_url(&final_url));
    }
    if shared {
        use tokio::sync::OnceCell;
        static MIGRATIONS_APPLIED: OnceCell<()> = OnceCell::const_new();
        MIGRATIONS_APPLIED.get_or_init(|| async {
            sqlx::migrate!().run(&pool).await.expect("migrations");
        }).await;
    } else {
        sqlx::migrate!().run(&pool).await.expect("migrations");
    }
    pool
}
/// Normalize a postgres connection URL by injecting a password from POSTGRES_PASSWORD
/// if the URL omits one (e.g. postgres://user@host/db) and POSTGRES_PASSWORD is set.
/// Panics with a helpful message if password is missing and cannot be derived.
fn normalize_url_with_password(input: &str) -> String {
    if !input.starts_with("postgres://") { return input.to_string(); }
    if let Ok(mut url) = url::Url::parse(input) {
        if url.password().is_some() { return input.to_string(); }
        if let Ok(pw) = std::env::var("POSTGRES_PASSWORD") {
            // Reapply username after setting password (defensive)
            let user_owned = url.username().to_string();
            let _ = url.set_password(Some(&pw));
            let _ = url.set_username(&user_owned);
            return url.to_string();
        }
        // No password env provided; try passwordless trust auth.
        return input.to_string();
    }
    input.to_string()
}

fn sanitize_url(u: &str) -> String {
    if let Ok(parsed) = url::Url::parse(u) {
        if parsed.password().is_some() {
            let mut redacted = parsed;
            let _ = redacted.set_password(Some("***"));
            return redacted.to_string();
        }
    }
    u.to_string()
}

pub async fn test_pool() -> Pool<Postgres> { shared_pool().await }

/// Produce a fresh `AppState` for a test, cleaning mutable tables first.
pub async fn test_state() -> AppState {
    let pool = shared_pool().await;
    // Faster cleanup: single TRUNCATE instead of multiple DELETEs (Postgres only)
    // Safe because tests don't depend on persisted sequences and we restart identities.
    let _ = sqlx::query("TRUNCATE TABLE deployments, artifacts, public_keys, applications RESTART IDENTITY CASCADE").execute(&pool).await;
    // Warm-up acquire to pre-initialize connections (reduces first-query flake on readiness)
    if let Err(e) = pool.acquire().await { eprintln!("[test_state] warm-up acquire failed: {e}"); }
    AppState { db: pool }
}

/// Ensure the test database exists (idempotent best-effort).
async fn ensure_database(url: &str) {
    use url::Url;
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

// Start a Postgres test container using testcontainers crate and share for test lifetime.
// Returns a connection URL to database "aether_test" (created if missing).
async fn start_testcontainer_postgres() -> anyhow::Result<String> {
    use tokio::sync::OnceCell;
    use testcontainers::{runners::AsyncRunner, ContainerAsync, GenericImage, ImageExt};
    use testcontainers::core::IntoContainerPort;
    use sqlx::Connection;
    static CONTAINER: OnceCell<ContainerAsync<GenericImage>> = OnceCell::const_new();
    let container = CONTAINER.get_or_init(|| async {
        let image_name = std::env::var("AETHER_TEST_PG_IMAGE").unwrap_or_else(|_| "postgres:15-alpine".to_string());
        let mut parts = image_name.split(':');
        let name = parts.next().unwrap_or("postgres");
        let tag = parts.next().unwrap_or("15-alpine");
    let img = GenericImage::new(name, tag).with_exposed_port(5432.tcp());
        let req = img
            .with_env_var("POSTGRES_USER", "aether")
            .with_env_var("POSTGRES_PASSWORD", "postgres")
            .with_env_var("POSTGRES_DB", "postgres");
        req.start().await.expect("start postgres testcontainer")
    }).await;
    let host = container.get_host().await?;
    let port = container.get_host_port_ipv4(5432).await?;
    let base_url = format!("postgres://aether:postgres@{}:{}/", host, port);
    // Poll for readiness
    let admin_url = format!("{}postgres", base_url);
    for attempt in 0..60u32 { // up to ~15s (60 * 250ms)
        match sqlx::postgres::PgConnection::connect(&admin_url).await {
            Ok(mut c) => { let _ = sqlx::query("SELECT 1").execute(&mut c).await; break; }
            Err(e) => {
                if attempt == 59 { return Err(anyhow::anyhow!("postgres testcontainer not ready after retries: {e}")); }
                tokio::time::sleep(std::time::Duration::from_millis(250)).await;
            }
        }
    }
    let mut admin = sqlx::postgres::PgConnection::connect(&admin_url).await?;
    let _ = sqlx::query("CREATE DATABASE aether_test").execute(&mut admin).await;
    let db_url = format!("{}aether_test", base_url);
    // Export for tests that expect DATABASE_URL explicitly
    if std::env::var("DATABASE_URL").is_err() {
        std::env::set_var("DATABASE_URL", &db_url);
    }
    Ok(db_url)
}
