// src/handlers/containers.rs

use crate::agent::ns::auth_ns;
use crate::models::V1ResourceMeta;
use crate::resources::v1::volumes::models::{V1Volume, V1VolumeRequest};
use crate::utils::namespace::resolve_namespace;
use crate::{
    entities::volumes::{self, ActiveModel as VolumeActiveModel},
    models::V1UserProfile,
    mutation::Mutation,
    query::Query,
    state::AppState,
};
use axum::{
    extract::{Extension, Json, Path, State},
    http::StatusCode,
    response::IntoResponse,
};
use chrono;
use sea_orm::DbErr;
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter,
};
use serde_json::json;
use short_uuid;

pub async fn get_volume(
    State(state): State<AppState>,
    Extension(user_profile): Extension<V1UserProfile>,
    Path((namespace, name)): Path<(String, String)>,
) -> Result<Json<V1Volume>, (StatusCode, Json<serde_json::Value>)> {
    let db_pool = &state.db_pool;
    let resolved_namespace = resolve_namespace(&namespace, &user_profile);

    let mut owner_ids: Vec<String> = if let Some(orgs) = &user_profile.organizations {
        orgs.keys().cloned().collect()
    } else {
        Vec::new()
    };

    // Include user's email (assuming owner_id is user's email)
    owner_ids.push(user_profile.email.clone());
    let owner_id_refs: Vec<&str> = owner_ids.iter().map(|s| s.as_str()).collect();

    let volume = Query::find_volume_by_namespace_name_and_owners(
        db_pool,
        &resolved_namespace,
        &name,
        &owner_id_refs,
    )
    .await
    .map_err(|err| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": format!("Database error: {}", err)})),
        )
    })?;

    Ok(Json(volume.to_v1()))
}

pub async fn create_volume(
    State(state): State<AppState>,
    Extension(user_profile): Extension<V1UserProfile>,
    Json(volume): Json<V1VolumeRequest>,
) -> Result<Json<V1Volume>, (StatusCode, Json<serde_json::Value>)> {
    let db_pool = &state.db_pool;

    // Get owner IDs from organizations and email
    let mut owner_ids: Vec<String> = if let Some(orgs) = &user_profile.organizations {
        orgs.keys().cloned().collect()
    } else {
        Vec::new()
    };
    owner_ids.push(user_profile.email.clone());
    let owner_id_refs: Vec<&str> = owner_ids.iter().map(|s| s.as_str()).collect();

    let namespace_opt = volume.clone().metadata.namespace;

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

    let name = volume
        .metadata
        .name
        .clone()
        .unwrap_or_else(|| petname::petname(2, "-").unwrap());

    let owner = auth_ns(db_pool, &owner_ids, &namespace)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": format!("Authorization error: {}", e)})),
            )
        })?;

    // Check if a volume with the same namespace and name already exists
    let existing_volume =
        Query::find_volume_by_namespace_name_and_owners(db_pool, &namespace, &name, &owner_id_refs)
            .await;

    if let Ok(_) = existing_volume {
        return Err((
            StatusCode::CONFLICT,
            Json(json!({
                "error": format!(
                    "Volume with namespace '{}' and name '{:?}' already exists",
                    namespace, name
                )
            })),
        ));
    }

    // Generate a unique ID for the volume
    let id = short_uuid::ShortUuid::generate().to_string();
    let now = chrono::Utc::now().into();

    // Create the volume entity directly as an ActiveModel
    let volume_entity = VolumeActiveModel {
        id: Set(id),
        name: Set(name.clone()),
        namespace: Set(namespace.clone()),
        full_name: Set(format!("{namespace}/{name}")),
        owner: Set(owner),
        owner_ref: Set(None),
        source: Set(volume.source.clone()),
        labels: Set(volume
            .metadata
            .labels
            .as_ref()
            .map(|labels| serde_json::to_value(labels).unwrap_or_default())),
        created_by: Set(user_profile.email.clone()),
        updated_at: Set(now),
        created_at: Set(now),
    };

    // Insert the volume into the database
    let volume_entity = volume_entity.insert(db_pool).await.map_err(|err| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": format!("Database error: {}", err)})),
        )
    })?;

    // Convert the entity back to V1Volume and return it
    Ok(Json(volume_entity.to_v1()))
}

pub async fn delete_volume(
    State(state): State<AppState>,
    Extension(user_profile): Extension<V1UserProfile>,
    Path((namespace, name)): Path<(String, String)>,
) -> Result<impl IntoResponse, (StatusCode, Json<serde_json::Value>)> {
    let db_pool = &state.db_pool;
    let resolved_namespace = resolve_namespace(&namespace, &user_profile);

    // Collect owner IDs
    let mut owner_ids: Vec<String> = if let Some(orgs) = &user_profile.organizations {
        orgs.keys().cloned().collect()
    } else {
        Vec::new()
    };
    owner_ids.push(user_profile.email.clone());
    let owner_id_refs: Vec<&str> = owner_ids.iter().map(|s| s.as_str()).collect();

    // 1) Look up volume by namespace + name
    let volume = Query::find_volume_by_namespace_name_and_owners(
        db_pool,
        &resolved_namespace,
        &name,
        &owner_id_refs,
    )
    .await
    .map_err(|err| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": format!("Database error: {}", err) })),
        )
    })?;

    // Delete the volume
    volumes::Entity::delete_by_id(volume.id)
        .exec(db_pool)
        .await
        .map_err(|err| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": format!("Failed to delete volume: {}", err)})),
            )
        })?;

    Ok(())
}

/// Internal helper function to ensure a volume exists with the given parameters.
/// Returns the volume if it exists, or creates it if it doesn't.
pub async fn ensure_volume(
    db_pool: &DatabaseConnection,
    namespace: &str,
    name: &str,
    owner: &str,
    source: &str,
    created_by: &str,
    labels: Option<serde_json::Value>,
) -> Result<volumes::Model, DbErr> {
    // First, try to find the volume by namespace and name
    let existing_volume = volumes::Entity::find()
        .filter(volumes::Column::Namespace.eq(namespace))
        .filter(volumes::Column::Name.eq(name))
        .one(db_pool)
        .await?;

    // If the volume exists and has the same source, return it
    if let Some(volume) = existing_volume {
        if volume.source == source {
            return Ok(volume);
        } else {
            return Err(DbErr::Custom(format!(
                "Volume with name '{}' already exists in namespace '{}', but has a different source ('{}' vs '{}'). This is currently not supported.",
                name, namespace, volume.source, source
            )));
        }
    }

    // If we get here, either the volume doesn't exist or has a different source
    // Generate a unique ID for the new volume
    let id = short_uuid::ShortUuid::generate().to_string();

    // Create the volume entity
    let volume_entity = volumes::Model::new(
        id,
        name.to_string(),
        namespace.to_string(),
        owner.to_string(),
        created_by.to_string(),
        labels,
        source.to_string(),
    )
    .map_err(|e| DbErr::Custom(format!("Failed to create volume: {}", e)))?;

    // Insert the volume into the database
    let volume_entity = VolumeActiveModel {
        id: Set(volume_entity.id),
        name: Set(volume_entity.name),
        namespace: Set(volume_entity.namespace),
        full_name: Set(volume_entity.full_name),
        owner: Set(volume_entity.owner),
        owner_ref: Set(volume_entity.owner_ref),
        source: Set(volume_entity.source),
        labels: Set(volume_entity.labels),
        created_by: Set(volume_entity.created_by),
        updated_at: Set(volume_entity.updated_at),
        created_at: Set(volume_entity.created_at),
    };

    let volume_entity = volume_entity.insert(db_pool).await?;

    Ok(volume_entity)
}

/// Handler: List volumes for the current user (and their organizations)
pub async fn list_volumes(
    State(state): State<AppState>,
    Extension(user_profile): Extension<V1UserProfile>,
) -> Result<Json<Vec<V1Volume>>, (StatusCode, Json<serde_json::Value>)> {
    let db_pool = &state.db_pool;

    // Gather all possible owner IDs from user + organizations
    let mut owner_ids: Vec<String> = if let Some(orgs) = &user_profile.organizations {
        orgs.keys().cloned().collect()
    } else {
        Vec::new()
    };
    owner_ids.push(user_profile.email.clone());
    let owner_id_refs: Vec<&str> = owner_ids.iter().map(|s| s.as_str()).collect();

    // Retrieve volumes
    let volumes_list = volumes::Entity::find()
        .filter(volumes::Column::Owner.is_in(owner_id_refs))
        .all(db_pool)
        .await
        .map_err(|err| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": format!("Database error: {}", err) })),
            )
        })?;

    // Transform them into V1Volume responses
    let volumes = volumes_list
        .into_iter()
        .map(|volume| volume.to_v1())
        .collect();

    Ok(Json(volumes))
}

pub async fn update_volume(
    State(state): State<AppState>,
    Extension(user_profile): Extension<V1UserProfile>,
    Path((namespace, name)): Path<(String, String)>,
    Json(payload): Json<V1VolumeRequest>,
) -> Result<Json<V1Volume>, (StatusCode, Json<serde_json::Value>)> {
    let db_pool = &state.db_pool;
    let resolved_namespace = resolve_namespace(&namespace, &user_profile);

    let mut owner_ids: Vec<String> = if let Some(orgs) = &user_profile.organizations {
        orgs.keys().cloned().collect()
    } else {
        Vec::new()
    };
    owner_ids.push(user_profile.email.clone());
    let owner_id_refs: Vec<&str> = owner_ids.iter().map(|s| s.as_str()).collect();

    let volume = Query::find_volume_by_namespace_name_and_owners(
        db_pool,
        &resolved_namespace,
        &name,
        &owner_id_refs,
    )
    .await
    .map_err(|err| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": format!("Database error: {}", err)})),
        )
    })?;

    // Create an ActiveModel from the existing volume
    let mut volume_active_model = volumes::ActiveModel::from(volume);

    // Update fields from the payload
    if let Some(name) = payload.metadata.name {
        volume_active_model.name = sea_orm::ActiveValue::Set(name.clone());
        volume_active_model.full_name =
            sea_orm::ActiveValue::Set(format!("{}/{}", resolved_namespace, name));
    }

    volume_active_model.source = sea_orm::ActiveValue::Set(payload.source);

    if let Some(labels) = payload.metadata.labels {
        volume_active_model.labels =
            sea_orm::ActiveValue::Set(Some(serde_json::to_value(labels).unwrap_or_default()));
    }

    volume_active_model.updated_at = sea_orm::ActiveValue::Set(chrono::Utc::now().into());

    // Save the updated volume to the database
    let updated_volume = volume_active_model.update(db_pool).await.map_err(|err| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": format!("Failed to update volume: {}", err)})),
        )
    })?;

    Ok(Json(updated_volume.to_v1()))
}
