use sqlx::{Pool, Postgres};
use crate::models::Application;

pub async fn create_app(pool: &Pool<Postgres>, name: &str) -> Result<Application, sqlx::Error> {
    sqlx::query_as::<_, Application>("INSERT INTO applications (name) VALUES ($1) RETURNING id, name, created_at, updated_at")
        .bind(name)
        .fetch_one(pool).await
}

pub async fn list_apps(pool: &Pool<Postgres>) -> Result<Vec<Application>, sqlx::Error> {
    sqlx::query_as::<_, Application>("SELECT id, name, created_at, updated_at FROM applications ORDER BY created_at DESC")
        .fetch_all(pool).await
}
