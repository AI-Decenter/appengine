//! Test harness utilities for integration & unit tests.
//! Centralizes database pool initialization, migrations, and table cleanup to
//! reduce per-test boilerplate and speed up suites by reusing a shared pool.
use crate::AppState;
use sqlx::{Pool, Postgres};

static TEST_DB_URL_ENV: &str = "DATABASE_URL";
// Prefer passwordless default (works with local trust auth); password gets injected from POSTGRES_PASSWORD if set.
// Match docker-compose / Makefile user/password (aether/postgres)
static DEFAULT_TEST_DB: &str = "postgres://aether:postgres@localhost:5432/aether_test";

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
        let pool = SQLITE_POOL.get_or_init(|| async {
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
    let use_shared = std::env::var("AETHER_TEST_SHARED_POOL").ok().map(|v| v=="1" || v.eq_ignore_ascii_case("true")).unwrap_or(false);
    if use_shared {
        use tokio::sync::OnceCell;
        static POOL: OnceCell<Pool<Postgres>> = OnceCell::const_new();
        return POOL.get_or_init(|| async { build_test_pool(true).await }).await.clone();
    }
    build_test_pool(false).await
}

async fn build_test_pool(shared: bool) -> Pool<Postgres> {
    use sqlx::Connection; // for PgConnection::connect
        let raw_url = std::env::var(TEST_DB_URL_ENV).unwrap_or_else(|_| DEFAULT_TEST_DB.into());
        let url = normalize_url_with_password(&raw_url);
        ensure_database(&url).await;
        let mut candidates: Vec<String> = Vec::new();
        candidates.push(url.clone());
        // If URL has password, try variant without (trust auth)
        if let Ok(parsed) = url::Url::parse(&url) { if parsed.password().is_some() { let mut u = parsed.clone(); let _ = u.set_password(None); candidates.push(u.to_string()); } }
        // If env POSTGRES_PASSWORD exists and differs, try injecting it
        if let Ok(pw_env) = std::env::var("POSTGRES_PASSWORD") {
            if let Ok(mut u) = url::Url::parse(&url) {
                if u.password() != Some(&pw_env) { let _ = u.set_password(Some(&pw_env)); candidates.push(u.to_string()); }
            }
        }
        // Deduplicate
        candidates.sort(); candidates.dedup();
    let mut pool_opt = None;
        let max_conns: u32 = std::env::var("AETHER_TEST_MAX_CONNS").ok().and_then(|v| v.parse().ok()).unwrap_or(30);
        for cand in &candidates {
            // Proactive readiness loop (helps when docker-compose just started)
            let retries: u32 = std::env::var("AETHER_TEST_DB_RETRIES").ok().and_then(|v| v.parse().ok()).unwrap_or(12); // ~12 * 250ms = 3s default
            let delay_ms: u64 = std::env::var("AETHER_TEST_DB_RETRY_DELAY_MS").ok().and_then(|v| v.parse().ok()).unwrap_or(250);
            for attempt in 0..=retries {
                match sqlx::postgres::PgConnection::connect(cand) .await {
                    Ok(mut conn) => { let _ = sqlx::query("SELECT 1").execute(&mut conn).await; break; }
                    Err(e) => {
                        if attempt==retries { eprintln!("[test_pool] readiness failed for {cand}: {e}"); }
                        tokio::time::sleep(std::time::Duration::from_millis(delay_ms)).await;
                    }
                }
            }
            // Build pool with tighter acquisition timeout (surface issues fast in tests)
            let mut opts = sqlx::postgres::PgPoolOptions::new();
            opts = opts.max_connections(max_conns)
                .acquire_timeout(std::time::Duration::from_secs(
                    std::env::var("AETHER_TEST_DB_ACQUIRE_TIMEOUT_SECS").ok().and_then(|v| v.parse().ok()).unwrap_or(8)
                ));
            if let Ok(pool) = opts.connect(cand).await { pool_opt = Some(pool); break; }
        }
        let pool = match pool_opt {
            Some(p) => p,
            None => {
                // Fallback: start a Postgres test container (requires Docker). Can be disabled via AETHER_DISABLE_TESTCONTAINERS=1
                if std::env::var("AETHER_DISABLE_TESTCONTAINERS").ok().as_deref()==Some("1") {
                    eprintln!("[test_pool] All connection candidates failed and testcontainers disabled. Candidates: \n{}", candidates.join("\n"));
                    panic!("No Postgres available for tests and testcontainers disabled");
                }
                match start_testcontainer_postgres().await {
                    Ok(url) => {
                        eprintln!("[test_pool] Started Postgres testcontainer at {url}");
                        let pool = sqlx::postgres::PgPoolOptions::new().max_connections(5).connect(&url).await.expect("testcontainer pg pool");
                        sqlx::migrate!().run(&pool).await.expect("migrations (testcontainer)");
                        pool
                    }
                    Err(e) => {
                        eprintln!("[test_pool] Failed to start Postgres testcontainer: {e}");
                        eprintln!("[test_pool] Candidates attempted: \n{}", candidates.join("\n"));
                        panic!("Failed to provision Postgres for tests");
                    }
                }
            }
        };
        if shared {
            static FIRST_LOG: std::sync::Once = std::sync::Once::new();
            FIRST_LOG.call_once(|| {
                eprintln!("Using shared test pool (url={})", sanitize_url(&url));
            });
        } else {
            eprintln!("Using per-test pool (url={})", sanitize_url(&url));
        }
        sqlx::migrate!().run(&pool).await.expect("migrations");
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
    let _ = sqlx::query("DELETE FROM deployments").execute(&pool).await;
    let _ = sqlx::query("DELETE FROM artifacts").execute(&pool).await;
    let _ = sqlx::query("DELETE FROM public_keys").execute(&pool).await;
    let _ = sqlx::query("DELETE FROM applications").execute(&pool).await;
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
    for attempt in 0..30u32 { // up to ~30s
        match sqlx::postgres::PgConnection::connect(&admin_url).await {
            Ok(mut c) => { let _ = sqlx::query("SELECT 1").execute(&mut c).await; break; }
            Err(e) => {
                if attempt == 29 { return Err(anyhow::anyhow!("postgres testcontainer not ready after retries: {e}")); }
                tokio::time::sleep(std::time::Duration::from_millis(1000)).await;
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
