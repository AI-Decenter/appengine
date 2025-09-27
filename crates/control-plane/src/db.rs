use sqlx::{Pool, Postgres};
use std::time::Duration;
use tracing::info;

pub async fn init_db(database_url: &str) -> anyhow::Result<Pool<Postgres>> {
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(5)
        .acquire_timeout(Duration::from_secs(3))
        .connect(database_url)
        .await?;
        sqlx::migrate!().run(&pool).await?;
    info!("migrations applied");
    Ok(pool)
}
