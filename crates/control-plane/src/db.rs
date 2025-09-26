use sqlx::{Pool, Postgres};
use std::time::Duration;
use tracing::{info, error};

pub async fn init_db(database_url: &str) -> anyhow::Result<Pool<Postgres>> {
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(5)
        .acquire_timeout(Duration::from_secs(3))
        .connect(database_url)
        .await?;
    if let Err(e) = sqlx::migrate!().run(&pool).await { error!(error=%e, "migration failure"); } else { info!("migrations applied (if any)"); }
    Ok(pool)
}
