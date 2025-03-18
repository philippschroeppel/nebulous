// src/handlers/containers.rs

use crate::container::factory::platform_factory;
use crate::models::{
    V1Container, V1ContainerList, V1ContainerRequest, V1ResourceMeta, V1UserProfile,
};

// Adjust the crate paths below to match your own project structure:
use crate::mutation::Mutation;
use crate::query::Query;
use crate::state::AppState;

use axum::extract::Query as QueryExtractor;
use axum::{
    extract::Extension, extract::Json, extract::Path, extract::State, http::StatusCode,
    response::IntoResponse,
};
use petname::Generator;
use sea_orm::*;
use serde_json::json;

pub async fn get_container(
    State(state): State<AppState>,
    Extension(user_profile): Extension<V1UserProfile>,
    Path(id): Path<String>,
) -> Result<Json<V1Container>, (StatusCode, Json<serde_json::Value>)> {
    let db_pool = &state.db_pool;

    let mut owner_ids: Vec<String> = if let Some(orgs) = &user_profile.organizations {
        orgs.keys().cloned().collect()
    } else {
        Vec::new()
    };

    // Include user's email (assuming owner_id is user's email)
    owner_ids.push(user_profile.email.clone());
    let owner_id_refs: Vec<&str> = owner_ids.iter().map(|s| s.as_str()).collect();

    let container = Query::find_container_by_id_and_owners(db_pool, &id, &owner_id_refs)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": format!("Database error: {}", e)})),
            )
        })?;

    let out_container = V1Container {
        kind: "Container".to_string(),
        metadata: V1ResourceMeta {
            name: container.name.clone(),
            namespace: container.namespace.clone(),
            id: container.id.to_string(),
            owner_id: container.owner_id,
            created_at: container.created_at.timestamp(),
            updated_at: container.updated_at.timestamp(),
            created_by: container.created_by.unwrap_or_default(),
            labels: container
                .labels
                .and_then(|v| serde_json::from_value(v).ok())
                .unwrap_or_default(),
        },
        image: container.image.clone(),
        platform: container.platform.unwrap_or_default(),
        env_vars: container
            .env_vars
            .and_then(|v| serde_json::from_value(v).ok())
            .unwrap_or_default(),
        command: container.command.clone(),
        volumes: container
            .volumes
            .and_then(|v| serde_json::from_value(v).ok()),
        accelerators: container.accelerators,
        meters: container
            .meters
            .and_then(|v| serde_json::from_value(v).ok()),
        status: container
            .status
            .and_then(|v| serde_json::from_value(v).ok()),
        restart: container.restart,
        queue: container.queue,
        timeout: container.timeout,
        resources: container
            .resources
            .and_then(|v| serde_json::from_value(v).ok()),
        ssh_keys: container
            .ssh_keys
            .and_then(|v| serde_json::from_value(v).ok()),
    };

    Ok(Json(out_container))
}

#[axum::debug_handler]
pub async fn list_containers(
    State(state): State<AppState>,
    Extension(user_profile): Extension<V1UserProfile>,
) -> Result<Json<V1ContainerList>, (StatusCode, Json<serde_json::Value>)> {
    let db_pool = &state.db_pool;

    let mut owner_ids: Vec<String> = if let Some(orgs) = &user_profile.organizations {
        orgs.keys().cloned().collect()
    } else {
        Vec::new()
    };

    // Include user's email (assuming owner_id is user's email)
    owner_ids.push(user_profile.email.clone());

    let owner_id_refs: Vec<&str> = owner_ids.iter().map(|s| s.as_str()).collect();

    // Query containers for all owner_ids
    let container_models = Query::find_containers_by_owners(db_pool, &owner_id_refs)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": format!("Database error: {}", e)})),
            )
        })?;

    // Convert database models to API response models
    let containers = container_models
        .into_iter()
        .map(|c| V1Container {
            kind: "Container".to_string(),
            metadata: V1ResourceMeta {
                name: c.name,
                namespace: c.namespace,
                id: c.id.to_string(),
                owner_id: c.owner_id,
                created_at: c.created_at.timestamp(),
                updated_at: c.updated_at.timestamp(),
                created_by: c.created_by.unwrap_or_default(),
                labels: c
                    .labels
                    .and_then(|v| serde_json::from_value(v).ok())
                    .unwrap_or_default(),
            },
            image: c.image,
            env_vars: c
                .env_vars
                .and_then(|v| serde_json::from_value(v).ok())
                .unwrap_or_default(),
            command: c.command,
            platform: c.platform.unwrap_or_default(),
            volumes: c.volumes.and_then(|v| serde_json::from_value(v).ok()),
            accelerators: c.accelerators,
            meters: c.meters.and_then(|v| serde_json::from_value(v).ok()),
            status: c.status.and_then(|v| serde_json::from_value(v).ok()),
            restart: c.restart,
            queue: c.queue,
            timeout: c.timeout,
            resources: c.resources.and_then(|v| serde_json::from_value(v).ok()),
            ssh_keys: c.ssh_keys.and_then(|v| serde_json::from_value(v).ok()),
        })
        .collect();

    Ok(Json(V1ContainerList { containers }))
}

pub async fn create_container(
    State(state): State<AppState>,
    Extension(user_profile): Extension<V1UserProfile>,
    Json(container_request): Json<V1ContainerRequest>,
) -> Result<Json<V1Container>, (StatusCode, Json<serde_json::Value>)> {
    let db_pool = &state.db_pool;

    let platform = platform_factory(
        container_request
            .clone()
            .platform
            .unwrap_or("runpod".to_string()),
    );
    let container = platform
        .declare(
            &container_request,
            db_pool,
            &user_profile,
            &user_profile.email,
        )
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": e.to_string()})),
            )
        })?;

    Ok(Json(container))
}

pub async fn delete_container(
    State(state): State<AppState>,
    Extension(user_profile): Extension<V1UserProfile>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, (StatusCode, Json<serde_json::Value>)> {
    let db_pool = &state.db_pool;

    let mut owner_ids: Vec<String> = if let Some(orgs) = &user_profile.organizations {
        orgs.keys().cloned().collect()
    } else {
        Vec::new()
    };

    // Include user's email (assuming owner_id is user's email)
    owner_ids.push(user_profile.email.clone());
    let owner_id_refs: Vec<&str> = owner_ids.iter().map(|s| s.as_str()).collect();

    let container = Query::find_container_by_id_and_owners(db_pool, &id, &owner_id_refs)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": format!("Database error: {}", e)})),
            )
        })?;

    // Check if user has permission to delete this container
    let owner_id = container.owner_id.clone();

    let platform = platform_factory(container.platform.unwrap().clone());

    platform
        .delete(&container.id.to_string(), db_pool)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": format!("Failed to delete container: {}", e)})),
            )
        })?;

    // Delete the container
    Mutation::delete_container(db_pool, id).await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": format!("Failed to delete container: {}", e)})),
        )
    })?;

    // Return just a 200 OK status code
    Ok(StatusCode::OK)
}

pub async fn fetch_container_logs(
    State(state): State<AppState>,
    Extension(user_profile): Extension<V1UserProfile>,
    Path(id): Path<String>,
) -> Result<Json<String>, (StatusCode, Json<serde_json::Value>)> {
    let db_pool = &state.db_pool;

    // Collect owner IDs from user_profile to use in your `Query` call
    let mut owner_ids: Vec<String> = user_profile
        .organizations
        .as_ref()
        .map(|orgs| orgs.keys().cloned().collect())
        .unwrap_or_default();

    // Add user email if necessary for ownership checks
    owner_ids.push(user_profile.email.clone());
    let owner_id_refs: Vec<&str> = owner_ids.iter().map(|s| s.as_str()).collect();

    // Find the container in the DB, ensuring the user has permission
    let container = Query::find_container_by_id_and_owners(db_pool, &id, &owner_id_refs)
        .await
        .map_err(|err| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": format!("Database error: {}", err) })),
            )
        })?;

    let platform = platform_factory(container.platform.unwrap().clone());

    // Use the helper function to fetch logs
    let logs = platform
        .logs(&container.id.to_string(), db_pool)
        .await
        .map_err(|err| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": format!("Failed to get logs: {}", err) })),
            )
        })?;

    Ok(Json(logs))
}
