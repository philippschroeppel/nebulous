use crate::accelerator::base::AcceleratorProvider;
use crate::accelerator::runpod::RunPodProvider;
use crate::container::base::{ContainerPlatform, ContainerStatus};
use crate::models::{V1Container, V1ContainerRequest, V1VolumeConfig, V1VolumePath};
use crate::volumes::rclone::{SymlinkConfig, VolumeConfig, VolumePath};
use petname;
use runpod::*;
use sea_orm::{ActiveModelTrait, DatabaseConnection, Set};
use short_uuid::ShortUuid;
use std::collections::HashMap;
use std::str::FromStr;
use tracing::{error, info};

/// A `TrainingPlatform` implementation that schedules training jobs on RunPod.
#[derive(Clone)]
pub struct RunpodPlatform {
    runpod_client: RunpodClient,
}

impl RunpodPlatform {
    pub fn new() -> Self {
        // Read the API key from environment variables
        let api_key = std::env::var("RUNPOD_API_KEY")
            .expect("[RunPod] Missing RUNPOD_API_KEY environment variable");

        RunpodPlatform {
            runpod_client: RunpodClient::new(api_key),
        }
    }

    /// Create a new RunpodPlatform with a specific API key
    pub fn with_api_key(api_key: String) -> Self {
        RunpodPlatform {
            runpod_client: RunpodClient::new(api_key),
        }
    }

    /// Select an appropriate GPU type based on VRAM request
    fn _select_gpu_type(
        &self,
        vram_request: &Option<u32>,
        gpu_types: &[GpuTypeWithDatacenter],
    ) -> Option<String> {
        // If no VRAM request, return None (will use default)
        let vram_gb = match vram_request {
            Some(vram) => *vram,
            None => return None,
        };

        // Find the smallest GPU that meets the VRAM requirement
        gpu_types
            .iter()
            .filter(|gpu| {
                if let Some(memory) = gpu.memory_in_gb {
                    memory >= vram_gb
                } else {
                    false
                }
            })
            .min_by_key(|gpu| gpu.memory_in_gb.unwrap_or(0))
            .map(|gpu| gpu.id.clone())
    }

    async fn ensure_network_volumes(
        &self,
        datacenter_id: &str,
    ) -> Result<String, Box<dyn std::error::Error>> {
        // Define the single required volume with its size
        let volume_name = "nebu"; // TODO: this needs to be per owner
        let size_gb = 500;

        // Get existing network volumes
        let existing_volumes = self.runpod_client.list_network_volumes().await?;

        // Extract existing volumes in the target datacenter
        let existing_volumes_in_dc: Vec<&NetworkVolume> = existing_volumes
            .iter()
            .filter(|vol| vol.data_center_id == datacenter_id)
            .collect();

        let existing_volume_names: Vec<String> = existing_volumes_in_dc
            .iter()
            .map(|vol| vol.name.clone())
            .collect();

        info!(
            "[RunPod] Existing network volumes in datacenter {}: {:?}",
            datacenter_id, existing_volume_names
        );

        // Check if our volume already exists
        let volume_id = if existing_volume_names.contains(&volume_name.to_string()) {
            info!(
                "[RunPod] Network volume '{}' already exists in datacenter {}",
                volume_name, datacenter_id
            );

            // Find and return the ID of the existing volume
            existing_volumes_in_dc
                .iter()
                .find(|vol| vol.name == volume_name)
                .map(|vol| vol.id.clone())
                .ok_or_else(|| format!("Volume '{}' exists but ID not found", volume_name))?
        } else {
            info!(
                "[RunPod] Creating network volume '{}' with size {}GB in datacenter {}",
                volume_name, size_gb, datacenter_id
            );

            // Create the network volume
            let create_request = NetworkVolumeCreateInput {
                name: volume_name.to_string(),
                size: size_gb,
                data_center_id: datacenter_id.to_string(),
            };

            match self
                .runpod_client
                .create_network_volume(create_request)
                .await
            {
                Ok(volume) => {
                    info!(
                        "[RunPod] Successfully created network volume '{}' (id = {})",
                        volume_name, volume.id
                    );
                    volume.id
                }
                Err(e) => {
                    error!(
                        "[RunPod] Error creating network volume '{}': {:?}",
                        volume_name, e
                    );
                    return Err(e.into());
                }
            }
        };

        Ok(volume_id)
    }

    /// Watch a pod and update its status in the database
    /// Watch a pod and update its status in the database
    pub async fn watch_pod_status(
        &self,
        container_id: &str,
        db: &DatabaseConnection,
    ) -> Result<(), Box<dyn std::error::Error>> {
        info!(
            "[RunPod] Starting to watch pod for container {}",
            container_id
        );

        // Get initial status from database
        let (mut last_status, resource_name) =
            match crate::query::Query::find_container_by_id(db, container_id.to_string()).await {
                Ok(container) => {
                    info!(
                        "[RunPod] Initial database container for {}: {:?}",
                        container_id, container
                    );
                    let status = container
                        .as_ref()
                        .and_then(|c| c.status.clone())
                        .map(|s| ContainerStatus::from_str(&s).unwrap_or(ContainerStatus::Pending));
                    let resource_name = container.as_ref().and_then(|c| c.resource_name.clone());

                    (status, resource_name)
                }
                Err(e) => {
                    error!(
                        "[RunPod] Error fetching initial container from database: {}",
                        e
                    );
                    (None, None)
                }
            };

        info!("[RunPod] Resource name: {:?}", resource_name);
        // Use resource_name from database if available, otherwise use the provided pod_id
        let pod_id_to_watch = resource_name.clone().unwrap();
        info!("[RunPod] Using pod ID for watching: {}", pod_id_to_watch);

        let mut consecutive_errors = 0;
        const MAX_ERRORS: usize = 5;

        // Poll the pod status every 20 seconds
        loop {
            match self.runpod_client.get_pod(&pod_id_to_watch).await {
                Ok(pod_response) => {
                    consecutive_errors = 0;

                    if let Some(pod_info) = pod_response.data {
                        info!("[RunPod] Pod {} info: {:?}", pod_id_to_watch, pod_info);

                        // Extract status information using desired_status field
                        let current_status = match pod_info.desired_status.as_str() {
                            // ... existing status mapping code ...
                            "RUNNING" => ContainerStatus::Running,
                            "EXITED" => ContainerStatus::Completed,
                            "TERMINATED" => ContainerStatus::Stopped,
                            "DEAD" => ContainerStatus::Failed,
                            "CREATED" => ContainerStatus::Defined,
                            "RESTARTING" => ContainerStatus::Restarting,
                            "PAUSED" => ContainerStatus::Paused,
                            _ => {
                                info!(
                                    "[RunPod] Unknown pod status: {}, defaulting to Pending",
                                    pod_info.desired_status
                                );
                                ContainerStatus::Pending
                            }
                        };

                        info!("[RunPod] Current RunPod status: {}", current_status);
                        info!("[RunPod] Last database status: {:?}", last_status);

                        // If status changed, update the database
                        if last_status.as_ref() != Some(&current_status) {
                            if let Some(last) = &last_status {
                                info!(
                                    "[RunPod] Pod {:?} status changed: {} -> {}",
                                    resource_name.clone(),
                                    last,
                                    current_status
                                );
                            } else {
                                info!(
                                    "[RunPod] Pod {:?} initial status: {}",
                                    resource_name, current_status
                                );
                            }

                            // Update the database with the new status using the Mutation struct
                            match crate::mutation::Mutation::update_container_status(
                                &db,
                                container_id.to_string(),
                                current_status.to_string(),
                            )
                            .await
                            {
                                Ok(_) => {
                                    info!(
                                        "[RunPod] Updated container {:?} status to {}",
                                        container_id, current_status
                                    );
                                    // Update last_status after successful database update
                                    last_status = Some(current_status.clone());
                                }
                                Err(e) => {
                                    error!(
                                    "[RunPod] Failed to update container status in database: {}",
                                    e
                                )
                                }
                            }

                            // If the pod is in a terminal state, exit the loop
                            match current_status {
                                ContainerStatus::Completed
                                | ContainerStatus::Failed
                                | ContainerStatus::Stopped
                                | ContainerStatus::Exited
                                | ContainerStatus::Paused => {
                                    info!(
                                        "[RunPod] Pod {:?} reached terminal state: {}",
                                        resource_name, current_status
                                    );
                                    break;
                                }
                                _ => {}
                            }
                        }
                    } else {
                        error!("[RunPod] No pod data returned for pod {:?}", resource_name);

                        // Check if pod was deleted or doesn't exist
                        match self.runpod_client.list_pods().await {
                            Ok(pods_list) => {
                                if let Some(my_pods) = pods_list.data {
                                    if !my_pods
                                        .pods
                                        .iter()
                                        .any(|p| &p.id == resource_name.as_ref().unwrap())
                                    {
                                        info!(
                                            "[RunPod] Pod {:?} no longer exists, marking job as failed",
                                            resource_name
                                        );

                                        // Update job as failed in database
                                        if let Err(e) =
                                            crate::mutation::Mutation::update_container_status(
                                                &db,
                                                container_id.to_string(),
                                                ContainerStatus::Failed.to_string(),
                                            )
                                            .await
                                        {
                                            error!(
                                                "[RunPod] Failed to update job status in database: {}",
                                                e
                                            );
                                        }

                                        break;
                                    }
                                }
                            }
                            Err(e) => error!("[RunPod] Error listing pods: {}", e),
                        }
                    }
                }
                Err(e) => {
                    error!("[RunPod] Error fetching pod status: {}", e);
                    consecutive_errors += 1;

                    // If we've had too many consecutive errors, mark the job as failed
                    if consecutive_errors >= MAX_ERRORS {
                        error!("[RunPod] Too many consecutive errors, marking job as failed");

                        if let Err(e) = crate::mutation::Mutation::update_container_status(
                            &db,
                            container_id.to_string(),
                            ContainerStatus::Failed.to_string(),
                        )
                        .await
                        {
                            error!(
                                "[RunPod] Failed to update container status in database: {}",
                                e
                            );
                        }

                        break;
                    }
                }
            }

            // Wait before checking again
            tokio::time::sleep(tokio::time::Duration::from_secs(20)).await;
        }

        info!(
            "[RunPod] Finished watching pod {:?} for container {}",
            resource_name, container_id
        );
        Ok(())
    }

    fn determin_volumes_config(model: Vec<V1VolumePath>) -> VolumeConfig {
        let mut volume_paths = Vec::new();
        let mut symlinks = Vec::new();
        let cache_dir = "/nebu/cache".to_string();

        for path in model {
            // Check if destination path is local (not starting with s3:// or other remote protocol)
            let is_local_destination = !path.destination_path.starts_with("s3://")
                && !path.destination_path.starts_with("gs://")
                && !path.destination_path.starts_with("azure://");

            let destination_path = if is_local_destination {
                // For local paths, we'll sync to cache directory instead
                let path_without_leading_slash = path.destination_path.trim_start_matches('/');
                let cache_path = format!("{}/{}", cache_dir, path_without_leading_slash);

                // Add a symlink from the cache path to the original destination path
                symlinks.push(SymlinkConfig {
                    source_path: cache_path.clone(),
                    symlink_path: path.destination_path.clone(),
                });

                cache_path
            } else {
                path.destination_path
            };

            let volume_path = VolumePath {
                source_path: path.source_path,
                destination_path: destination_path,
                resync: path.resync,
                bidirectional: path.bidirectional,
                continuous: path.continuous,
            };
            volume_paths.push(volume_path);
        }

        let volume_config = VolumeConfig {
            paths: volume_paths,
            cache_dir: cache_dir,
            symlinks: symlinks,
        };
        volume_config
    }
}

#[derive(Debug, Clone)]
struct GpuTypeWithDatacenter {
    id: String,
    memory_in_gb: Option<u32>,
    data_center_id: String,
}

impl ContainerPlatform for RunpodPlatform {
    /// Asynchronously run a container by provisioning a RunPod spot or on-demand pod.
    async fn run(
        &self,
        config: &V1ContainerRequest,
        db: &DatabaseConnection,
        owner_id: &str,
    ) -> Result<V1Container, Box<dyn std::error::Error>> {
        let name = config
            .metadata
            .as_ref()
            .and_then(|meta| meta.name.clone())
            .unwrap_or_else(|| {
                // Generate a random human-friendly name using petname
                petname::petname(3, "-").unwrap()
            });
        info!("[RunPod] Using name: {}", name);
        let gpu_types_response = match self.runpod_client.list_gpu_types_graphql().await {
            Ok(response) => response,
            Err(e) => {
                error!("[RunPod] Error fetching GPU types: {:?}", e);

                // More detailed error information
                if let Some(status) = e.status() {
                    error!("[RunPod] HTTP Status: {}", status);
                }

                return Err(Box::<dyn std::error::Error>::from(format!(
                    "Error fetching GPU types: {:?}",
                    e
                )));
            }
        };

        let mut runpod_gpu_type_id: String = "NVIDIA_TESLA_T4".to_string(); // Default value
        let mut requested_gpu_count = 1; // Default value
        let mut datacenter_id = String::from("US"); // Default value
        let mut available_gpu_types = Vec::new();

        // Extract available GPU types from the response
        if let Some(data) = &gpu_types_response.data {
            // Log the available GPU types
            for gpu_type in data {
                available_gpu_types.push(gpu_type.id.clone());

                // Log GPU type details
                let memory_str = match gpu_type.memory_in_gb {
                    Some(mem) => format!("{} GB", mem),
                    None => "Unknown".to_string(),
                };

                info!(
                    "[RunPod] GPU Type: {}, Display Name: {}, Memory: {}",
                    gpu_type.id, gpu_type.display_name, memory_str
                );
            }
            info!("[RunPod] Available GPU types: {:?}", available_gpu_types);
        }
        info!("[RunPod] GPU types response: {:?}", gpu_types_response);

        // Parse accelerators if provided
        if let Some(accelerators) = &config.accelerators {
            if !accelerators.is_empty() {
                let mut found_valid_accelerator = false;

                // Try each accelerator in the list until we find one that works
                for accelerator in accelerators {
                    // Parse the accelerator (format: "count:type")
                    info!("[RunPod] Accelerator: {}", accelerator);
                    let parts: Vec<&str> = accelerator.split(':').collect();
                    if parts.len() == 2 {
                        if let Ok(count) = parts[0].parse::<i32>() {
                            // Convert from our accelerator name to RunPod's GPU type ID
                            if let Some(runpod_gpu_name) = self.accelerator_map().get(parts[1]) {
                                info!("[RunPod] RunPod GPU name: {}", runpod_gpu_name);
                                // Check if this GPU type is available on RunPod
                                if available_gpu_types.is_empty()
                                    || available_gpu_types.contains(runpod_gpu_name)
                                {
                                    // This accelerator is available, use it
                                    requested_gpu_count = count;
                                    runpod_gpu_type_id = runpod_gpu_name.clone();
                                    found_valid_accelerator = true;

                                    info!(
                                        "[RunPod] Using accelerator: {} (count: {})",
                                        runpod_gpu_type_id, requested_gpu_count
                                    );

                                    // We found a valid accelerator, stop looking
                                    break;
                                } else {
                                    info!(
                                        "[RunPod] Accelerator type '{}' is not available, trying next option",
                                        runpod_gpu_name
                                    );
                                }
                            } else {
                                info!(
                                    "[RunPod] Unknown accelerator type: {}, trying next option",
                                    parts[1]
                                );
                            }
                        }
                    }
                }

                // If we couldn't find any valid accelerator, return an error
                if !found_valid_accelerator {
                    error!(
                        "[RunPod] None of the requested accelerator types are available. Available types: {:?}",
                        available_gpu_types
                    );
                    return Err(Box::<dyn std::error::Error>::from(
                        "None of the requested accelerator types are available on RunPod"
                            .to_string(),
                    ));
                }
            }
        }

        // Check if we got data back
        // if let Some(datacenters) = gpu_types_response.data {
        //     // Collect all GPU types from all datacenters
        //     let mut all_gpu_types = Vec::new();

        //     info!("[RunPod] Available datacenters and GPU types:");
        //     for datacenter in &datacenters {
        //         info!(
        //             "[RunPod] Datacenter: {} ({})",
        //             datacenter.name, datacenter.id
        //         );

        //         // Remove the Option check since gpu_types is already a Vec
        //         for gpu_type in &datacenter.gpu_types {
        //             info!(
        //                 "[Runpod]  ID: {}, Name: {}, Memory: {} GB",
        //                 gpu_type.id, gpu_type.display_name, gpu_type.memory_in_gb as u32
        //             );

        //             // Store GPU type with datacenter info
        //             all_gpu_types.push(GpuTypeWithDatacenter {
        //                 id: gpu_type.id.clone(),
        //                 memory_in_gb: Some(gpu_type.memory_in_gb as u32),
        //                 data_center_id: datacenter.id.clone(),
        //             });
        //         }
        //     }

        //     info!("[RunPod] Using GPU type: {}", runpod_gpu_type_id);

        //     // Determine datacenter ID based on selected GPU type
        //     if let Some(gpu_info) = all_gpu_types.iter().find(|g| g.id == runpod_gpu_type_id) {
        //         datacenter_id = gpu_info.data_center_id.clone();
        //     }
        // } else if let Some(errors) = gpu_types_response.errors {
        //     let error_messages: Vec<String> = errors.iter().map(|e| e.message.clone()).collect();
        //     error!("[RunPod] GraphQL errors: {:?}", error_messages);
        //     return Err(Box::<dyn std::error::Error>::from(format!(
        //         "Error fetching GPU types: GraphQL errors: {:?}",
        //         error_messages
        //     )));
        // } else {
        //     error!("[RunPod] No data or errors returned from GPU types query");
        //     return Err(Box::<dyn std::error::Error>::from(
        //         "Error fetching GPU types: No data or errors returned",
        //     ));
        // }

        // Now call ensure_network_volumes with the datacenter ID - directly await
        // let network_volume_id = self
        //     .ensure_network_volumes(&datacenter_id)
        //     .await
        //     .map_err(|e| {
        //         Box::<dyn std::error::Error>::from(format!(
        //             "Failed to ensure network volume exists: {}",
        //             e
        //         ))
        //     })?;

        let mut env_vec = Vec::new();

        for (key, value) in self.get_common_env_vars() {
            env_vec.push(runpod::EnvVar { key, value });
        }

        // Add NEBU_SYNC_CONFIG environment variable with serialized volumes configuration
        if let Some(volumes) = &config.volumes {
            let volume_config = RunpodPlatform::determin_volumes_config(volumes.clone());
            match serde_yaml::to_string(&volume_config) {
                Ok(serialized_volumes) => {
                    env_vec.push(runpod::EnvVar {
                        key: "NEBU_SYNC_CONFIG".to_string(),
                        value: serialized_volumes,
                    });
                    info!("[RunPod] Added NEBU_SYNC_CONFIG environment variable");
                }
                Err(e) => {
                    error!("[RunPod] Failed to serialize volumes configuration: {}", e);
                    // Continue without this env var rather than failing the whole operation
                }
            }
        }
        info!("[RunPod] Environment variables: {:?}", env_vec);

        // Only add env_vars if they exist
        if let Some(env_vars) = &config.env_vars {
            for env_var in env_vars {
                env_vec.push(runpod::EnvVar {
                    key: env_var.key.clone(),
                    value: env_var.value.clone(),
                });
            }
        }
        info!("[RunPod] Environment variables: {:?}", env_vec);
        let id = ShortUuid::generate().to_string();
        info!("[RunPod] ID: {}", id);

        let docker_command = config.command.clone().map(|cmd| format!("nebu sync --config /nebu/sync.yaml --interval-seconds 5 --create-if-missing --watch --background --block-once --config-from-env && {}", cmd));
        info!("[RunPod] Docker command: {:?}", docker_command);

        // 5) Create an on-demand instance instead of a spot instance
        let create_request = CreateOnDemandPodRequest {
            cloud_type: Some("SECURE".to_string()),
            gpu_count: Some(requested_gpu_count),
            volume_in_gb: Some(500),
            container_disk_in_gb: Some(1000),
            min_vcpu_count: Some(8),
            min_memory_in_gb: Some(30),
            gpu_type_id: Some(runpod_gpu_type_id),
            name: Some(id.clone()),
            image_name: Some(config.image.clone()),
            docker_args: None,
            docker_entrypoint: docker_command.clone(),
            ports: Some("8000".to_string()),
            // volume_mount_path: Some("/nebu/cache".to_string()),
            volume_mount_path: None,
            env: env_vec,
            // network_volume_id: Some(network_volume_id),
            network_volume_id: None,
        };

        info!(
            "[RunPod] Creating on-demand pod with request: {:?}",
            create_request
        );

        // Attempt to create the on-demand pod - directly await
        let pod_id = match self
            .runpod_client
            .create_on_demand_pod(create_request)
            .await
        {
            Ok(resp) => {
                if let Some(pod) = resp.data {
                    info!(
                        "[RunPod] Successfully created On-Demand Pod '{}' (id = {}) on RunPod!",
                        name, pod.id
                    );

                    // Create the container record in the database
                    let container = crate::entities::containers::ActiveModel {
                        id: Set(id.clone()),
                        namespace: Set(config
                            .metadata
                            .as_ref()
                            .and_then(|meta| meta.namespace.clone())
                            .unwrap_or_else(|| "default".to_string())),
                        name: Set(name.clone()),
                        owner_id: Set(owner_id.to_string()),
                        image: Set(config.image.clone()),
                        env_vars: Set(config.env_vars.clone().map(|vars| serde_json::json!(vars))),
                        volumes: Set(config.volumes.clone().map(|vols| serde_json::json!(vols))),
                        accelerators: Set(config.accelerators.clone()),
                        cpu_request: Set(None),
                        memory_request: Set(None),
                        status: Set(Some(ContainerStatus::Defined.to_string())),
                        platform: Set(Some("runpod".to_string())),
                        meters: Set(config
                            .meters
                            .clone()
                            .map(|meters| serde_json::json!(meters))),
                        resource_name: Set(Some(pod.id.clone())),
                        resource_namespace: Set(None),
                        command: Set(config.command.clone()),
                        labels: Set(config.metadata.as_ref().and_then(|meta| {
                            meta.labels.clone().map(|labels| serde_json::json!(labels))
                        })),
                        restart: Set(config.restart.clone()),
                        public_ip: Set(None),
                        private_ip: Set(None),
                        created_by: Set(Some(owner_id.to_string())),
                        updated_at: Set(chrono::Utc::now().into()),
                        created_at: Set(chrono::Utc::now().into()),
                    };

                    if let Err(e) = container.insert(db).await {
                        error!("[RunPod] Failed to create container in database: {:?}", e);
                        return Err(
                            format!("Failed to create container in database: {:?}", e).into()
                        );
                    } else {
                        info!(
                            "[RunPod] Created container {} in database with RunPod pod ID {}",
                            name, pod.id
                        );
                    }

                    // Start watching the pod status in a separate task
                    let id_clone = id.clone();
                    let db_clone = db.clone();
                    let self_clone = self.clone();

                    tokio::spawn(async move {
                        if let Err(e) = self_clone.watch_pod_status(&id_clone, &db_clone).await {
                            error!("[RunPod] Error watching pod status: {:?}", e);
                        }
                    });

                    pod.id
                } else {
                    return Err(format!(
                        "On-Demand Pod creation returned empty data for job '{}'",
                        name
                    )
                    .into());
                }
            }
            Err(e) => {
                return Err(format!(
                    "Error creating on-demand pod on RunPod for '{}': {:?}",
                    name, e
                )
                .into());
            }
        };

        info!(
            "[RunPod] Job {} created on RunPod with pod ID {}",
            name, pod_id
        );

        Ok(V1Container {
            kind: "Container".to_string(),
            metadata: crate::models::V1ContainerMeta {
                name: name,
                namespace: config
                    .metadata
                    .as_ref()
                    .and_then(|meta| meta.namespace.clone())
                    .unwrap_or_else(|| "default".to_string()),
                id: id.clone(),
                owner_id: owner_id.to_string(),
                created_at: chrono::Utc::now().timestamp(),
                updated_at: chrono::Utc::now().timestamp(),
                created_by: owner_id.to_string(),
                labels: config
                    .metadata
                    .as_ref()
                    .and_then(|meta| meta.labels.clone()),
            },
            image: config.image.clone(),
            env_vars: config.env_vars.clone(),
            command: config.command.clone(),
            volumes: config.volumes.clone(),
            accelerators: config.accelerators.clone(),
            meters: config.meters.clone(),
            status: Some("pending".to_string()),
            restart: config.restart.clone(),
        })
    }

    async fn delete(
        &self,
        id: &str,
        db: &DatabaseConnection,
    ) -> Result<(), Box<dyn std::error::Error>> {
        info!("[RunPod] Attempting to delete container with name: {}", id);

        // First, list all pods to find the one with our name
        match self.runpod_client.list_pods().await {
            Ok(pods_response) => {
                if let Some(my_pods) = pods_response.data {
                    // Find the pod with matching name
                    let pod_to_delete = my_pods.pods.iter().find(|p| p.name == id);

                    if let Some(pod) = pod_to_delete {
                        info!(
                            "[RunPod] Found pod with ID: {} for container: {}",
                            pod.id, id
                        );

                        // Stop the pod
                        match self.runpod_client.delete_pod(&pod.id).await {
                            Ok(_) => {
                                info!("[RunPod] Successfully stopped pod: {}", pod.id);

                                // Update container status in database
                                if let Err(e) = crate::mutation::Mutation::update_container_status(
                                    &db,
                                    id.clone().to_string(),
                                    "stopped".to_string(),
                                )
                                .await
                                {
                                    error!("[RunPod] Failed to update container status in database: {}", e);
                                    return Err(e.into());
                                } else {
                                    info!("[RunPod] Updated container {} status to stopped", id);
                                }
                            }
                            Err(e) => {
                                error!("[RunPod] Failed to stop pod {}: {}", pod.id, e);
                                return Err(e.into());
                            }
                        }
                    } else {
                        info!("[RunPod] No pod found with name: {}", id);
                    }
                } else {
                    error!("[RunPod] No pods data returned from RunPod API");
                    return Err("No pods data returned from RunPod API".into());
                }
            }
            Err(e) => {
                error!("[RunPod] Error listing pods: {}", e);
                return Err(e.into());
            }
        }

        Ok(())
    }

    fn accelerator_map(&self) -> HashMap<String, String> {
        let provider = RunPodProvider::new();
        provider.accelerator_map().clone()
    }
}
