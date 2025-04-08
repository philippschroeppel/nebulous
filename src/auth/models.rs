use crate::auth::db;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ApiKey {
    pub id: String,
    pub hash: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub last_used_at: Option<chrono::DateTime<chrono::Utc>>,
    pub revoked_at: Option<chrono::DateTime<chrono::Utc>>,
    pub is_active: bool,
}

impl ApiKey {
    pub fn new(id: String, hash: String) -> Self {
        Self {
            id,
            hash,
            created_at: chrono::Utc::now(),
            last_used_at: None,
            revoked_at: None,
            is_active: true,
        }
    }
}

impl From<db::Model> for ApiKey {
    fn from(model: db::Model) -> Self {
        Self {
            id: model.id,
            hash: model.hash,
            created_at: model.created_at,
            last_used_at: model.last_used_at,
            revoked_at: model.revoked_at,
            is_active: model.revoked_at.is_none(),
        }
    }
}

impl From<ApiKey> for db::Model {
    fn from(api_key: ApiKey) -> Self {
        Self {
            id: api_key.id,
            hash: api_key.hash,
            created_at: api_key.created_at,
            last_used_at: api_key.last_used_at,
            revoked_at: api_key.revoked_at,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct SanitizedApiKey {
    pub id: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub last_used_at: Option<chrono::DateTime<chrono::Utc>>,
    pub revoked_at: Option<chrono::DateTime<chrono::Utc>>,
    pub is_active: bool,
}

impl From<ApiKey> for SanitizedApiKey {
    fn from(api_key: ApiKey) -> Self {
        Self {
            id: api_key.id,
            created_at: api_key.created_at,
            last_used_at: api_key.last_used_at,
            revoked_at: api_key.revoked_at,
            is_active: api_key.is_active,
        }
    }
}
