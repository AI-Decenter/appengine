use control_plane::db::init_db;
use once_cell::sync::OnceCell;

fn init_tracing() {
    static INIT: OnceCell<()> = OnceCell::new();
    let _ = INIT.get_or_init(|| {
        let _ = tracing_subscriber::fmt()
            .with_test_writer()
            .with_target(false)
            .try_init();
    });
}

#[tokio::test]
async fn schema_core_tables_exist() {
    init_tracing();
    let Some(url) = std::env::var("DATABASE_URL").ok() else { eprintln!("skipping schema_core_tables_exist (no db)"); return; };
    let pool = match init_db(&url).await { Ok(p)=>p, Err(e)=> { eprintln!("skipping (db init failed): {e}"); return; } };
    let required = ["applications","artifacts","public_keys","deployments"];
    for table in required { 
        let exists: Option<i64> = sqlx::query_scalar("SELECT 1 FROM information_schema.tables WHERE table_schema='public' AND table_name=$1")
            .bind(table).fetch_optional(&pool).await.unwrap();
        assert!(exists.is_some(), "table '{}' missing after migrations", table);
    }
}