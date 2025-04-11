use crate::agent::ns::auth_ns;
use crate::entities::processors;
use crate::models::{V1ResourceMetaRequest, V1StreamData, V1StreamMessage, V1UserProfile};
use crate::query::Query;
use crate::resources::v1::containers::models::{V1Container, V1ContainerRequest};
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
use short_uuid::ShortUuid;
use std::sync::Arc;
use tracing::{debug, error};

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

    debug!(
        "Authorizing namespace {:?} with owner_ids {:?}",
        namespace, owner_ids
    );
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
    let processor = match platform
        .declare(
            &processor_request,
            db_pool,
            &user_profile,
            &owner,
            &namespace,
        )
        .await
    {
        Ok(processor) => processor,
        Err(e) => {
            error!("Error declaring processor: {:?}", e);
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": e.to_string()})),
            ));
        }
    };

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
    let id = ShortUuid::generate().to_string();

    // Generate a return stream name if we need to wait for a response
    let return_stream = if stream_data.wait.unwrap_or(false) {
        let return_stream_name = format!("{}.return.{}", stream_name, id.clone());
        Some(return_stream_name)
    } else {
        None
    };
    debug!("Sending message to processor: {}", stream_name);
    debug!("content: {:?}", stream_data.content);

    // Create a stream message
    let message = V1StreamMessage {
        kind: "StreamMessage".to_string(),
        id: id.clone(),
        content: stream_data.content,
        created_at: chrono::Utc::now().timestamp(),
        return_stream: return_stream.clone(),
        user_id: Some(user_profile.email.clone()),
        orgs: user_profile.organizations.clone().map(|orgs| json!(orgs)),
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

                // Wait for response with a timeout (1 hour)
                const TIMEOUT_MS: u64 = 3600000;

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
    debug!("Deleting processor: {} in namespace: {}", name, namespace);
    let db_pool = &state.db_pool;

    let mut owner_ids: Vec<String> = if let Some(orgs) = &user_profile.organizations {
        orgs.keys().cloned().collect()
    } else {
        Vec::new()
    };
    owner_ids.push(user_profile.email.clone());
    let owner_id_refs: Vec<&str> = owner_ids.iter().map(|s| s.as_str()).collect();

    debug!("Finding processor: {} in namespace: {}", name, namespace);
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

    debug!("Deleting processor: {}", processor.id);
    let app_state = Arc::new(AppState {
        db_pool: db_pool.clone(),
        message_queue: state.message_queue.clone(),
    });
    let platform = StandardProcessor::new(app_state);

    let redis = match &state.message_queue {
        crate::state::MessageQueue::Redis { client } => client,
        _ => {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(json!({"error": "Kafka streams are not currently supported"})),
            ))
        }
    };

    platform
        .delete(&processor.id, db_pool, redis)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": format!("Failed to delete processor: {}", e)})),
            )
        })?;

    debug!("Deleted processor: {}", processor.id);

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

    // Convert processor model to V1Processor for comparison and potential return value
    let processor_v1 = processor.to_v1_processor().map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": format!("Failed to convert processor: {}", e)})),
        )
    })?;

    // --- Start: Determine if recreation is required ---
    let mut requires_recreation = false;

    // Check stream (Assuming processor_v1 has stream: String)
    // Note: processor_v1 doesn't directly expose stream, it's part of the DB model 'processor'
    if let Some(update_stream) = &update_request.stream {
        if *update_stream != processor.stream {
            // Compare with the original db model field
            requires_recreation = true;
            debug!(
                "Stream changed ('{}' vs '{}'), requires recreation",
                update_stream, processor.stream
            );
        }
    }

    // Check schema
    if !requires_recreation
        && update_request.schema.is_some()
        && update_request.schema != processor_v1.schema
    {
        debug!("Schema changed, does not require recreation");
    }

    // Check common_schema
    if !requires_recreation
        && update_request.common_schema.is_some()
        && update_request.common_schema != processor_v1.common_schema
    {
        debug!("Common schema changed, does not require recreation");
    }

    // Check scale
    if !requires_recreation
        && update_request.scale.is_some()
        && update_request.scale != processor_v1.scale
    {
        debug!("Scale changed, does not require recreation");
    }

    // Check max_replicas
    if !requires_recreation
        && update_request.max_replicas.is_some()
        && update_request.max_replicas != processor_v1.max_replicas
    {
        debug!("Max replicas changed, does not require recreation");
    }

    // Check container (ignoring status)
    if !requires_recreation {
        match (&update_request.container, &processor_v1.container) {
            (Some(update_req), Some(existing_container)) => {
                // Compare V1ContainerRequest fields against V1Container fields
                if update_req != existing_container {
                    requires_recreation = true;
                    debug!("Container config changed, requires recreation");
                }
            }
            (Some(_), None) => {
                // Adding a container where none existed
                requires_recreation = true;
                debug!("Container added, requires recreation");
            }
            (None, Some(_)) => {
                // Removing a container. Decide if this needs recreation.
                // For now, let's assume removing requires recreation if not handled by scaling/stopping.
                // If update_request.container is None, it implies the user wants it removed or managed by another field.
                // Consider if scale to 0 or a dedicated 'stop' action should handle this instead of implicit removal via update.
                // For safety, let's trigger recreation if container exists but is not in the update request.
                // requires_recreation = true;
                // debug!("Container removed (implicitly), requires recreation");
                // --- OR --- Treat as no-change if only removal is implied?
                debug!("Container exists but not specified in update. No change triggered.");
            }
            (None, None) => {
                // No container before or after
                debug!("No container specified in update or existing. No change.");
            }
        }
    }
    // --- End: Determine if recreation is required ---

    // If changes require recreation
    if requires_recreation {
        debug!("Processor configuration changed, recreation required.");
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

        let redis = match &state.message_queue {
            crate::state::MessageQueue::Redis { client } => client,
            _ => {
                return Err((
                    StatusCode::BAD_REQUEST,
                    Json(json!({"error": "Kafka streams are not currently supported"})),
                ))
            }
        };

        platform
            .delete(&processor.id, db_pool, redis)
            .await
            .map_err(|e| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(json!({"error": format!("Failed to delete processor: {}", e)})),
                )
            })?;

        // --- Start: Create the potential final processor state by merging updates ---
        // This is needed for the declare call if recreation happens.
        let merged_processor_request = V1ProcessorRequest {
            kind: update_request
                .kind
                .clone()
                .unwrap_or_else(|| processor_v1.kind.clone()), // Use existing if not provided
            metadata: V1ResourceMetaRequest {
                name: Some(processor.name.clone()), // Name doesn't change on update
                namespace: Some(processor.namespace.clone()), // Namespace doesn't change on update
                labels: update_request
                    .metadata
                    .as_ref()
                    .and_then(|m| m.labels.clone())
                    .or_else(|| processor_v1.metadata.labels.clone()), // processor_v1.metadata is V1ResourceMeta
                owner: None,     // Usually set during creation/retrieval, not update
                owner_ref: None, // Usually set during creation/retrieval, not update
            },
            container: update_request
                .container
                .clone()
                .or(processor_v1.container.clone()), // Merge container
            schema: update_request
                .schema
                .clone()
                .or(processor_v1.schema.clone()), // Merge schema
            common_schema: update_request
                .common_schema
                .clone()
                .or(processor_v1.common_schema.clone()), // Merge common schema
            min_replicas: update_request.min_replicas.or(processor_v1.min_replicas), // Merge min_replicas
            max_replicas: update_request.max_replicas.or(processor_v1.max_replicas), // Merge max_replicas
            scale: update_request.scale.clone().or(processor_v1.scale.clone()),      // Merge scale
        };
        // --- End: Create the potential final processor state ---

        // Create the new processor with merged values
        debug!("Creating new processor with updated fields");
        let app_state = Arc::new(AppState {
            db_pool: db_pool.clone(),
            message_queue: state.message_queue.clone(),
        });
        let platform = StandardProcessor::new(app_state);

        let created = platform
            .declare(
                &merged_processor_request, // Use the merged request
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
        debug!("No changes requiring processor recreation detected. Checking for other updatable fields.");
        // --- Start: Handle updates if no recreation needed ---
        let mut processor_active_model = processors::ActiveModel::from(processor.clone()); // Use clone as processor is used later
        let mut model_updated = false;

        // Check metadata labels
        if let Some(metadata_req) = &update_request.metadata {
            if let Some(labels) = &metadata_req.labels {
                let current_labels_json = processor_active_model
                    .labels
                    .as_ref()
                    .clone()
                    .unwrap_or(serde_json::Value::Null);
                let new_labels_json = serde_json::to_value(labels).map_err(|e| {
                    (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(json!({"error": format!("Failed to serialize labels: {}", e)})),
                    )
                })?;

                if current_labels_json != new_labels_json {
                    processor_active_model.labels = ActiveValue::Set(Some(new_labels_json));
                    model_updated = true;
                    debug!("Processor labels updated.");
                }
            }
            // Add checks for other metadata fields here if they become updatable without recreation
        }

        // Check min_replicas
        if let Some(new_min_replicas) = update_request.min_replicas {
            if new_min_replicas <= 0 {
                return Err((
                    StatusCode::BAD_REQUEST,
                    Json(json!({"error": "min_replicas must be a positive integer"})),
                ));
            }
            let current_min_replicas = processor.min_replicas;
            if current_min_replicas != Some(new_min_replicas) {
                processor_active_model.min_replicas = ActiveValue::Set(Some(new_min_replicas));
                model_updated = true;
                debug!("Processor min_replicas updated to {}.", new_min_replicas);

                // Ensure desired_replicas is at least min_replicas
                let current_desired = processor.desired_replicas.unwrap_or(0);
                if current_desired < new_min_replicas {
                    debug!(
                        "Adjusting desired_replicas from {} to match new min_replicas {}",
                        current_desired, new_min_replicas
                    );
                    processor_active_model.desired_replicas =
                        ActiveValue::Set(Some(new_min_replicas));
                    // model_updated is already true
                }
            }
        }

        // Check max_replicas
        if let Some(new_max_replicas) = update_request.max_replicas {
            if new_max_replicas <= 0 {
                return Err((
                    StatusCode::BAD_REQUEST,
                    Json(json!({"error": "max_replicas must be a positive integer"})),
                ));
            }
            let current_max_replicas = processor.max_replicas;
            if current_max_replicas != Some(new_max_replicas) {
                processor_active_model.max_replicas = ActiveValue::Set(Some(new_max_replicas));
                model_updated = true;
                debug!("Processor max_replicas updated to {}.", new_max_replicas);
            }
        }

        // Check schema
        if let Some(new_schema) = &update_request.schema {
            if processor_v1.schema != Some(new_schema.clone()) {
                processor_active_model.schema = ActiveValue::Set(Some(new_schema.clone()));
                model_updated = true;
                debug!("Processor schema updated.");
            }
        }

        // Check common_schema
        if let Some(new_common_schema) = &update_request.common_schema {
            if processor_v1.common_schema != Some(new_common_schema.clone()) {
                processor_active_model.common_schema =
                    ActiveValue::Set(Some(new_common_schema.clone()));
                model_updated = true;
                debug!("Processor common_schema updated.");
            }
        }

        // Check scale
        if let Some(new_scale) = &update_request.scale {
            if processor_v1.scale.as_ref() != Some(new_scale) {
                let new_scale_json = serde_json::to_value(new_scale).map_err(|e| {
                    (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(json!({"error": format!("Failed to serialize scale: {}", e)})),
                    )
                })?;
                processor_active_model.scale = ActiveValue::Set(new_scale_json);
                model_updated = true;
                debug!("Processor scale updated.");
            }
        }

        if model_updated {
            debug!("Applying updates to processor.");
            let updated_processor_model =
                processor_active_model.update(db_pool).await.map_err(|e| {
                    (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(json!({"error": format!("Failed to update processor: {}", e)})),
                    )
                })?;
            let updated_processor_v1 = updated_processor_model.to_v1_processor().map_err(|e| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(json!({"error": format!("Failed to convert updated processor: {}", e)})),
                )
            })?;
            return Ok(Json(updated_processor_v1));
        } else {
            debug!("No recreation required and no other updates detected. Returning original processor state.");
            // If no recreation and no other changes, return the original state
            Ok(Json(processor_v1))
        }
        // --- End: Handle updates ---
    }
}
