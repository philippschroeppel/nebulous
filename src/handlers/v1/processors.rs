use crate::auth::ns::auth_ns;
use crate::entities::processors;
use crate::models::{
    V1ResourceMeta, V1ResourceMetaRequest, V1StreamData, V1StreamMessage, V1UserProfile,
};
use crate::query::Query;
use crate::resources::v1::containers::models::V1ContainerRequest;
use crate::resources::v1::processors::base::ProcessorPlatform;
use crate::resources::v1::processors::models::{
    V1Processor, V1ProcessorRequest, V1ProcessorScaleRequest, V1Processors, V1UpdateProcessor,
};
use crate::resources::v1::processors::standard::StandardProcessor;
use crate::state::AppState;
use axum::{
    extract::Extension, extract::Json, extract::Path, extract::State, http::StatusCode,
    response::IntoResponse,
};
use sea_orm::{ActiveModelTrait, ActiveValue, DatabaseConnection};
use serde_json::json;
use std::sync::Arc;
use tracing::{debug, error};
use uuid::Uuid;

pub async fn create_processor(
    State(state): State<AppState>,
    Extension(user_profile): Extension<V1UserProfile>,
    Json(processor_request): Json<V1ProcessorRequest>,
) -> Result<Json<V1Processor>, (StatusCode, Json<serde_json::Value>)> {
    let db_pool = &state.db_pool;

    match crate::validate::validate_name(
        &processor_request.clone().metadata.name.unwrap_or_default(),
    ) {
        Ok(_) => (),
        Err(e) => {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(json!({ "error": format!("Invalid name: {}", e) })),
            ));
        }
    }
    debug!("Processor request: {:?}", processor_request);

    let namespace_opt = processor_request.clone().metadata.namespace;

    let handle = match user_profile.handle.clone() {
        Some(handle) => handle,
        None => user_profile
            .email
            .clone()
            .replace("@", "-")
            .replace(".", "-"),
    };
    debug!("Handle: {:?}", handle);

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
    debug!(">> Using namespace for processor creation: {:?}", namespace);

    crate::validate::validate_namespace(&namespace).map_err(|err| {
        (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": format!("Invalid namespace: {}", err) })),
        )
    })?;
    debug!("Validated namespace");

    let mut owner_ids: Vec<String> = if let Some(orgs) = &user_profile.organizations {
        orgs.keys().cloned().collect()
    } else {
        Vec::new()
    };
    owner_ids.push(user_profile.email.clone());

    debug!("Authorizing namespace");
    let owner = auth_ns(db_pool, &owner_ids, &namespace)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": format!("Authorization error: {}", e)})),
            )
        })?;
    debug!("Authorized namespace");

    // Create the standard processor platform
    let app_state = Arc::new(AppState {
        db_pool: db_pool.clone(),
        message_queue: state.message_queue.clone(),
    });
    let platform = StandardProcessor::new(app_state);

    debug!("Declaring processor with namespace: {:?}", namespace);
    let processor = platform
        .declare(
            &processor_request,
            db_pool,
            &user_profile,
            &owner,
            &namespace,
        )
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": e.to_string()})),
            )
        })?;

    Ok(Json(processor))
}

pub async fn scale_processor(
    State(state): State<AppState>,
    Extension(user_profile): Extension<V1UserProfile>,
    Path((namespace, name)): Path<(String, String)>,
    Json(scale_request): Json<V1ProcessorScaleRequest>,
) -> Result<impl IntoResponse, (StatusCode, Json<serde_json::Value>)> {
    let result = _scale_processor(
        &state.db_pool,
        &namespace,
        &name,
        &user_profile,
        scale_request,
    )
    .await?;

    Ok(Json(result))
}

// Internal function that performs the actual scaling
async fn _scale_processor(
    db_pool: &DatabaseConnection,
    namespace: &str,
    name: &str,
    user_profile: &V1UserProfile,
    scale_request: V1ProcessorScaleRequest,
) -> Result<V1Processor, (StatusCode, Json<serde_json::Value>)> {
    // Validate we have at least one parameter
    if scale_request.replicas.is_none() && scale_request.min_replicas.is_none() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "At least one of 'replicas' or 'min_replicas' must be provided"})),
        ));
    }

    // Collect owner IDs
    let mut owner_ids: Vec<String> = if let Some(orgs) = &user_profile.organizations {
        orgs.keys().cloned().collect()
    } else {
        Vec::new()
    };
    owner_ids.push(user_profile.email.clone());
    let owner_id_refs: Vec<&str> = owner_ids.iter().map(|s| s.as_str()).collect();

    // Find the processor
    let processor = Query::find_processor_by_namespace_name_and_owners(
        db_pool,
        namespace,
        name,
        &owner_id_refs,
    )
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": format!("Database error: {}", e)})),
        )
    })?;

    let mut active_model = processors::ActiveModel::from(processor);

    // Handle min_replicas update if provided
    if let Some(min_replicas) = scale_request.min_replicas {
        if min_replicas <= 0 {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(json!({"error": "min_replicas must be a positive integer"})),
            ));
        }
        debug!("Setting min_replicas to {}", min_replicas);
        active_model.min_replicas = ActiveValue::Set(Some(min_replicas));
    }

    // Handle desired_replicas update if provided or if min_replicas requires an update
    match scale_request.replicas {
        // If replicas is explicitly set
        Some(replicas) => {
            if replicas <= 0 {
                return Err((
                    StatusCode::BAD_REQUEST,
                    Json(json!({"error": "replicas must be a positive integer"})),
                ));
            }
            debug!("Setting desired_replicas to {}", replicas);
            active_model.desired_replicas = ActiveValue::Set(Some(replicas));
        }
        // If only min_replicas is provided, ensure desired_replicas is at least that amount
        None => {
            if let Some(min_replicas) = scale_request.min_replicas {
                let current_desired = match &active_model.desired_replicas {
                    ActiveValue::Set(val) => val.clone(),
                    ActiveValue::Unchanged(val) => val.clone(),
                    _ => None,
                };

                // If current desired_replicas is less than the new min_replicas or not set
                if current_desired.is_none() || current_desired.unwrap_or(0) < min_replicas {
                    debug!(
                        "Setting desired_replicas to match min_replicas: {}",
                        min_replicas
                    );
                    active_model.desired_replicas = ActiveValue::Set(Some(min_replicas));
                }
            }
        }
    }

    // Update the processor in the database
    let updated_processor = active_model.update(db_pool).await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": format!("Failed to update processor: {}", e)})),
        )
    })?;

    // Convert the updated processor model to V1Processor for the response
    let processor_v1 = updated_processor.to_v1_processor().map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": format!("Failed to convert processor: {}", e)})),
        )
    })?;

    Ok(processor_v1)
}

#[axum::debug_handler]
pub async fn list_processors(
    State(state): State<AppState>,
    Extension(user_profile): Extension<V1UserProfile>,
) -> Result<Json<V1Processors>, (StatusCode, Json<serde_json::Value>)> {
    let db_pool = &state.db_pool;

    let mut owner_ids: Vec<String> = if let Some(orgs) = &user_profile.organizations {
        orgs.keys().cloned().collect()
    } else {
        Vec::new()
    };

    // Include user's email (assuming owner_id is user's email)
    owner_ids.push(user_profile.email.clone());

    let owner_id_refs: Vec<&str> = owner_ids.iter().map(|s| s.as_str()).collect();

    // Query processors for all owner_ids
    let processor_models = Query::find_processors_by_owners(db_pool, &owner_id_refs)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": format!("Database error: {}", e)})),
            )
        })?;

    // Convert database models to API response models
    let processors_result: Result<Vec<V1Processor>, _> = processor_models
        .into_iter()
        .map(|p| p.to_v1_processor())
        .collect();

    let processors = processors_result.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": format!("Failed to convert processors: {}", e)})),
        )
    })?;

    Ok(Json(V1Processors { processors }))
}

pub async fn get_processor(
    State(state): State<AppState>,
    Extension(user_profile): Extension<V1UserProfile>,
    Path((namespace, name)): Path<(String, String)>,
) -> Result<Json<V1Processor>, (StatusCode, Json<serde_json::Value>)> {
    let db_pool = &state.db_pool;

    let mut owner_ids: Vec<String> = if let Some(orgs) = &user_profile.organizations {
        orgs.keys().cloned().collect()
    } else {
        Vec::new()
    };
    owner_ids.push(user_profile.email.clone());
    let owner_id_refs: Vec<&str> = owner_ids.iter().map(|s| s.as_str()).collect();

    let processor = Query::find_processor_by_namespace_name_and_owners(
        db_pool,
        &namespace,
        &name,
        &owner_id_refs,
    )
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": format!("Database error: {}", e)})),
        )
    })?;

    let processor_v1 = processor.to_v1_processor().map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": format!("Failed to convert processor: {}", e)})),
        )
    })?;

    Ok(Json(processor_v1))
}

pub async fn send_processor(
    State(state): State<AppState>,
    Extension(user_profile): Extension<V1UserProfile>,
    Path((namespace, name)): Path<(String, String)>,
    Json(stream_data): Json<V1StreamData>,
) -> Result<impl IntoResponse, (StatusCode, Json<serde_json::Value>)> {
    let db_pool = &state.db_pool;

    // Collect owner IDs from user_profile
    let mut owner_ids: Vec<String> = if let Some(orgs) = &user_profile.organizations {
        orgs.keys().cloned().collect()
    } else {
        Vec::new()
    };
    owner_ids.push(user_profile.email.clone());
    let owner_id_refs: Vec<&str> = owner_ids.iter().map(|s| s.as_str()).collect();

    // Find the processor
    let processor = Query::find_processor_by_namespace_name_and_owners(
        db_pool,
        &namespace,
        &name,
        &owner_id_refs,
    )
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": format!("Database error: {}", e)})),
        )
    })?;

    // Get the stream name
    let stream_name = processor.stream;

    // Generate a return stream name if we need to wait for a response
    let return_stream = if stream_data.wait.unwrap_or(false) {
        let return_stream_name = format!("return-stream-{}", Uuid::new_v4());
        Some(return_stream_name)
    } else {
        None
    };

    // Create a stream message
    let message = V1StreamMessage {
        kind: "ProcessorInput".to_string(),
        id: Uuid::new_v4().to_string(),
        content: stream_data.content,
        created_at: chrono::Utc::now().timestamp(),
        return_stream: return_stream.clone(),
        user_id: Some(user_profile.email.clone()),
        organizations: user_profile.organizations.clone().map(|orgs| json!(orgs)),
        handle: user_profile.handle.clone(),
        adapter: Some(format!("processor:{}", processor.id)),
    };

    // Access the Redis client from the message queue
    match &state.message_queue {
        crate::state::MessageQueue::Redis { client } => {
            // Get a Redis connection
            let mut conn = client.get_connection().map_err(|e| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(json!({"error": format!("Redis connection error: {}", e)})),
                )
            })?;

            // Serialize the message to JSON
            let message_json = serde_json::to_string(&message).map_err(|e| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(json!({"error": format!("Failed to serialize message: {}", e)})),
                )
            })?;

            // Add the message to the stream using higher-level xadd
            let stream_id: String = redis::cmd("XADD")
                .arg(stream_name)
                .arg("*") // Auto-generate ID
                .arg("data")
                .arg(&message_json)
                .query(&mut conn)
                .map_err(|e| {
                    (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(json!({"error": format!("Failed to send message to stream: {}", e)})),
                    )
                })?;

            // If we need to wait for a response
            if let Some(return_stream_name) = return_stream {
                tracing::debug!(
                    "Waiting for response on return stream: {}",
                    return_stream_name
                );

                // Create the return stream with a dummy message to ensure it exists
                let _: Result<String, redis::RedisError> = redis::cmd("XADD")
                    .arg(&return_stream_name)
                    .arg("*")
                    .arg("init")
                    .arg("true")
                    .query(&mut conn);

                // Wait for response with a timeout (60 seconds)
                const TIMEOUT_MS: u64 = 60000;

                // Use the higher-level streams API to read from the stream
                let result: redis::streams::StreamReadReply = redis::cmd("XREAD")
                    .arg("BLOCK")
                    .arg(TIMEOUT_MS)
                    .arg("STREAMS")
                    .arg(&return_stream_name)
                    .arg("0") // Read from the beginning
                    .query(&mut conn)
                    .map_err(|e| {
                        (
                            StatusCode::INTERNAL_SERVER_ERROR,
                            Json(json!({"error": format!("Error reading from response stream: {}", e)})),
                        )
                    })?;

                // Clean up the return stream
                let _: Result<(), redis::RedisError> =
                    redis::cmd("DEL").arg(&return_stream_name).query(&mut conn);

                // Check if we got a response
                if result.keys.is_empty() {
                    return Err((
                        StatusCode::REQUEST_TIMEOUT,
                        Json(json!({"error": "Timed out waiting for processor response"})),
                    ));
                }

                // Process the response
                for key in result.keys {
                    for id in key.ids {
                        if let Some(data_value) = id.map.get("data") {
                            // Convert the Redis value to a string
                            let data_str = match data_value {
                                redis::Value::BulkString(bytes) => {
                                    String::from_utf8_lossy(bytes).to_string()
                                }
                                redis::Value::SimpleString(s) => s.clone(),
                                _ => format!("{:?}", data_value),
                            };

                            // Try to parse the data as JSON
                            if let Ok(json_data) =
                                serde_json::from_str::<serde_json::Value>(&data_str)
                            {
                                return Ok(Json(json_data).into_response());
                            } else {
                                // Return raw data if JSON parsing fails
                                return Ok(Json(json!({"raw": data_str})).into_response());
                            }
                        }
                    }
                }

                // If we couldn't find data in the response
                return Err((
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(json!({"error": "Received response without data field"})),
                ));
            } else {
                // If not waiting, just return success
                Ok(Json(json!({
                    "success": true,
                    "stream_id": stream_id,
                    "message_id": message.id
                }))
                .into_response())
            }
        }
        crate::state::MessageQueue::Kafka { .. } => Err((
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "Kafka streams are not currently supported"})),
        )),
    }
}

pub async fn delete_processor(
    State(state): State<AppState>,
    Extension(user_profile): Extension<V1UserProfile>,
    Path((namespace, name)): Path<(String, String)>,
) -> Result<impl IntoResponse, (StatusCode, Json<serde_json::Value>)> {
    let db_pool = &state.db_pool;

    let mut owner_ids: Vec<String> = if let Some(orgs) = &user_profile.organizations {
        orgs.keys().cloned().collect()
    } else {
        Vec::new()
    };
    owner_ids.push(user_profile.email.clone());
    let owner_id_refs: Vec<&str> = owner_ids.iter().map(|s| s.as_str()).collect();

    let processor = Query::find_processor_by_namespace_name_and_owners(
        db_pool,
        &namespace,
        &name,
        &owner_id_refs,
    )
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": format!("Database error: {}", e)})),
        )
    })?;

    let app_state = Arc::new(AppState {
        db_pool: db_pool.clone(),
        message_queue: state.message_queue.clone(),
    });
    let platform = StandardProcessor::new(app_state);

    platform.delete(&processor.id, db_pool).await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": format!("Failed to delete processor: {}", e)})),
        )
    })?;

    Ok(StatusCode::OK)
}

pub async fn update_processor(
    State(state): State<AppState>,
    Extension(user_profile): Extension<V1UserProfile>,
    Path((namespace, name)): Path<(String, String)>,
    Json(update_request): Json<V1UpdateProcessor>,
) -> Result<Json<V1Processor>, (StatusCode, Json<serde_json::Value>)> {
    let db_pool = &state.db_pool;

    // Collect owner IDs from user_profile
    let mut owner_ids: Vec<String> = if let Some(orgs) = &user_profile.organizations {
        orgs.keys().cloned().collect()
    } else {
        Vec::new()
    };
    owner_ids.push(user_profile.email.clone());
    let owner_id_refs: Vec<&str> = owner_ids.iter().map(|s| s.as_str()).collect();

    // Find the processor
    let processor = Query::find_processor_by_namespace_name_and_owners(
        db_pool,
        &namespace,
        &name,
        &owner_id_refs,
    )
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": format!("Database error: {}", e)})),
        )
    })?;

    let no_delete = update_request.no_delete.unwrap_or(false);

    // Create a deep clone of processor for comparison later
    let processor_v1 = processor.to_v1_processor().map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": format!("Failed to convert processor: {}", e)})),
        )
    })?;

    // Clone all fields we'll need to check later
    let container_is_none = update_request.container.is_none();
    let stream_is_none = update_request.stream.is_none();
    let schema_is_none = update_request.schema.is_none();
    let common_schema_is_none = update_request.common_schema.is_none();
    let scale_is_none = update_request.scale.is_none();
    let max_replicas_is_none = update_request.max_replicas.is_none();
    let min_replicas_is_some = update_request.min_replicas.is_some();

    // Create a new processor request with all updated fields
    let updated_processor = V1ProcessorRequest {
        kind: update_request
            .kind
            .unwrap_or_else(|| "Processor".to_string()),
        metadata: V1ResourceMetaRequest {
            name: Some(processor.name.clone()),
            namespace: Some(processor.namespace.clone()),
            labels: update_request
                .metadata
                .as_ref()
                .and_then(|m| m.labels.clone()),
            owner: None,
            owner_ref: None,
        },
        container: update_request.container,
        schema: update_request.schema,
        common_schema: update_request.common_schema,
        min_replicas: update_request.min_replicas.or(processor.min_replicas),
        max_replicas: update_request.max_replicas.or(processor.max_replicas),
        scale: update_request.scale,
    };

    // Check if only min_replicas changed (or nothing changed)
    let min_replicas_only_changed = container_is_none
        && stream_is_none
        && schema_is_none
        && common_schema_is_none
        && scale_is_none
        && max_replicas_is_none
        && min_replicas_is_some;

    // If only min_replicas is provided, use the scale operation instead
    if min_replicas_only_changed {
        debug!("Only min_replicas changed, using scale operation");
        let scale_request = V1ProcessorScaleRequest {
            min_replicas: updated_processor.min_replicas,
            replicas: None,
        };

        // Call the internal scale function directly
        return Ok(Json(
            _scale_processor(db_pool, &namespace, &name, &user_profile, scale_request).await?,
        ));
    }

    // Check if any fields changed that would require recreation
    let changed_outside_metadata = !container_is_none
        || !stream_is_none
        || !schema_is_none
        || !common_schema_is_none
        || !scale_is_none
        || !max_replicas_is_none;

    // If changes require recreation
    if changed_outside_metadata {
        debug!("Processor changed outside metadata");
        if no_delete {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(json!({
                    "error": "Processor changes require deletion, but no_delete=true"
                })),
            ));
        }

        debug!("Deleting old processor");
        let app_state = Arc::new(AppState {
            db_pool: db_pool.clone(),
            message_queue: state.message_queue.clone(),
        });
        let platform = StandardProcessor::new(app_state);

        platform.delete(&processor.id, db_pool).await.map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": format!("Failed to delete processor: {}", e)})),
            )
        })?;

        // Create the new processor with merged values
        debug!("Creating new processor with updated fields");
        let app_state = Arc::new(AppState {
            db_pool: db_pool.clone(),
            message_queue: state.message_queue.clone(),
        });
        let platform = StandardProcessor::new(app_state);

        let created = platform
            .declare(
                &updated_processor,
                db_pool,
                &user_profile,
                &user_profile.email,
                &namespace,
            )
            .await
            .map_err(|e| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(json!({"error": e.to_string()})),
                )
            })?;
        debug!("Created new processor: {:?}", created);

        return Ok(Json(created));
    } else {
        debug!("No changes to processor, skipping update");
    }

    // Return the original processor if no changes were made
    Ok(Json(processor_v1))
}
