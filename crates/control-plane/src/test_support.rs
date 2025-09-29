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
                // Attempt embedded Postgres fallback if allowed (default allow). Disable by setting AETHER_DISABLE_EMBEDDED_PG=1
                if std::env::var("AETHER_DISABLE_EMBEDDED_PG").ok().as_deref() != Some("1") {
                    eprintln!("[test_pool] No external Postgres reachable. Launching embedded Postgres for tests...");
                    match launch_embedded_pg().await {
                        Ok((url, _guard)) => {
                            // Keep guard alive in static to hold process for test lifetime
                            use once_cell::sync::OnceCell;
                            static PG_GUARD: OnceCell<pg_embed::postgres::PgEmbed> = OnceCell::new();
                            let embed_ref = _guard; // rename
                            PG_GUARD.set(embed_ref).ok();
                            // Connect a pool now
                            let pool = sqlx::postgres::PgPoolOptions::new().max_connections(5).connect(&url).await.expect("embed pg pool");
                            sqlx::migrate!().run(&pool).await.expect("migrations (embedded)");
                            pool
                        }
                        Err(e) => {
                            eprintln!("[test_pool] Embedded Postgres launch failed: {e}");
                            eprintln!("[test_pool] All connection candidates failed:\n{}", candidates.join("\n"));
                            panic!("Failed to connect to Postgres (external + embedded). Set DATABASE_URL or install local postgres.");
                        }
                    }
                } else {
                    eprintln!("[test_pool] All connection candidates failed and embedded disabled:\n{}", candidates.join("\n"));
                    panic!("Failed to connect to Postgres (see above candidates). Set TEST DATABASE_URL or start docker compose.");
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

// Launch an embedded Postgres instance on a dynamic port.
// Returns (database_url, guard) where guard must stay alive.
async fn launch_embedded_pg() -> anyhow::Result<(String, pg_embed::postgres::PgEmbed)> {
    use pg_embed::{postgres::{PgEmbed, PgSettings}, pg_enums::PgAuthMethod, pg_fetch::{PgFetchSettings, PG_V15}};
    use std::path::PathBuf;
    use sqlx::Connection;
    let port = portpicker::pick_unused_port().unwrap_or(55432);
    let target_dir = std::env::var("CARGO_TARGET_DIR").map(PathBuf::from).unwrap_or_else(|_| PathBuf::from("target"));
    let data_dir = target_dir.join("embedded-pg-test");
    let settings = PgSettings {
        database_dir: data_dir,
        port: port as u16,
        user: "aether".into(),
        password: "postgres".into(),
        auth_method: PgAuthMethod::Plain,
        persistent: true,
        timeout: Some(std::time::Duration::from_secs(15)),
        migration_dir: None,
    };
    let fetch = PgFetchSettings { version: PG_V15, ..Default::default() };
    let mut pg = PgEmbed::new(settings, fetch).await?;
    pg.setup().await?;
    pg.start_db().await?;
    // Create the database if not exists
    let admin_url = format!("{}/postgres", pg.db_uri);
    let mut admin_conn = sqlx::postgres::PgConnection::connect(&admin_url).await?;
    let _ = sqlx::query("CREATE DATABASE aether_test").execute(&mut admin_conn).await;
    let db_url = format!("{}/aether_test", pg.db_uri);
    eprintln!("[embedded-pg] started at {}", db_url);
    Ok((db_url, pg))
}
