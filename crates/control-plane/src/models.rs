use serde::{Serialize, Deserialize};
use utoipa::ToSchema;
use uuid::Uuid;
use chrono::{DateTime, Utc};

#[derive(sqlx::FromRow, Serialize, Deserialize, Debug, Clone)]
pub struct Application { pub id: Uuid, pub name: String, pub created_at: DateTime<Utc>, pub updated_at: DateTime<Utc> }

#[derive(sqlx::FromRow, Serialize, Deserialize, Debug, Clone)]
pub struct Deployment { pub id: Uuid, pub app_id: Uuid, pub artifact_url: String, pub status: String, pub created_at: DateTime<Utc> }

#[derive(sqlx::FromRow, Serialize, Deserialize, Debug, Clone, ToSchema)]
pub struct Artifact {
	pub id: Uuid,
	pub app_id: Option<Uuid>,
	pub digest: String,
	pub size_bytes: i64,
	pub signature: Option<String>,
	pub sbom_url: Option<String>,
	pub manifest_url: Option<String>,
	pub verified: bool,
	pub storage_key: Option<String>,
	pub status: String,
	pub created_at: DateTime<Utc>,
	pub completed_at: Option<DateTime<Utc>>,
	pub idempotency_key: Option<String>,
	pub multipart_upload_id: Option<String>,
}
