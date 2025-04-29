use crate::agent::ns::auth_ns;
use crate::models::V1ResourceMeta;
use crate::resources::v1::secrets::models::{V1Secret, V1SecretRequest};
use crate::utils::namespace::resolve_namespace;
use crate::{
    entities::secrets, models::V1UserProfile, mutation::Mutation, query::Query, state::AppState,
};
use axum::{
    extract::{Extension, Json, Path, State},
    http::StatusCode,
    response::IntoResponse,
};
use sea_orm::*;
use serde_json::json;
use short_uuid::ShortUuid;
use tracing::{debug, info};

/// Handler: List secrets for the current user (and their organizations)
pub async fn list_secrets(
    State(state): State<AppState>,
    Extension(user_profile): Extension<V1UserProfile>,
) -> Result<Json<Vec<V1Secret>>, (StatusCode, Json<serde_json::Value>)> {
    let db_pool = &state.db_pool;

    // Gather all possible owner IDs from user + organizations
    let mut owner_ids: Vec<String> = user_profile
        .organizations
        .as_ref()
        .map(|orgs| orgs.keys().cloned().collect())
        .unwrap_or_default();
    owner_ids.push(user_profile.email.clone());
    let owner_id_refs: Vec<&str> = owner_ids.iter().map(|s| s.as_str()).collect();

    info!("Listing secrets for user: {}", owner_ids.join(", "));

    // Retrieve secrets
    let secrets_list = Query::find_secrets_by_owners(db_pool, &owner_id_refs)
        .await
        .map_err(|err| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": format!("Database error: {}", err) })),
            )
        })?;

    info!("Found {} secrets", secrets_list.len());

    // Transform them into our V1Secret response (decrypted if needed)
    let response = secrets_list
        .into_iter()
        .map(|secret| {
            let decrypted_value = secret.decrypt_value().ok(); // returns Result; ignore errors
            V1Secret {
                kind: "Secret".to_string(),
                metadata: V1ResourceMeta {
                    id: secret.id,
                    name: secret.name,
                    namespace: secret.namespace,
                    owner: secret.owner,
                    owner_ref: secret.owner_ref,
                    created_by: secret.created_by.unwrap_or_default(),
                    labels: secret
                        .labels
                        .as_ref()
                        .and_then(|v| serde_json::from_value(v.clone()).ok())
                        .unwrap_or_default(),
                    created_at: secret.created_at.timestamp(),
                    updated_at: secret.updated_at.timestamp(),
                },
                value: decrypted_value,
                expires_at: secret.expires_at,
            }
        })
        .collect();

    debug!("Found secrets response: {:?}", response);
    Ok(Json(response))
}

/// Handler: Get a single secret by namespace and name
pub async fn get_secret(
    State(state): State<AppState>,
    Extension(user_profile): Extension<V1UserProfile>,
    Path((namespace, name)): Path<(String, String)>,
) -> Result<Json<V1Secret>, (StatusCode, Json<serde_json::Value>)> {
    let db_pool = &state.db_pool;
    let resolved_namespace = resolve_namespace(&namespace, &user_profile);

    // Attempt to retrieve the secret from the DB
    let secret = Query::find_secret_by_namespace_and_name(db_pool, &resolved_namespace, &name)
        .await
        .map_err(|err| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": format!("Database error: {}", err) })),
            )
        })?;

    // If the secret is `None`, return a 404; otherwise, proceed
    let secret_model = match secret {
        Some(secret_model) => secret_model,
        None => {
            return Err((
                StatusCode::NOT_FOUND,
                Json(json!({ "error": "Secret not found" })),
            ));
        }
    };

    // Decrypt the value if needed
    let decrypted_value = secret_model.decrypt_value().ok();

    // Build and return the V1Secret response
    let secret_response = V1Secret {
        kind: "Secret".to_string(),
        metadata: V1ResourceMeta {
            id: secret_model.id,
            name: secret_model.name,
            namespace: secret_model.namespace,
            owner: secret_model.owner,
            owner_ref: secret_model.owner_ref,
            created_by: secret_model.created_by.unwrap_or_default(),
            labels: secret_model
                .labels
                .as_ref()
                .and_then(|v| serde_json::from_value(v.clone()).ok())
                .unwrap_or_default(),
            created_at: secret_model.created_at.timestamp(),
            updated_at: secret_model.updated_at.timestamp(),
        },
        value: decrypted_value,
        expires_at: secret_model.expires_at,
    };

    Ok(Json(secret_response))
}

pub async fn get_secret_by_id(
    State(state): State<AppState>,
    Extension(user_profile): Extension<V1UserProfile>,
    Path(id): Path<String>,
) -> Result<Json<V1Secret>, (StatusCode, Json<serde_json::Value>)> {
    let db_pool = &state.db_pool;

    _get_secret_by_id(db_pool, &id, &user_profile).await
}

pub async fn _get_secret_by_id(
    db_pool: &DatabaseConnection,
    id: &str,
    user_profile: &V1UserProfile,
) -> Result<Json<V1Secret>, (StatusCode, Json<serde_json::Value>)> {
    // Gather owners
    let mut owner_ids: Vec<String> = user_profile
        .organizations
        .as_ref()
        .map(|orgs| orgs.keys().cloned().collect())
        .unwrap_or_default();
    owner_ids.push(user_profile.email.clone());
    let owner_id_refs: Vec<&str> = owner_ids.iter().map(|s| s.as_str()).collect();

    info!("Getting secret for user: {}", owner_ids.join(", "));

    // Fetch secret from DB
    let secret_model = Query::find_secret_by_id_and_owners(db_pool, &id, &owner_id_refs)
        .await
        .map_err(|err| {
            (
                StatusCode::NOT_FOUND,
                Json(json!({ "error": format!("Secret not found: {}", err) })),
            )
        })?;

    info!("Found secret: {}", secret_model.id);

    // Decrypt
    let decrypted_value = secret_model.decrypt_value().ok();

    let secret_response = V1Secret {
        kind: "Secret".to_string(),
        metadata: V1ResourceMeta {
            id: secret_model.id,
            name: secret_model.name,
            namespace: secret_model.namespace,
            owner: secret_model.owner,
            owner_ref: secret_model.owner_ref,
            created_by: secret_model.created_by.unwrap_or_default(),
            labels: secret_model
                .labels
                .as_ref()
                .and_then(|v| serde_json::from_value(v.clone()).ok())
                .unwrap_or_default(),
            created_at: secret_model.created_at.timestamp(),
            updated_at: secret_model.updated_at.timestamp(),
        },
        value: decrypted_value,
        expires_at: secret_model.expires_at,
    };

    Ok(Json(secret_response))
}

/// Handler: Create a new secret
pub async fn create_secret(
    State(state): State<AppState>,
    Extension(user_profile): Extension<V1UserProfile>,
    Json(payload): Json<V1SecretRequest>,
) -> Result<Json<V1Secret>, (StatusCode, Json<serde_json::Value>)> {
    let db_pool = &state.db_pool;

    // Generate a unique ID for the secret. You might use `short_uuid`, etc.
    let secret_id = ShortUuid::generate().to_string();

    // If the name is None, we generate a petname. Otherwise, use the provided name.
    let name = payload
        .metadata
        .name
        .clone()
        .unwrap_or_else(|| petname::petname(2, "-").unwrap());

    crate::validate::validate_name(&name).map_err(|err| {
        (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": format!("Invalid name: {}", err) })),
        )
    })?;

    let namespace_opt = payload.metadata.namespace;

    let handle = match user_profile.handle.clone() {
        Some(handle) => handle,
        None => user_profile
            .email
            .clone()
            .replace("@", "-")
            .replace(".", "-"),
    };

    let namespace = match namespace_opt {
        Some(namespace) => namespace,
        None => match crate::handlers::v1::namespaces::ensure_namespace(
            db_pool,
            &handle,
            &user_profile.email,
            &user_profile.email,
            None,
        )
        .await
        {
            Ok(_) => handle,
            Err(e) => {
                return Err((
                    StatusCode::BAD_REQUEST,
                    Json(json!({ "error": format!("Invalid namespace: {}", e) })),
                ));
            }
        },
    };

    crate::validate::validate_namespace(&namespace).map_err(|err| {
        (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": format!("Invalid namespace: {}", err) })),
        )
    })?;

    let mut owner_ids: Vec<String> = if let Some(orgs) = &user_profile.organizations {
        orgs.keys().cloned().collect()
    } else {
        Vec::new()
    };
    owner_ids.push(user_profile.email.clone());

    let owner = auth_ns(db_pool, &owner_ids, &namespace)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": format!("Authorization error: {}", e)})),
            )
        })?;

    // Create the new Model, which will auto-encrypt the secret value
    let secret_model = secrets::Model::new(
        secret_id.clone(),
        name,
        namespace,
        owner,
        &payload.value,
        Some(user_profile.email.clone()),
        Some(serde_json::json!(payload.metadata.labels)),
        payload.expires_at,
    )
    .map_err(|err| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": format!("Failed to encrypt secret: {}", err) })),
        )
    })?;

    // Insert into DB
    let inserted = secrets::ActiveModel::from(secret_model)
        .insert(db_pool)
        .await
        .map_err(|err| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": format!("Failed to store secret: {}", err) })),
            )
        })?;

    let created_by = inserted.created_by.clone().unwrap_or_default();
    let labels = inserted
        .labels
        .as_ref()
        .and_then(|v| serde_json::from_value(v.clone()).ok())
        .unwrap_or_default();

    // Now decrypt using `inserted`, leaving the prior fields intact
    let decrypted_value = inserted.decrypt_value().ok();

    // Finally build the response
    let response = V1Secret {
        kind: "Secret".to_string(),
        metadata: V1ResourceMeta {
            id: inserted.id,
            name: inserted.name,
            namespace: inserted.namespace,
            owner: inserted.owner,
            owner_ref: inserted.owner_ref,
            created_by,
            labels,
            created_at: inserted.created_at.timestamp(),
            updated_at: inserted.updated_at.timestamp(),
        },
        value: decrypted_value,
        expires_at: inserted.expires_at,
    };

    Ok(Json(response))
}

/// Handler: Update a secret by namespace/name
pub async fn update_secret(
    State(state): State<AppState>,
    Extension(user_profile): Extension<V1UserProfile>,
    Path((namespace, name)): Path<(String, String)>,
    Json(payload): Json<V1SecretRequest>,
) -> Result<Json<V1Secret>, (StatusCode, Json<serde_json::Value>)> {
    let db_pool = &state.db_pool;
    let resolved_namespace = resolve_namespace(&namespace, &user_profile);

    // 1) Fetch secret by namespace+name
    let secret_opt = Query::find_secret_by_namespace_and_name(db_pool, &resolved_namespace, &name)
        .await
        .map_err(|err| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": format!("Database error: {}", err)})),
            )
        })?;

    // 2) If not found => 404
    let secret_model = match secret_opt {
        Some(s) => s,
        None => {
            return Err((
                StatusCode::NOT_FOUND,
                Json(json!({"error": "Secret not found"})),
            ));
        }
    };

    // 3) Call the shared helper, passing the existing secret's ID
    _update_secret_by_id(db_pool, &secret_model.id, &user_profile, &payload).await
}

/// Handler: Update a secret by ID
pub async fn update_secret_by_id(
    State(state): State<AppState>,
    Extension(user_profile): Extension<V1UserProfile>,
    Path(secret_id): Path<String>,
    Json(payload): Json<V1SecretRequest>,
) -> Result<Json<V1Secret>, (StatusCode, Json<serde_json::Value>)> {
    let db_pool = &state.db_pool;
    _update_secret_by_id(db_pool, &secret_id, &user_profile, &payload).await
}

pub async fn _update_secret_by_id(
    db_pool: &DatabaseConnection,
    secret_id: &str,
    user_profile: &V1UserProfile,
    payload: &V1SecretRequest,
) -> Result<Json<V1Secret>, (StatusCode, Json<serde_json::Value>)> {
    // Gather owners
    let mut owner_ids: Vec<String> = user_profile
        .organizations
        .as_ref()
        .map(|orgs| orgs.keys().cloned().collect())
        .unwrap_or_default();
    owner_ids.push(user_profile.email.clone());
    let owner_id_refs: Vec<&str> = owner_ids.iter().map(|s| s.as_str()).collect();

    // Fetch the model to ensure it exists and user can access
    let existing_secret = Query::find_secret_by_id_and_owners(db_pool, &secret_id, &owner_id_refs)
        .await
        .map_err(|err| {
            (
                StatusCode::NOT_FOUND,
                Json(json!({ "error": format!("Secret not found: {}", err) })),
            )
        })?;

    // Perform the update
    let updated_secret = Mutation::update_secret(
        db_pool,
        existing_secret,
        payload.metadata.name.clone(),
        // Provide new_value if you want to re-encrypt. If you want partial updates, handle Option.
        Some(payload.value.clone()),
        Some(json!(payload.metadata.labels)),
    )
    .await
    .map_err(|err| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": format!("Failed to update secret: {}", err) })),
        )
    })?;

    // Decrypt the newly updated secret
    let decrypted_value = updated_secret.decrypt_value().ok();

    // Build response
    let response = V1Secret {
        kind: "Secret".to_string(),
        metadata: V1ResourceMeta {
            id: updated_secret.id,
            name: updated_secret.name,
            namespace: updated_secret.namespace,
            owner: updated_secret.owner,
            owner_ref: updated_secret.owner_ref,
            created_by: updated_secret.created_by.unwrap_or_default(),
            labels: updated_secret
                .labels
                .as_ref()
                .and_then(|v| serde_json::from_value(v.clone()).ok())
                .unwrap_or_default(),
            created_at: updated_secret.created_at.timestamp(),
            updated_at: updated_secret.updated_at.timestamp(),
        },
        value: decrypted_value,
        expires_at: updated_secret.expires_at,
    };

    Ok(Json(response))
}

/// Handler: Delete a secret by namespace/name
pub async fn delete_secret(
    State(state): State<AppState>,
    Extension(user_profile): Extension<V1UserProfile>,
    Path((namespace, name)): Path<(String, String)>,
) -> Result<impl IntoResponse, (StatusCode, Json<serde_json::Value>)> {
    let db_pool = &state.db_pool;
    let resolved_namespace = resolve_namespace(&namespace, &user_profile);

    // 1) Look up secret by namespace + name
    let secret_opt = Query::find_secret_by_namespace_and_name(db_pool, &resolved_namespace, &name)
        .await
        .map_err(|err| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": format!("Database error: {}", err) })),
            )
        })?;

    // 2) If None => 404
    let secret_model = match secret_opt {
        Some(s) => s,
        None => {
            return Err((
                StatusCode::NOT_FOUND,
                Json(json!({ "error": "Secret not found" })),
            ));
        }
    };

    // 3) Call the shared helper to delete by ID
    _delete_secret_by_id(db_pool, &secret_model.id, &user_profile).await
}

/// Handler: Delete a secret by ID
pub async fn delete_secret_by_id(
    State(state): State<AppState>,
    Extension(user_profile): Extension<V1UserProfile>,
    Path(secret_id): Path<String>,
) -> Result<impl IntoResponse, (StatusCode, Json<serde_json::Value>)> {
    let db_pool = &state.db_pool;
    // Just call our shared helper directly
    _delete_secret_by_id(db_pool, &secret_id, &user_profile).await
}

/// Handler: Delete a secret
pub async fn _delete_secret_by_id(
    db_pool: &DatabaseConnection,
    secret_id: &str,
    user_profile: &V1UserProfile,
) -> Result<impl IntoResponse, (StatusCode, Json<serde_json::Value>)> {
    // Gather owners
    let mut owner_ids: Vec<String> = user_profile
        .organizations
        .as_ref()
        .map(|orgs| orgs.keys().cloned().collect())
        .unwrap_or_default();
    owner_ids.push(user_profile.email.clone());
    let owner_id_refs: Vec<&str> = owner_ids.iter().map(|s| s.as_str()).collect();

    // Make sure the secret is accessible
    let _ = Query::find_secret_by_id_and_owners(db_pool, &secret_id, &owner_id_refs)
        .await
        .map_err(|err| {
            (
                StatusCode::NOT_FOUND,
                Json(json!({ "error": format!("Secret not found: {}", err) })),
            )
        })?;

    // Actually delete
    let result = Mutation::delete_secret(db_pool, secret_id.to_string())
        .await
        .map_err(|err| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": format!("Failed to delete secret: {}", err)})),
            )
        })?;

    if result.rows_affected == 0 {
        // No rows were deleted
        return Err((
            StatusCode::NOT_FOUND,
            Json(json!({"error": "Secret not found or already deleted"})),
        ));
    }

    // Return a 200 OK
    Ok(StatusCode::OK)
}
