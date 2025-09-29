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
    let url = match std::env::var("DATABASE_URL") { Ok(v)=>v, Err(_)=> { eprintln!("skipping schema_core_tables_exist: DATABASE_URL not set"); return; } };
    let pool = init_db(&url).await.expect("db init failed");
    sqlx::migrate!().run(&pool).await.expect("migrations failed");
    let required = ["applications","artifacts","public_keys","deployments"];
    for table in required {
        // Use EXISTS returning BOOL to avoid any driver int width decoding issues
        let exists: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM information_schema.tables WHERE table_schema='public' AND table_name=$1)")
            .bind(table).fetch_one(&pool).await.unwrap();
        assert!(exists, "table '{}' missing after migrations", table);
    }
}