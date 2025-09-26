use serde::{Serialize, Deserialize};
use uuid::Uuid;
use chrono::{DateTime, Utc};

#[derive(Serialize, Deserialize, Debug)]
pub struct Application { pub id: Uuid, pub name: String, pub created_at: DateTime<Utc>, pub updated_at: DateTime<Utc> }

#[derive(Serialize, Deserialize, Debug)]
pub struct Deployment { pub id: Uuid, pub app_id: Uuid, pub status: String, pub created_at: DateTime<Utc> }
