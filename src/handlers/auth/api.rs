use crate::auth;
use crate::auth::models::SanitizedApiKey;
use crate::models::V1UserProfile;
use crate::state::AppState;
use axum::extract::{Json, Path, State};
use axum::http::StatusCode;
use axum::Extension;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
pub struct ApiKeyRequest {
    pub id: String,
}

#[derive(Serialize, Deserialize)]
pub struct RawApiKeyResponse {
    pub api_key: String,
}

impl RawApiKeyResponse {
    pub fn new(api_key: String) -> Self {
        Self { api_key }
    }
}

#[derive(Serialize, Deserialize)]
pub struct ApiKeyListResponse {
    pub api_keys: Vec<SanitizedApiKey>,
}

pub async fn get_api_key(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<SanitizedApiKey>, (StatusCode, Json<serde_json::Value>)> {
    match auth::api::get_sanitized_api_key(&state.db_pool, &id).await {
        Ok(api_key) => Ok(Json(api_key)),
        Err(_) => Err((
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "API key not found"})),
        )),
    }
}

pub async fn list_api_keys(
    State(state): State<AppState>,
) -> Result<Json<ApiKeyListResponse>, (StatusCode, Json<serde_json::Value>)> {
    match auth::api::list_api_keys(&state.db_pool).await {
        Ok(api_keys) => Ok(Json(ApiKeyListResponse { api_keys })),
        Err(_) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": "Failed to list API keys"})),
        )),
    }
}

pub async fn generate_api_key(
    State(state): State<AppState>,
) -> Result<Json<RawApiKeyResponse>, (StatusCode, Json<serde_json::Value>)> {
    match auth::api::generate_api_key(&state.db_pool).await {
        Ok(api_key) => Ok(Json(RawApiKeyResponse::new(api_key))),
        Err(_) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": "Failed to generate API key"})),
        )),
    }
}

pub async fn revoke_api_key(
    State(state): State<AppState>,
    Json(api_key): Json<ApiKeyRequest>,
) -> Result<Json<SanitizedApiKey>, (StatusCode, Json<serde_json::Value>)> {
    match auth::api::revoke_api_key(&state.db_pool, &api_key.id).await {
        Ok(api_key) => Ok(Json(SanitizedApiKey::from(api_key))),
        Err(_) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": "Failed to revoke API key"})),
        )),
    }
}
