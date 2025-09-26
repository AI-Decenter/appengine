use sqlx::{Pool, Postgres};
use crate::models::Deployment;
use sqlx::Row;

pub async fn create_deployment(pool: &Pool<Postgres>, app_name: &str, artifact_url: &str) -> Result<Deployment, sqlx::Error> {
    let rec = sqlx::query("SELECT id FROM applications WHERE name = $1")
        .bind(app_name).fetch_optional(pool).await?;
    let app_id: uuid::Uuid = rec.ok_or(sqlx::Error::RowNotFound)?.get("id");
    sqlx::query_as::<_, Deployment>("INSERT INTO deployments (app_id, artifact_url, status) VALUES ($1,$2,$3) RETURNING id, app_id, artifact_url, status, created_at")
        .bind(app_id)
        .bind(artifact_url)
        .bind("pending")
        .fetch_one(pool).await
}

pub async fn list_deployments(pool: &Pool<Postgres>) -> Result<Vec<Deployment>, sqlx::Error> {
    sqlx::query_as::<_, Deployment>("SELECT id, app_id, artifact_url, status, created_at FROM deployments ORDER BY created_at DESC")
        .fetch_all(pool).await
}
