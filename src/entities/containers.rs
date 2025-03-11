// src/entities/training_job.rs

use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};
use serde_json::Value as Json;

#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "containers")]
pub struct Model {
    #[sea_orm(primary_key, column_type = "Text", auto_increment = false)]
    pub id: String,
    pub namespace: String,
    pub name: String,
    pub owner_id: String,
    pub image: String,
    pub env_vars: Option<Json>,
    pub volumes: Option<Json>,
    pub accelerators: Option<Vec<String>>,
    pub cpu_request: Option<String>,
    pub memory_request: Option<String>,
    pub status: Option<String>,
    pub platform: Option<String>,
    pub resource_name: Option<String>,
    pub resource_namespace: Option<String>,
    pub command: Option<String>,
    pub labels: Option<Json>,
    pub created_by: Option<String>,
    pub updated_at: DateTimeWithTimeZone,
    pub created_at: DateTimeWithTimeZone,
}

// The Relation enum is required, even if empty.
#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

// The ActiveModelBehavior is required, even if empty.
impl ActiveModelBehavior for ActiveModel {}
