use crate::auth::db;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ApiKey {
    pub id: String,
    pub key: String,
    pub created_at: String,
    pub last_used_at: Option<String>,
    pub revoked_at: Option<String>,
    pub is_active: bool,
}

impl ApiKey {
    pub fn new(id: String, key: String) -> Self {
        Self {
            id,
            key,
            created_at: chrono::Utc::now().to_string(),
            last_used_at: None,
            revoked_at: None,
            is_active: true,
        }
    }

    pub fn access(&mut self) {
        self.last_used_at = Some(chrono::Utc::now().to_string());
    }

    pub fn revoke(&mut self) {
        self.revoked_at = Some(chrono::Utc::now().to_string());
        self.is_active = false;
    }
}

impl From<db::Model> for ApiKey {
    fn from(model: db::Model) -> Self {
        Self {
            id: model.id,
            key: model.key,
            created_at: model.created_at.to_string(),
            last_used_at: model.last_used_at.map(|dt| dt.to_string()),
            revoked_at: model.revoked_at.clone().map(|dt| dt.to_string()),
            is_active: model.revoked_at.is_none(),
        }
    }
}

impl From<ApiKey> for db::Model {
    fn from(api_key: ApiKey) -> Self {
        Self {
            id: api_key.id,
            key: api_key.key,
            created_at: chrono::DateTime::parse_from_rfc3339(&api_key.created_at).unwrap(),
            last_used_at: api_key
                .last_used_at
                .map(|dt| chrono::DateTime::parse_from_rfc3339(&dt).unwrap()),
            revoked_at: api_key
                .revoked_at
                .map(|dt| chrono::DateTime::parse_from_rfc3339(&dt).unwrap()),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct SanitizedApiKey {
    pub id: String,
    pub created_at: String,
    pub last_used_at: Option<String>,
    pub revoked_at: Option<String>,
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
