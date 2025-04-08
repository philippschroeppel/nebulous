// src/handlers/auth.rs

use crate::models::V1UserProfile;
use axum::{extract::Extension, extract::Json, http::StatusCode};

pub async fn get_user_profile(
    Extension(user_profile): Extension<V1UserProfile>,
) -> Result<Json<V1UserProfile>, (StatusCode, Json<serde_json::Value>)> {
    Ok(Json(user_profile))
}
