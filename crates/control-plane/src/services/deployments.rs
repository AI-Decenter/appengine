use sqlx::{Pool, Postgres, Row};
use crate::models::Deployment;

/// List deployments for an application.
/// Returns `sqlx::Error::RowNotFound` if the application does not exist.
/// (Previously this returned Ok(Vec::new()) which forced the caller to run a
/// secondary existence query and introduced a race where another test deleting
/// the app between queries caused a spurious 404.)
pub async fn list_for_app(pool: &Pool<Postgres>, app_name: &str, limit: i64, offset: i64) -> Result<Vec<Deployment>, sqlx::Error> {
    let app_row = sqlx::query("SELECT id FROM applications WHERE name = $1")
        .bind(app_name)
        .fetch_optional(pool)
        .await?;
    let app_id: uuid::Uuid = match app_row { Some(r) => r.get("id"), None => return Err(sqlx::Error::RowNotFound) };
    let rows = sqlx::query_as::<_, Deployment>("SELECT id, app_id, artifact_url, status, created_at, digest, failure_reason FROM deployments WHERE app_id = $1 ORDER BY created_at DESC LIMIT $2 OFFSET $3")
        .bind(app_id)
        .bind(limit)
        .bind(offset)
        .fetch_all(pool)
        .await?;
    Ok(rows)
}

pub async fn create_deployment(pool: &Pool<Postgres>, app_name: &str, artifact_url: &str, digest: Option<&str>) -> Result<Deployment, sqlx::Error> {
    let rec = sqlx::query("SELECT id FROM applications WHERE name = $1")
        .bind(app_name).fetch_optional(pool).await?;
    let app_id: uuid::Uuid = rec.ok_or(sqlx::Error::RowNotFound)?.get("id");
    sqlx::query_as::<_, Deployment>("INSERT INTO deployments (app_id, artifact_url, status, digest) VALUES ($1,$2,$3,$4) RETURNING id, app_id, artifact_url, status, created_at, digest, failure_reason")
        .bind(app_id)
        .bind(artifact_url)
        .bind("pending")
        .bind(digest)
        .fetch_one(pool).await
}

pub async fn list_deployments(pool: &Pool<Postgres>) -> Result<Vec<Deployment>, sqlx::Error> {
    sqlx::query_as::<_, Deployment>("SELECT id, app_id, artifact_url, status, created_at, digest, failure_reason FROM deployments ORDER BY created_at DESC")
        .fetch_all(pool).await
}

pub async fn get_deployment(pool: &Pool<Postgres>, id: uuid::Uuid) -> Result<Deployment, sqlx::Error> {
    sqlx::query_as::<_, Deployment>("SELECT id, app_id, artifact_url, status, created_at, digest, failure_reason FROM deployments WHERE id=$1")
        .bind(id)
        .fetch_one(pool).await
}

pub async fn mark_running(pool: &Pool<Postgres>, id: uuid::Uuid) {
    let _ = sqlx::query("UPDATE deployments SET status='running', failure_reason=NULL WHERE id=$1")
        .bind(id)
        .execute(pool).await;
    let _ = sqlx::query("INSERT INTO deployment_events (deployment_id, event_type, message) VALUES ($1,'running',NULL)")
        .bind(id)
        .execute(pool).await;
}

pub async fn mark_failed(pool: &Pool<Postgres>, id: uuid::Uuid, reason: &str) {
    let _ = sqlx::query("UPDATE deployments SET status='failed', failure_reason=$2 WHERE id=$1")
        .bind(id)
        .bind(reason)
        .execute(pool).await;
    let _ = sqlx::query("INSERT INTO deployment_events (deployment_id, event_type, message) VALUES ($1,'failed',$2)")
        .bind(id)
        .bind(reason)
        .execute(pool).await;
}
