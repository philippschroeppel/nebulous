use crate::agent::aws::{create_s3_scoped_user, delete_s3_scoped_user, IamCredentials};
use crate::agent::ns::auth_ns;
use crate::config::CONFIG;
use crate::models::{V1ResourceMeta, V1UserProfile};
use crate::state::AppState;
use axum::{
    extract::{Extension, Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde::Serialize;
use serde_json::json;
use tracing::error;

#[derive(Serialize)]
pub struct V1IamCredentialsResponse {
    kind: String,
    metadata: V1ResourceMeta,
    username: String,
    access_key_id: String,
    secret_access_key: String,
    base_key: String,
}

/// Handler: Create a new S3-scoped IAM user for a given namespace and name
pub async fn create_scoped_s3_token(
    State(state): State<AppState>,
    Extension(user_profile): Extension<V1UserProfile>,
    Path((namespace, name)): Path<(String, String)>,
) -> Result<Json<V1IamCredentialsResponse>, (StatusCode, Json<serde_json::Value>)> {
    let db_pool = &state.db_pool;

    // --- Authorization ---
    let mut owner_ids: Vec<String> = if let Some(orgs) = &user_profile.organizations {
        orgs.keys().cloned().collect()
    } else {
        Vec::new()
    };
    owner_ids.push(user_profile.email.clone());

    let owner = match auth_ns(db_pool, &owner_ids, &namespace).await {
        Ok(owner) => owner,
        Err(e) => {
            error!("Authorization failed for namespace {}: {}", namespace, e);
            return Err((
                StatusCode::FORBIDDEN,
                Json(json!({"error": format!("Not authorized for namespace '{}'", namespace)})),
            ));
        }
    };

    // --- Get Bucket Name from Config ---
    // Bucket name is read from global CONFIG at startup
    let bucket_name = CONFIG.bucket_name.clone();

    // --- Call AWS Agent ---
    let credentials = match create_s3_scoped_user(&bucket_name, &namespace, &name).await {
        Ok(creds) => creds,
        Err(e) => {
            error!(
                "Failed to create S3 scoped user '{}/{}': {}",
                namespace, name, e
            );
            // Consider mapping specific AWS errors to different HTTP status codes
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({
                    "error": format!("Failed to create AWS IAM user: {}", e)
                })),
            ));
        }
    };

    // --- Format Response ---
    // We re-use V1ResourceMeta but note some fields aren't directly applicable
    // We'll use the IAM username generated as the 'name' in the metadata
    let response = V1IamCredentialsResponse {
        kind: "IamCredentials".to_string(),
        metadata: V1ResourceMeta {
            id: credentials.username.clone(),
            name: name.clone(),
            namespace: namespace.clone(),
            owner: owner.clone(),
            owner_ref: None,
            created_by: user_profile.email,
            labels: None,
            created_at: chrono::Utc::now().timestamp(),
            updated_at: chrono::Utc::now().timestamp(),
        },
        username: credentials.username.clone(),
        access_key_id: credentials.access_key_id,
        secret_access_key: credentials.secret_access_key,
        base_key: format!("s3://{}/data/{}", bucket_name, namespace),
    };

    Ok(Json(response))
}

/// Handler: Delete an S3-scoped IAM user for a given namespace and name
pub async fn delete_scoped_s3_token(
    State(state): State<AppState>,
    Extension(user_profile): Extension<V1UserProfile>,
    Path((namespace, name)): Path<(String, String)>,
) -> Result<impl IntoResponse, (StatusCode, Json<serde_json::Value>)> {
    let db_pool = &state.db_pool;

    // --- Authorization ---
    let mut owner_ids: Vec<String> = if let Some(orgs) = &user_profile.organizations {
        orgs.keys().cloned().collect()
    } else {
        Vec::new()
    };
    owner_ids.push(user_profile.email.clone());

    match auth_ns(db_pool, &owner_ids, &namespace).await {
        Ok(_) => (),
        Err(e) => {
            error!(
                "Authorization failed for delete request on namespace {}: {}",
                namespace, e
            );
            return Err((
                StatusCode::FORBIDDEN,
                Json(json!({"error": format!("Not authorized for namespace '{}'", namespace)})),
            ));
        }
    };

    // --- Call AWS Agent to Delete ---
    match delete_s3_scoped_user(&namespace, &name).await {
        Ok(_) => {
            // Deletion successful
            Ok(StatusCode::NO_CONTENT)
        }
        Err(e) => {
            error!(
                "Failed to delete S3 scoped user '{}/{}': {}",
                namespace, name, e
            );
            // Consider mapping specific AWS errors (e.g., user not found) if needed
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({
                    "error": format!("Failed to delete AWS IAM user: {}", e)
                })),
            ));
        }
    }
}
