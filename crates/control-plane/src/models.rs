use serde::{Serialize, Deserialize};
use uuid::Uuid;
use chrono::{DateTime, Utc};

#[derive(sqlx::FromRow, Serialize, Deserialize, Debug, Clone)]
pub struct Application { pub id: Uuid, pub name: String, pub created_at: DateTime<Utc>, pub updated_at: DateTime<Utc> }

#[derive(sqlx::FromRow, Serialize, Deserialize, Debug, Clone)]
pub struct Deployment { pub id: Uuid, pub app_id: Uuid, pub artifact_url: String, pub status: String, pub created_at: DateTime<Utc> }
