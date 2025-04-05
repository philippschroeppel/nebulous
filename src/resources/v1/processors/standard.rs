use crate::config::CONFIG;
use crate::entities::containers;
use crate::entities::processors;
use crate::models::V1ResourceMetaRequest;
use crate::models::V1UserProfile;
use crate::mutation::Mutation;
use crate::query::Query;
use crate::resources::v1::containers::base::ContainerPlatform;
use crate::resources::v1::containers::factory::platform_factory;
use crate::resources::v1::containers::models::V1EnvVar;
use crate::resources::v1::containers::models::{RestartPolicy, V1ContainerRequest};
use crate::resources::v1::containers::runpod::RunpodPlatform;
use crate::resources::v1::processors::base::{ProcessorPlatform, ProcessorStatus};
use crate::resources::v1::processors::models::{V1Processor, V1ProcessorRequest};
use crate::state::MessageQueue;
use crate::streams::redis::get_consumer_group_progress;
use crate::AppState;
use chrono::{DateTime, Duration, Utc};
use sea_orm::{ActiveModelTrait, ActiveValue::Set, DatabaseConnection};
use std::str::FromStr;
use std::sync::Arc;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

/// Standard implementation of the ProcessorPlatform trait
pub struct StandardProcessor {
    state: Arc<AppState>,
}

impl StandardProcessor {
    /// Create a new StandardProcessor instance
    pub fn new(state: Arc<AppState>) -> Self {
        Self { state }
    }

    fn customize_container(
        &self,
        processor: &processors::Model,
        container: Option<V1ContainerRequest>,
        redis_client: &redis::Client,
    ) -> Result<V1ContainerRequest, Box<dyn std::error::Error + Send + Sync>> {
        debug!(
            "[Processor Controller] Customizing container {:?}",
            container
        );
        // Parse container or use default
        let mut parsed_container = match container {
            Some(c) => c,
            None => match processor.parse_container() {
                Ok(Some(c)) => c,
                Ok(None) => {
                    info!("[Processor Controller] Using default container spec");
                    Default::default()
                }
                Err(e) => {
                    return Err(format!(
                        "Failed to parse container JSON for processor {}: {}",
                        processor.id, e
                    )
                    .into());
                }
            },
        };

        let mut metadata = parsed_container.metadata.unwrap_or_default();
        let mut env = parsed_container.env.clone().unwrap_or_default();

        // Use processor ID for username (sanitize for Redis)
        let username = format!("proc_{}", processor.id.replace("-", "_"));

        // TODO: lol
        let password = format!("pass_{}", processor.id.replace("-", ""));

        // Stream key pattern this processor should access
        let stream_pattern = format!("processor:{}:{}*", processor.namespace, processor.name);

        // Connect to Redis
        let mut conn = redis_client.get_connection().map_err(
            |e| -> Box<dyn std::error::Error + Send + Sync> {
                format!("Failed to connect to Redis: {}", e).into()
            },
        )?;

        // Check if user already exists
        let user_exists: bool = redis::cmd("ACL")
            .arg("GETUSER")
            .arg(&username)
            .query(&mut conn)
            .unwrap_or(false);

        // Only create the user if it doesn't exist
        if !user_exists {
            // Create ACL user with permissions only for specific stream pattern
            let acl_result: redis::RedisResult<String> = redis::cmd("ACL")
                .arg("SETUSER")
                .arg(&username)
                .arg("on")
                .arg(format!(">{}", &password))
                .arg(format!("~{}", &stream_pattern)) // Key pattern restriction
                .arg("+xread") // Allow stream reading
                .arg("+xadd") // Allow adding to streams
                .arg("+xgroup") // Allow consumer group operations
                .arg("+xreadgroup") // Allow reading as consumer group
                .arg("+ping") // Basic connection check
                .query(&mut conn);

            match acl_result {
                Ok(_) => info!(
                    "[Processor] Created Redis ACL user for processor {}",
                    processor.id
                ),
                Err(e) => return Err(format!("Failed to create Redis ACL user: {}", e).into()),
            }
        }

        // Add Redis credentials to environment variables
        env.push(V1EnvVar {
            key: "REDIS_USERNAME".to_string(),
            value: Some(username.clone()),
            secret_name: None,
        });

        env.push(V1EnvVar {
            key: "REDIS_PASSWORD".to_string(),
            value: Some(password.clone()),
            secret_name: None,
        });

        env.push(V1EnvVar {
            key: "REDISCLI_AUTH".to_string(),
            value: Some(password.clone()),
            secret_name: None,
        });

        // Redis URL with credentials
        let redis_url = format!(
            "redis://{}:{}@{}:{}",
            username, password, CONFIG.redis_host, CONFIG.redis_port
        );

        // Add all Redis env vars
        env.push(V1EnvVar {
            key: "REDIS_URL".to_string(),
            value: Some(redis_url),
            secret_name: None,
        });
        env.push(V1EnvVar {
            key: "REDIS_HOST".to_string(),
            value: Some(CONFIG.redis_host.clone()),
            secret_name: None,
        });
        env.push(V1EnvVar {
            key: "REDIS_PORT".to_string(),
            value: Some(CONFIG.redis_port.clone()),
            secret_name: None,
        });
        env.push(V1EnvVar {
            key: "REDIS_CONSUMER_GROUP".to_string(),
            value: Some(processor.id.clone()),
            secret_name: None,
        });
        env.push(V1EnvVar {
            key: "REDIS_STREAM".to_string(),
            value: Some(processor.stream.clone()),
            secret_name: None,
        });

        // Configure labels and metadata
        let mut labels = metadata.labels.clone().unwrap_or_default();
        labels.insert("processor".to_string(), processor.id.clone());
        metadata.labels = Some(labels);
        metadata.owner_ref = Some(format!(
            "{}.{}.Processor",
            processor.name, processor.namespace
        ));
        metadata.namespace = Some(processor.namespace.clone());

        // Update the container
        parsed_container.metadata = Some(metadata);
        parsed_container.env = Some(env);

        Ok(parsed_container)
    }

    /// Start a processor, creating its minimum number of containers on Runpod (example).
    async fn start_processor(
        &self,
        db: &DatabaseConnection,
        processor: processors::Model,
        namespace: &str,
        redis_client: &redis::Client,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        info!("[Processor Controller] Starting processor {}", processor.id);

        // 1) Mark Processor status as Creating
        Mutation::update_processor_status(
            db,
            processor.id.clone(),
            Some(ProcessorStatus::Creating.to_string()),
            None,
        )
        .await?;

        // 2) Get customized container
        let container = self.customize_container(&processor, None, redis_client)?;

        // 3) Decide how many containers to create based on `min_replicas`
        let min_replicas = processor.min_replicas.unwrap_or(1).max(1);
        info!(
            "[Processor Controller] Processor {} => creating {} container(s).",
            processor.id, min_replicas
        );

        // 4) Create a ContainerPlatform
        let runpod = RunpodPlatform::new();

        // 5) User profile for container creation
        let user_profile = V1UserProfile {
            email: processor
                .created_by
                .clone()
                .unwrap_or_else(|| "unknown@domain.tld".to_string()),
            ..Default::default()
        };

        // 6) For each replica, create the container
        for replica_index in 0..min_replicas {
            let mut request_for_replica = container.clone();
            if let Some(mut meta) = request_for_replica.metadata.take() {
                meta.name = Some(format!("{:?}-replica-{:?}", meta.name, replica_index));
                request_for_replica.metadata = Some(meta);
            }

            info!(
                "[Processor Controller] Creating container #{} for processor {}",
                replica_index, processor.id
            );

            let declared = runpod
                .declare(
                    &request_for_replica,
                    db,
                    &user_profile,
                    &processor.owner,
                    &processor.namespace,
                )
                .await?;

            info!(
                "[Processor Controller] Created container {} (id = {}) for processor {}",
                declared.metadata.name, declared.metadata.id, processor.id
            );
        }

        // 7) Update Processor status to Created
        Mutation::update_processor_status(
            db,
            processor.id,
            Some(ProcessorStatus::Created.to_string()),
            None,
        )
        .await?;

        Ok(())
    }

    // Helper function to parse duration string into chrono::Duration
    fn parse_duration(
        &self,
        duration_str: &str,
    ) -> Result<Duration, Box<dyn std::error::Error + Send + Sync>> {
        // Parse duration strings in format like "1m", "30s", "5h"
        let len = duration_str.len();
        if len < 2 {
            return Err(format!("Invalid duration format: {}", duration_str).into());
        }

        let (value_str, unit) = duration_str.split_at(len - 1);
        let value = value_str
            .parse::<i64>()
            .map_err(|e| format!("Invalid duration value: {}", e))?;

        match unit {
            "s" => Ok(Duration::seconds(value)),
            "m" => Ok(Duration::minutes(value)),
            "h" => Ok(Duration::hours(value)),
            "d" => Ok(Duration::days(value)),
            _ => Err(format!("Unsupported duration unit: {}", unit).into()),
        }
    }

    // Helper to check if duration threshold has been met
    #[allow(dependency_on_unit_never_type_fallback)] // TODO: this is due to Redis crate
    async fn duration_threshold_met(
        &self,
        redis_client: &redis::Client,
        processor_id: &str,
        action_type: &str,
        required_duration: &Duration,
    ) -> Result<bool, Box<dyn std::error::Error + Send + Sync>> {
        let redis_key = format!("processor:{}:scale:{}", processor_id, action_type);
        let mut conn = redis_client.get_connection()?;

        // Check if we already have timestamp in Redis
        let threshold_start: Option<String> = redis::cmd("GET")
            .arg(&redis_key)
            .query::<Option<String>>(&mut conn)
            .unwrap_or(None);

        match threshold_start {
            Some(timestamp_str) => {
                // Parse the stored timestamp
                let threshold_time = DateTime::parse_from_rfc3339(&timestamp_str)
                    .map_err(|e| format!("Invalid timestamp format in Redis: {}", e))?
                    .with_timezone(&Utc);

                // Check if enough time has elapsed
                let elapsed = Utc::now() - threshold_time;
                Ok(elapsed >= *required_duration)
            }
            None => {
                // First time we're seeing this threshold exceeded, store current time
                let now = Utc::now();
                redis::cmd("SET")
                    .arg(&redis_key)
                    .arg(now.to_rfc3339())
                    .query(&mut conn)?;

                // Have not met duration threshold yet
                Ok(false)
            }
        }
    }

    // Helper to clear stored duration threshold
    async fn clear_duration_threshold(
        &self,
        redis_client: &redis::Client,
        processor_id: &str,
        action_type: &str,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        #![allow(dependency_on_unit_never_type_fallback)]
        let redis_key = format!("processor:{}:scale:{}", processor_id, action_type);
        let mut conn = redis_client.get_connection()?;

        redis::cmd("DEL").arg(&redis_key).query(&mut conn)?;

        Ok(())
    }

    /// Watch/monitor a processor and scale containers based on Redis queue 'pressure'.
    async fn watch_processor(
        &self,
        db: &DatabaseConnection,
        processor: processors::Model,
        redis_client: &redis::Client,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        use tracing::info;

        // 2) Attempt to parse container config from JSON in `processor.container`.
        //    If none is stored, fall back to some defaults.
        let mut parsed_container = match processor.parse_container() {
            Ok(Some(c)) => c, // `c` is a V1Container
            Ok(None) => {
                // No container data stored, so provide a fallback:
                info!(
                    "[Processor Controller] Processor {} has no container spec; using defaults",
                    processor.id
                );
                Default::default()
            }
            Err(e) => {
                // If there's invalid JSON in the DB, handle or return error
                return Err(format!(
                    "Failed to parse container JSON for processor {}: {}",
                    processor.id, e
                )
                .into());
            }
        };

        // Get actual current replica count from DB
        let current_replicas = Query::count_active_containers_for_processor(db, &processor.id)
            .await
            .map_err(|e| format!("Failed to count current containers: {}", e))?;

        info!("Current replicas: {:?}", current_replicas);

        self.reconcile_replicas(
            &processor,
            current_replicas as i32,
            processor.desired_replicas.unwrap_or(1),
            parsed_container.clone(),
            db,
            redis_client,
        )
        .await?;

        // Make a connection from the client:
        let mut con = redis_client.get_connection()?;

        info!("[Processor Controller] Watching processor {}", processor.id);

        // 1) Make sure there's a stream name in the processor.
        //    We'll treat the processor's `stream` field as the Redis stream name.
        let stream_name = processor.stream.clone();

        // 2) The consumer group is the processor's ID.
        let consumer_group = &processor.id;
        debug!("Consumer group: {:?}", consumer_group);

        // 3) Check how many messages are pending for this group in the Redis stream (i.e. 'pressure').
        //    This uses the `redis` crate’s XPending or XPendingCount functionality.
        //    Adjust the connection string or usage as necessary for your environment.
        let pending_count =
            match get_consumer_group_progress(&mut con, &stream_name, consumer_group) {
                Ok(progress) => progress.remaining_entries(),
                Err(err) => {
                    error!(
                    "[Processor Controller] Error getting pending count for processor {:?}: {:?}",
                    processor.id, err
                );
                    return Err(format!(
                        "Error getting pending count for processor {:?}: {:?}",
                        processor.id, err
                    )
                    .into());
                }
            };

        debug!("Pending count: {:?}", pending_count);

        // 4) Compare `pending_count` to scale.up.pressure and scale.down.pressure.
        //    We'll parse the scale from the DB (the 'scale' JSON column).
        let scale = if let Ok(s) = processor.parse_scale() {
            s
        } else {
            None
        };

        // If no scale object, do nothing special
        let Some(scale) = scale else {
            info!(
                "[Processor Controller] Processor {} has no scale rules; skipping watch",
                processor.id
            );
            return Ok(());
        };

        debug!("Scale: {:?}", scale);

        // Extract scale up/down thresholds.
        let scale_up_threshold = scale
            .up
            .as_ref()
            .and_then(|up| up.above_pressure)
            .unwrap_or(i32::MAX); // If none specified, we won't scale up
        let scale_down_threshold = scale
            .down
            .as_ref()
            .and_then(|down| down.below_pressure)
            .unwrap_or(0); // If none, we won't scale down

        // Extract and parse scale durations
        let scale_up_duration = scale
            .up
            .as_ref()
            .and_then(|up| up.duration.clone())
            .map(|d| self.parse_duration(&d))
            .transpose()?
            .unwrap_or(Duration::zero()); // Default to instant scaling if no duration specified

        let scale_down_duration = scale
            .down
            .as_ref()
            .and_then(|down| down.duration.clone())
            .map(|d| self.parse_duration(&d))
            .transpose()?
            .unwrap_or(Duration::zero()); // Default to instant scaling if no duration specified

        debug!(
            "Scale up threshold: {:?}, duration: {:?}",
            scale_up_threshold, scale_up_duration
        );
        debug!(
            "Scale down threshold: {:?}, duration: {:?}",
            scale_down_threshold, scale_down_duration
        );

        let mut new_replica_count = current_replicas;
        debug!("New replica count: {:?}", new_replica_count);

        // Scale-up check with duration handling
        if (pending_count as i32) >= scale_up_threshold {
            // Check if we need duration tracking for scale up
            if scale_up_duration > Duration::zero() {
                let duration_met = self
                    .duration_threshold_met(redis_client, &processor.id, "up", &scale_up_duration)
                    .await?;

                if duration_met {
                    // Scale up only if threshold met for required duration
                    new_replica_count = current_replicas + 1;
                    info!(
                        "[Processor Controller] Scaling UP processor {} from {} -> {} replicas (duration threshold met)",
                        processor.id, current_replicas, new_replica_count
                    );

                    // Clear the threshold after scaling
                    self.clear_duration_threshold(redis_client, &processor.id, "up")
                        .await?;
                } else {
                    info!(
                        "[Processor Controller] Processor {} is above scale-up threshold, but duration not yet met",
                        processor.id
                    );
                }
            } else {
                // Instant scale up (no duration requirement)
                new_replica_count = current_replicas + 1;
                info!(
                    "[Processor Controller] Scaling UP processor {} from {} -> {} replicas",
                    processor.id, current_replicas, new_replica_count
                );
            }

            // Clear any scale-down threshold tracking when we're in scale-up condition
            self.clear_duration_threshold(redis_client, &processor.id, "down")
                .await?;
        }
        // Scale-down check with duration handling
        else if (pending_count as i32) <= scale_down_threshold && current_replicas > 1 {
            // Check if we need duration tracking for scale down
            if scale_down_duration > Duration::zero() {
                let duration_met = self
                    .duration_threshold_met(
                        redis_client,
                        &processor.id,
                        "down",
                        &scale_down_duration,
                    )
                    .await?;

                if duration_met {
                    // Scale down only if threshold met for required duration
                    new_replica_count = (current_replicas - 1).max(1);
                    info!(
                        "[Processor Controller] Scaling DOWN processor {} from {} -> {} replicas (duration threshold met)",
                        processor.id, current_replicas, new_replica_count
                    );

                    // Clear the threshold after scaling
                    self.clear_duration_threshold(redis_client, &processor.id, "down")
                        .await?;
                } else {
                    info!(
                        "[Processor Controller] Processor {} is below scale-down threshold, but duration not yet met",
                        processor.id
                    );
                }
            } else {
                // Instant scale down (no duration requirement)
                new_replica_count = (current_replicas - 1).max(1);
                info!(
                    "[Processor Controller] Scaling DOWN processor {} from {} -> {} replicas",
                    processor.id, current_replicas, new_replica_count
                );
            }

            // Clear any scale-up threshold tracking when we're in scale-down condition
            self.clear_duration_threshold(redis_client, &processor.id, "up")
                .await?;
        } else {
            // We're not in a scale condition, clear both trackers
            self.clear_duration_threshold(redis_client, &processor.id, "up")
                .await?;
            self.clear_duration_threshold(redis_client, &processor.id, "down")
                .await?;
        }

        // 6) If the replica count changed, update DB, then reconcile or create/destroy containers as needed.
        if new_replica_count != current_replicas {
            debug!(
                "[Processor Controller] Processor {} replica count changed from {} to {}",
                processor.id, current_replicas, new_replica_count
            );
            let mut active_model = processors::ActiveModel::from(processor.clone());
            active_model.desired_replicas =
                sea_orm::ActiveValue::Set(Some(new_replica_count as i32));
            let updated_model = active_model.update(db).await?;

            let mut metadata = parsed_container.metadata.clone().unwrap_or_default();
            metadata.namespace = Some(processor.namespace.clone());
            metadata.owner_ref = Some(format!(
                "{}.{}.Processor",
                processor.name, processor.namespace
            ));
            parsed_container.metadata = Some(metadata);

            info!(
                "[Processor Controller] Updated processor {} min_replicas to {} in DB",
                updated_model.id, new_replica_count
            );
            self.reconcile_replicas(
                &updated_model,
                current_replicas as i32,
                new_replica_count as i32,
                parsed_container,
                db,
                redis_client,
            )
            .await?;
        } else {
            info!(
                "[Processor Controller] No scale change for processor {}; replicas remain {}",
                processor.id, current_replicas
            );
        }

        Ok(())
    }

    async fn reconcile_replicas(
        &self,
        processor: &processors::Model,
        current_replicas: i32,
        new_replica_count: i32,
        container_request: V1ContainerRequest,
        db: &DatabaseConnection,
        redis_client: &redis::Client,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // Get the customized container with all our environment variables
        let container =
            self.customize_container(processor, Some(container_request), redis_client)?;

        debug!(
            "[Processor Controller] Processor {} customized container: {:?}",
            processor.id, container
        );

        // Get the appropriate platform for this container
        let platform_str = container.platform.clone().unwrap_or("runpod".to_string());
        let platform = platform_factory(platform_str);

        if new_replica_count > current_replicas {
            // Create containers for the difference between current and new count
            for replica_index in current_replicas..new_replica_count {
                let mut request_for_replica = container.clone();
                if let Some(mut meta) = request_for_replica.metadata.take() {
                    meta.name = Some(format!(
                        "{}-replica-{}",
                        meta.name.unwrap_or_default(),
                        replica_index
                    ));
                    request_for_replica.metadata = Some(meta);
                }

                info!(
                    "[Processor Controller] Creating container #{} for processor {}",
                    replica_index, processor.id
                );

                let user_profile = V1UserProfile {
                    email: processor
                        .created_by
                        .clone()
                        .unwrap_or_else(|| "unknown@domain.tld".to_string()),
                    ..Default::default()
                };

                let declared = platform
                    .declare(
                        &request_for_replica,
                        db,
                        &user_profile,
                        &processor.owner,
                        &processor.namespace,
                    )
                    .await?;

                info!(
                    "[Processor Controller] Created container {} (id = {}) for processor {}",
                    declared.metadata.name, declared.metadata.id, processor.id
                );
            }
        } else if new_replica_count < current_replicas {
            // Sort containers by replica number (extracted from name)
            let containers: Vec<containers::Model> =
                match Query::find_containers_by_owner_ref(db, &processor.id).await {
                    Ok(c) => c,
                    Err(e) => {
                        error!(
                            "[Processor Controller] Error finding containers for processor {}: {}",
                            processor.id, e
                        );
                        return Err(e.into());
                    }
                };

            debug!("Containers: {:?}", containers);

            let mut containers_to_remove: Vec<(i32, containers::Model)> = containers
                .into_iter()
                .filter_map(|c: containers::Model| {
                    c.name
                        .split("-replica-")
                        .nth(1)
                        .and_then(|num| num.parse::<i32>().ok())
                        .map(|replica_num| (replica_num, c))
                })
                .collect();

            debug!("Containers to remove: {:?}", containers_to_remove);

            containers_to_remove.sort_by_key(|(num, _)| *num);
            containers_to_remove.reverse(); // Remove highest numbered replicas first

            // Remove containers from highest replica number down to new_replica_count
            for (_, container) in containers_to_remove
                .iter()
                .take((current_replicas - new_replica_count) as usize)
            {
                info!(
                    "[Processor Controller] Removing container {} for processor {}",
                    container.name, processor.id
                );

                match platform.delete(&container.id, db).await {
                    Ok(_) => info!(
                        "[Processor Controller] Successfully removed container {} for processor {}",
                        container.name, processor.id
                    ),
                    Err(e) => error!(
                        "[Processor Controller] Failed to remove container {} for processor {}: {}",
                        container.name, processor.id, e
                    ),
                }
            }
        }

        Ok(())
    }
}

impl ProcessorPlatform for StandardProcessor {
    async fn declare(
        &self,
        config: &V1ProcessorRequest,
        db: &DatabaseConnection,
        user_profile: &V1UserProfile,
        owner_id: &str,
        namespace: &str,
    ) -> Result<V1Processor, Box<dyn std::error::Error + Send + Sync>> {
        // 1. Generate a unique ID for the new processor.
        let new_id = Uuid::new_v4().to_string();
        let name = config
            .metadata
            .name
            .clone()
            .unwrap_or(petname::petname(3, "-").unwrap());

        // 2. Create an ActiveModel to represent the new record in the database.
        let processor_am = processors::ActiveModel {
            // Primary fields
            id: Set(new_id),
            name: Set(name.clone()),
            namespace: Set(namespace.to_string()),
            owner: Set(owner_id.to_string()),
            created_by: Set(Some(user_profile.email.clone())),

            // Any JSON fields from config (e.g., container & scale).
            // Adjust as needed depending on your actual request struct.
            container: Set(config
                .container
                .clone()
                .map(|c| serde_json::to_value(c))
                .transpose()?),
            scale: Set(
                config
                    .scale
                    .clone()
                    .map(serde_json::to_value)
                    .transpose()? // produces Result<Option<JsonValue>, _>
                    .unwrap_or(serde_json::Value::Null), // ensure a valid JSON Value
            ),
            labels: Set(config
                .metadata
                .labels
                .clone()
                .map(|l| serde_json::to_value(l))
                .transpose()?),

            stream: Set(format!("processor:{}:{}", namespace, name)),

            // Typically set an initial status or desired_status to "Defined" or similar.
            status: Set(Some(serde_json::to_value(ProcessorStatus::Defined)?)),
            desired_status: Set(Some(ProcessorStatus::Running.to_string())),

            // For scale, you might also set min_replicas/max_replicas if that’s appropriate.
            min_replicas: Set(config.min_replicas.clone()),
            max_replicas: Set(config.max_replicas.clone()),

            // Auto-set timestamps.
            created_at: Set(Utc::now().into()),
            updated_at: Set(Utc::now().into()),

            ..Default::default()
        };

        // 3. Insert into the DB.
        let inserted_model = processor_am.insert(db).await?;

        // 4. Convert the inserted record back to your desired V1Processor and return.
        Ok(inserted_model.to_v1_processor()?)
    }

    async fn reconcile(
        &self,
        processor: &processors::Model,
        db: &DatabaseConnection,
        redis_client: &redis::Client,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        debug!(
            "[DEBUG:standard.rs:reconcile] Entering reconcile for processor {}",
            processor.id
        );

        // 1) Attempt to parse the current status from the DB row
        if let Ok(Some(parsed_status)) = processor.parse_status() {
            let status_str = parsed_status
                .status
                .clone()
                .unwrap_or_else(|| ProcessorStatus::Invalid.to_string());
            debug!(
                "[DEBUG:standard.rs:reconcile] Processor {} has status '{}'",
                processor.id, status_str
            );

            let status = ProcessorStatus::from_str(&status_str).unwrap_or(ProcessorStatus::Invalid);

            // 2) If it needs to start, call our 'start_processor' helper
            if status.needs_start() {
                info!(
                    "[Processor Controller] Processor {} needs to be started",
                    processor.id
                );
                if let Some(ds) = &processor.desired_status {
                    if ds == &ProcessorStatus::Running.to_string() {
                        info!(
                            "[Processor Controller] Processor {} desired_status is 'Running'; starting...",
                            processor.id
                        );
                        self.start_processor(
                            db,
                            processor.clone(),
                            &processor.namespace.as_str(),
                            redis_client,
                        )
                        .await?;
                    } else {
                        info!(
                            "[Processor Controller] Processor {} desired_status is '{}', not 'Running'",
                            processor.id, ds
                        );
                    }
                } else {
                    info!(
                        "[Processor Controller] Processor {} has no desired_status. Skipping start.",
                        processor.id
                    );
                }
            }

            // 3) If it needs to be watched, we call our watch helper
            if status.needs_watch() {
                info!(
                    "[Processor Controller] Processor {} needs to be watched",
                    processor.id
                );
                // 1) Match on the enum to get the Redis Client, if it’s a Redis-based queue
                match &self.state.message_queue {
                    MessageQueue::Redis { client } => {
                        self.watch_processor(db, processor.clone(), client.as_ref())
                            .await?;
                    }
                    MessageQueue::Kafka { .. } => {
                        info!("[Processor Controller] Not a Redis queue; skipping watch");
                    }
                }
            }
        } else {
            warn!(
                "[Processor Controller] Processor {} has no parsable status; skipping reconcile",
                processor.id
            );
        }

        debug!(
            "[DEBUG:standard.rs:reconcile] Completed reconcile for processor {}",
            processor.id
        );
        Ok(())
    }

    async fn delete(
        &self,
        id: &str,
        db: &DatabaseConnection,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        use crate::entities::processors;
        use crate::query::Query;
        use crate::resources::v1::containers::factory::platform_factory;
        use sea_orm::EntityTrait;

        // 1) Find the processor in the database by `id`.
        let Some(processor) = processors::Entity::find_by_id(id.to_string())
            .one(db)
            .await?
        else {
            return Ok(());
        };

        tracing::info!("Deleting processor '{}'...", processor.id);

        // 2) If you query by metadata->>'owner_ref' or by labels->>'processor-id':
        let associated_containers = Query::find_containers_by_owner_ref(db, &processor.id).await?;

        if associated_containers.is_empty() {
            tracing::info!(
                "No containers found referencing processor '{}'",
                processor.id
            );
        } else {
            tracing::info!(
                "Found {} container(s) referencing processor '{}'",
                associated_containers.len(),
                processor.id
            );
        }

        // 3) We’ll remove each container from its own platform:
        for container in associated_containers {
            // a) Parse the container's intended platform (e.g. "runpod" or "kube")
            let platform_str = container.platform.clone().unwrap_or("runpod".to_string());
            // fallback to "runpod" or whichever makes sense

            let platform = platform_factory(platform_str);
            platform.delete(&container.id, db).await?;

            // // e) Remove the container record from DB
            // container.delete(db).await?;
        }

        // 4) Finally, delete the processor record
        processors::Entity::delete_by_id(processor.id)
            .exec(db)
            .await?;

        tracing::info!(
            "Successfully deleted processor '{}' and its associated containers.",
            id
        );

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    #[allow(unused_imports)]
    use super::*;

    // Unit tests for StandardProcessor
    #[tokio::test]
    async fn test_declare() {
        // Test implementation
    }

    #[tokio::test]
    async fn test_reconcile() {
        // Test implementation
    }

    #[tokio::test]
    async fn test_delete() {
        // Test implementation
    }
}
