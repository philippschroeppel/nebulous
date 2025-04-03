use crate::config::CONFIG;
use crate::entities::processors;
use crate::models::{V1ResourceMetaRequest, V1UserProfile};
use crate::resources::v1::containers::models::{RestartPolicy, V1ContainerRequest, V1EnvVar};
use crate::resources::v1::processors::base::{ProcessorPlatform, ProcessorStatus};
use crate::resources::v1::processors::models::{V1Processor, V1ProcessorRequest};
use crate::state::MessageQueue;
use crate::streams::redis::get_consumer_group_progress;
use crate::AppState;
use chrono::Utc;
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

    /// Start a processor, creating its minimum number of containers on Runpod (example).
    async fn start_processor(
        &self,
        db: &DatabaseConnection,
        processor: processors::Model,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        use crate::models::V1UserProfile;
        use crate::mutation::Mutation;
        use crate::resources::v1::containers::base::ContainerPlatform;
        use crate::resources::v1::containers::models::{RestartPolicy, V1ContainerRequest};
        use crate::resources::v1::containers::runpod::RunpodPlatform;
        use crate::resources::v1::processors::base::ProcessorStatus;
        use tracing::info;

        info!("[Processor Controller] Starting processor {}", processor.id);

        // 1) Mark Processor status as Creating.
        Mutation::update_processor_status(
            db,
            processor.id.clone(),
            Some(ProcessorStatus::Creating.to_string()),
            None,
        )
        .await?;

        // 2) Attempt to parse container config from JSON in `processor.container`.
        //    If none is stored, fall back to some defaults.
        let parsed_container = match processor.parse_container() {
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

        // 3) Decide how many containers to create based on `min_replicas` in the DB.
        let min_replicas = processor.min_replicas.unwrap_or(1).max(1);
        info!(
            "[Processor Controller] Processor {} => creating {} container(s).",
            processor.id, min_replicas
        );

        let mut env = parsed_container.env.unwrap_or_default();

        debug!(
            "[DEBUG:standard.rs:start_processor] creating redis with config: {:?}",
            CONFIG
        );
        let redis_password = CONFIG.redis_password.clone();

        if let Some(password) = redis_password.clone() {
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
        }

        // Check if REDIS_PASSWORD is set
        let redis_url = match redis_password.clone() {
            Some(password) if !password.is_empty() => {
                format!(
                    "redis://:{}@{}:{}",
                    password.clone(),
                    CONFIG.redis_host,
                    CONFIG.redis_port
                )
            }
            _ => {
                format!("redis://{}:{}", CONFIG.redis_host, CONFIG.redis_port)
            }
        };
        env.push(V1EnvVar {
            key: "REDIS_URL".to_string(),
            value: Some(redis_url.clone()),
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
            value: Some(processor.stream.clone().unwrap_or_default()),
            secret_name: None,
        });

        // 4) Add processor ID to the labels so we can track which processor it belongs to.
        let mut labels = parsed_container.metadata.labels.unwrap_or_default();
        labels.insert("processor".to_string(), processor.id.clone());

        // 4) Build a new V1ContainerRequest from the parsed container.
        //    We'll fill in some fields from V1Container (image, env, volumes, etc.).
        //    If your processor stores more fields (command, resources, etc.), copy them here.
        let container_request = V1ContainerRequest {
            image: parsed_container.image,
            env: Some(env.clone()),
            command: parsed_container.command,
            args: parsed_container.args,
            volumes: parsed_container.volumes,
            accelerators: parsed_container.accelerators,
            meters: parsed_container.meters,
            resources: parsed_container.resources,
            health_check: parsed_container.health_check,
            restart: RestartPolicy::Always.to_string(), // TODO
            queue: parsed_container.queue,
            timeout: parsed_container.timeout,
            ssh_keys: parsed_container.ssh_keys,
            metadata: Some(V1ResourceMetaRequest {
                name: Some(format!("processor-{}", processor.name)),
                namespace: Some(processor.namespace.clone()),
                // The rest can be left empty if you only need partial data:
                owner: Some(processor.owner.clone()),
                labels: Some(labels),
                owner_ref: Some(processor.id.clone()),
            }),
            authz: parsed_container.authz,
            // Optional fields
            kind: "Container".to_string(),
            platform: Some(parsed_container.platform.clone()),
            ports: parsed_container.ports,
            proxy_port: parsed_container.proxy_port,
        };

        // 5) Create a ContainerPlatform — in this case, Runpod.
        let runpod = RunpodPlatform::new();

        // 6) For each replica, optionally modify the request with different names.
        //    Then declare the container with runpod, storing in DB + provisioning in RunPod.
        let owner_id = processor.owner.clone();
        let user_profile = V1UserProfile {
            email: processor
                .created_by
                .clone()
                .unwrap_or_else(|| "unknown@domain.tld".to_string()),
            ..Default::default()
        };

        for replica_index in 0..min_replicas {
            let mut request_for_replica = container_request.clone();
            if let Some(mut meta) = request_for_replica.metadata.take() {
                meta.name = Some(format!("{:?}-replica-{:?}", meta.name, replica_index));
                request_for_replica.metadata = Some(meta);
            }

            info!(
                "[Processor Controller] Creating container #{} for processor {}",
                replica_index, processor.id
            );

            let declared = runpod
                .declare(&request_for_replica, db, &user_profile, &owner_id)
                .await?;

            info!(
                "[Processor Controller] Created container {} (id = {}) for processor {}",
                declared.metadata.name, declared.metadata.id, processor.id
            );
        }

        // 7) Once all containers are declared, update Processor status to Created.
        Mutation::update_processor_status(
            db,
            processor.id,
            Some(ProcessorStatus::Created.to_string()),
            None,
        )
        .await?;

        Ok(())
    }

    /// Watch/monitor a processor (stubbed example).
    /// Watch/monitor a processor and scale containers based on Redis queue 'pressure'.
    async fn watch_processor(
        &self,
        db: &DatabaseConnection,
        processor: processors::Model,
        redis_client: &redis::Client,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        use tracing::info;

        // Make a connection from the client:
        let mut con = redis_client.get_connection()?;

        info!("[Processor Controller] Watching processor {}", processor.id);

        // 1) Make sure there's a stream name in the processor.
        //    We'll treat the processor's `stream` field as the Redis stream name.
        let Some(stream_name) = processor.stream.as_deref() else {
            info!(
                "[Processor Controller] Processor {} has no stream defined; skipping watch",
                processor.id
            );
            return Ok(());
        };

        // 2) The consumer group is the processor's ID.
        let consumer_group = &processor.id;

        // 3) Check how many messages are pending for this group in the Redis stream (i.e. 'pressure').
        //    This uses the `redis` crate’s XPending or XPendingCount functionality.
        //    Adjust the connection string or usage as necessary for your environment.
        let pending_count = match get_consumer_group_progress(&mut con, stream_name, consumer_group)
        {
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

        // Extract scale up/down thresholds.
        let scale_up_threshold = scale
            .up
            .as_ref()
            .and_then(|up| up.above_pressure)
            .unwrap_or(i32::MAX); // If none specified, we won’t scale up
        let scale_down_threshold = scale
            .down
            .as_ref()
            .and_then(|down| down.below_pressure)
            .unwrap_or(0); // If none, we won’t scale down

        // 5) Determine the current desired or min_replicas for this processor.
        //    If you store “current” replicas differently, adjust accordingly.
        let current_replicas = processor.min_replicas.unwrap_or(1);

        let mut new_replica_count = current_replicas;

        // Example scale-up check
        if (pending_count as i32) >= scale_up_threshold {
            // For demonstration, just scale up by 1.
            // You can parse `scale.up.rate` if you want a bigger jump.
            new_replica_count = current_replicas + 1;
            info!(
                "[Processor Controller] Scaling UP processor {} from {} -> {} replicas",
                processor.id, current_replicas, new_replica_count
            );
        }
        // Example scale-down check
        else if (pending_count as i32) <= scale_down_threshold && current_replicas > 1 {
            // For demonstration, scale down by 1
            new_replica_count = (current_replicas - 1).max(1);
            info!(
                "[Processor Controller] Scaling DOWN processor {} from {} -> {} replicas",
                processor.id, current_replicas, new_replica_count
            );
        }

        // 6) If the replica count changed, update DB, then reconcile or create/destroy containers as needed.
        if new_replica_count != current_replicas {
            // Update the processor's record in the DB. We'll set min_replicas to the new count;
            // for a real system, you might want separate “desired replicas” or another field.
            let mut active_model = processors::ActiveModel::from(processor.clone());
            active_model.desired_replicas = sea_orm::ActiveValue::Set(Some(new_replica_count));
            let updated_model = active_model.update(db).await?;

            info!(
                "[Processor Controller] Updated processor {} min_replicas to {} in DB",
                updated_model.id, new_replica_count
            );
        } else {
            info!(
                "[Processor Controller] No scale change for processor {}; replicas remain {}",
                processor.id, current_replicas
            );
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
    ) -> Result<V1Processor, Box<dyn std::error::Error + Send + Sync>> {
        // 1. Generate a unique ID for the new processor.
        let new_id = Uuid::new_v4().to_string();

        // 2. Create an ActiveModel to represent the new record in the database.
        let processor_am = processors::ActiveModel {
            // Primary fields
            id: Set(new_id),
            name: Set(config
                .metadata
                .name
                .clone()
                .unwrap_or(petname::petname(3, "-").unwrap())),
            namespace: Set(config
                .metadata
                .namespace
                .clone()
                .unwrap_or("default".to_string())),
            owner: Set(owner_id.to_string()),
            created_by: Set(Some(user_profile.email.clone())),

            // Any JSON fields from config (e.g., container & scale).
            // Adjust as needed depending on your actual request struct.
            container: Set(config
                .container
                .clone()
                .map(|c| serde_json::to_value(c))
                .transpose()?),
            scale: Set(config
                .scale
                .clone()
                .map(|s| serde_json::to_value(s))
                .transpose()?),
            labels: Set(config
                .metadata
                .labels
                .clone()
                .map(|l| serde_json::to_value(l))
                .transpose()?),

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
                        self.start_processor(db, processor.clone()).await?;
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
