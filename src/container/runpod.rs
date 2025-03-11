use crate::accelerator::base::AcceleratorProvider;
use crate::accelerator::runpod::RunPodProvider;
use crate::container::base::ContainerPlatform;
use sea_orm::{DatabaseConnection, Set, ActiveModelTrait};
use crate::models::{Container, ContainerRequest};
use petname;
use runpod::*;
use short_uuid::ShortUuid;
use serde_json::Value as Json;
use std::collections::HashMap;
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
    fn select_gpu_type(
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
        let volume_name = "nebu";
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
        pod_id: &str,
        container_id: &str,
        db: &DatabaseConnection,
    ) -> Result<(), Box<dyn std::error::Error>> {
        info!(
            "[RunPod] Starting to watch pod {} for container {}",
            pod_id, container_id
        );

        // Initial status check
        let mut last_status = String::new();
        let mut consecutive_errors = 0;
        const MAX_ERRORS: usize = 5;

        // Poll the pod status every 30 seconds
        loop {
            match self.runpod_client.get_pod(pod_id).await {
                Ok(pod_response) => {
                    consecutive_errors = 0;

                    if let Some(pod_info) = pod_response.data {
                        info!("[RunPod] Pod {} info: {:?}", pod_id, pod_info);
                        // Extract status information
                        let current_status = if let Some(runtime) = &pod_info.runtime {
                            if runtime.uptime_in_seconds.unwrap_or(0) > 0 {
                                "running".to_string()
                            } else {
                                "starting".to_string()
                            }
                        } else {
                            "pending".to_string()
                        };

                        // If status changed, update the database
                        if current_status != last_status {
                            info!(
                                "[RunPod] Pod {} status changed: {} -> {}",
                                pod_id, last_status, current_status
                            );
                            last_status = current_status.clone();

                            // Update the database with the new status using the Mutation struct
                            match crate::mutation::Mutation::update_container_status(
                                &db,
                                container_id.to_string(),
                                current_status.clone(),
                            )
                            .await
                            {
                                Ok(_) => {
                                    info!(
                                        "[RunPod] Updated container {} status to {}",
                                        container_id, current_status
                                    )
                                }
                                Err(e) => {
                                    error!(
                                        "[RunPod] Failed to update container status in database: {}",
                                        e
                                    )
                                }
                            }

                            // If the pod is in a terminal state, exit the loop
                            if current_status == "completed"
                                || current_status == "failed"
                                || current_status == "stopped"
                            {
                                info!(
                                    "[RunPod] Pod {} reached terminal state: {}",
                                    pod_id, current_status
                                );
                                break;
                            }
                        }
                    } else {
                        error!("[RunPod] No pod data returned for pod {}", pod_id);

                        // Check if pod was deleted or doesn't exist
                        match self.runpod_client.list_pods().await {
                            Ok(pods_list) => {
                                if let Some(my_pods) = pods_list.data {
                                    if !my_pods.pods.iter().any(|p| p.id == pod_id) {
                                        info!(
                                            "[RunPod] Pod {} no longer exists, marking job as failed",
                                            pod_id
                                        );

                                        // Update job as failed in database
                                        if let Err(e) =
                                            crate::mutation::Mutation::update_container_status(
                                                &db,
                                                container_id.to_string(),
                                                "failed".to_string(),
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
                            "failed".to_string(),
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
            "[RunPod] Finished watching pod {} for container {}",
            pod_id, container_id
        );
        Ok(())
    }
}

#[derive(Debug, Clone)]
struct GpuTypeWithDatacenter {
    id: String,
    memory_in_gb: Option<u32>,
    data_center_id: String,
}

impl ContainerPlatform for RunpodPlatform {
    /// Synchronously "run" a training job by provisioning a RunPod spot or on-demand pod.
    ///
    /// Because the original `TrainingPlatform` trait is not async, this function uses a
    /// Tokio runtime internally to block on the async `create_on_demand_pod` or `create_spot_pod`.
    fn run(&self, config: &ContainerRequest, db: &DatabaseConnection, owner_id: &str) -> Result<Container, Box<dyn std::error::Error>> {
        let name = config.name.clone().unwrap_or_else(|| {
            // Generate a random human-friendly name using petname
            petname::petname(3, "-").unwrap()
        });
        info!("[RunPod] Using name: {}", name);

        // Create a runtime to handle the async call
        let rt = tokio::runtime::Runtime::new()
            .map_err(|e| Box::<dyn std::error::Error>::from(format!("Failed to create Tokio runtime: {}", e)))?;

        // Get the list of available GPU types
        let gpu_types_response = rt.block_on(async {
            self.runpod_client
                .list_available_gpus_with_datacenters()
                .await
        })
        .map_err(|e| Box::<dyn std::error::Error>::from(format!("Error fetching GPU types: {:?}", e)))?;

        let mut runpod_gpu_type_id: String = "NVIDIA_TESLA_T4".to_string(); // Default value
        let mut requested_gpu_count = 1; // Default value
        let mut datacenter_id = String::from("US"); // Default value

        // Parse accelerators if provided
        if let Some(accelerators) = &config.accelerators {
            if !accelerators.is_empty() {
                // Parse the first accelerator in the list (format: "count:type")
                let parts: Vec<&str> = accelerators[0].split(':').collect();
                if parts.len() == 2 {
                    if let Ok(count) = parts[0].parse::<i32>() {
                        requested_gpu_count = count;
                    }

                    // Convert from our accelerator name to RunPod's GPU type ID
                    if let Some(runpod_gpu_name) = self.accelerator_map().get(parts[1]) {
                        runpod_gpu_type_id = runpod_gpu_name.clone();
                        info!(
                            "[RunPod] Using accelerator: {} (count: {})",
                            runpod_gpu_type_id, requested_gpu_count
                        );
                    } else {
                        error!(
                            "[RunPod] Unknown accelerator type: {}, cannot proceed",
                            parts[1]
                        );
                        return Err(Box::<dyn std::error::Error>::from(
                            format!("Unknown accelerator type: {}", parts[1])
                        ));
                    }
                }
            }
        }

        // Check if we got data back
        if let Some(datacenters) = gpu_types_response.data {
            // Collect all GPU types from all datacenters
            let mut all_gpu_types = Vec::new();

            info!("[RunPod] Available datacenters and GPU types:");
            for datacenter in &datacenters {
                info!(
                    "[RunPod] Datacenter: {} ({})",
                    datacenter.name, datacenter.id
                );

                // Remove the Option check since gpu_types is already a Vec
                for gpu_type in &datacenter.gpu_types {
                    info!(
                        "[Runpod]  ID: {}, Name: {}, Memory: {} GB",
                        gpu_type.id, gpu_type.display_name, gpu_type.memory_in_gb as u32
                    );

                    // Store GPU type with datacenter info
                    all_gpu_types.push(GpuTypeWithDatacenter {
                        id: gpu_type.id.clone(),
                        memory_in_gb: Some(gpu_type.memory_in_gb as u32),
                        data_center_id: datacenter.id.clone(),
                    });
                }
            }

            info!("[RunPod] Using GPU type: {}", runpod_gpu_type_id);

            // Determine datacenter ID based on selected GPU type
            if let Some(gpu_info) = all_gpu_types.iter().find(|g| g.id == runpod_gpu_type_id) {
                datacenter_id = gpu_info.data_center_id.clone();
            }
        } else if let Some(errors) = gpu_types_response.errors {
            let error_messages: Vec<String> = errors.iter().map(|e| e.message.clone()).collect();
            error!("[RunPod] Error fetching GPU types: {:?}", error_messages);
            return Err(Box::<dyn std::error::Error>::from(format!(
                "Error fetching GPU types: {:?}",
                error_messages
            )));
        }

        // Now call ensure_network_volumes with the datacenter ID
        let network_volume_id = rt.block_on(async {
            self.ensure_network_volumes(&datacenter_id).await
        })
        .map_err(|e| Box::<dyn std::error::Error>::from(format!("Failed to ensure network volume exists: {}", e)))?;

        let mut env_vec = Vec::new();

        for (key, value) in self.get_common_env_vars() {
            env_vec.push(runpod::EnvVar { key, value });
        }
        
        // Add ORIGN_SYNC_CONFIG environment variable with serialized volumes configuration
        if let Some(volumes) = &config.volumes {
            match serde_yaml::to_string(volumes) {
                Ok(serialized_volumes) => {
                    env_vec.push(runpod::EnvVar {
                        key: "ORIGN_SYNC_CONFIG".to_string(),
                        value: serialized_volumes,
                    });
                    info!("[RunPod] Added ORIGN_SYNC_CONFIG environment variable");
                }
                Err(e) => {
                    error!("[RunPod] Failed to serialize volumes configuration: {}", e);
                    // Continue without this env var rather than failing the whole operation
                }
            }
        }

        // Only add env_vars if they exist
        if let Some(env_vars) = &config.env_vars {
            for (key, value) in env_vars {
                env_vec.push(runpod::EnvVar {
                    key: key.clone(),
                    value: value.clone(),
                });
            }
        }
        let id = ShortUuid::generate().to_string();

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
            docker_args: config.command.clone(),
            ports: Some("8000".to_string()),
            volume_mount_path: Some("/cache/rclone".to_string()),
            env: env_vec,
            network_volume_id: Some(network_volume_id),
        };

        info!(
            "[RunPod] Creating on-demand pod with request: {:?}",
            create_request
        );


        // 6) Because our trait is non-async, we need to spawn a runtime to do `.await`.
        let rt = tokio::runtime::Runtime::new()
            .map_err(|e| Box::<dyn std::error::Error>::from(format!("Failed to create Tokio runtime: {}", e)))?;
        
        let pod_id = rt.block_on(async {
            // Attempt to create the on-demand pod
            match self
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
                            namespace: Set(config.namespace.clone().unwrap_or_else(|| "default".to_string())),
                            name: Set(name.clone()),
                            owner_id: Set(owner_id.to_string()),
                            image: Set(config.image.clone()),
                            env_vars: Set(config.env_vars.clone().map(|vars| serde_json::json!(vars))),
                            volumes: Set(config.volumes.clone().map(|vols| serde_json::json!(vols))),
                            accelerators: Set(config.accelerators.clone()),
                            cpu_request: Set(None),
                            memory_request: Set(None),
                            status: Set(Some("pending".to_string())),
                            platform: Set(Some("runpod".to_string())),
                            resource_name: Set(Some(pod.id.clone())),
                            resource_namespace: Set(None),
                            command: Set(config.command.clone()),
                            labels: Set(config.labels.clone().map(|labels| serde_json::json!(labels))),
                            created_by: Set(Some("runpod".to_string())),
                            updated_at: Set(chrono::Utc::now().into()),
                            created_at: Set(chrono::Utc::now().into()),
                        };

                        if let Err(e) = container.insert(db).await {
                            error!("[RunPod] Failed to create container in database: {:?}", e);
                            return Err(format!("Failed to create container in database: {:?}", e));
                        } else {
                            info!("[RunPod] Created container {} in database with RunPod pod ID {}", name, pod.id);
                        }

                        // Start watching the pod status in a separate task
                        let pod_id_clone = pod.id.clone();
                        let name_clone = name.clone();
                        let db_clone = db.clone();
                        let self_clone = self.clone();
                        
                        tokio::spawn(async move {
                            if let Err(e) = self_clone.watch_pod_status(&pod_id_clone, &name_clone, &db_clone).await {
                                error!("[RunPod] Error watching pod status: {:?}", e);
                            }
                        });

                        Ok(pod.id)
                    } else {
                        Err(format!("On-Demand Pod creation returned empty data for job '{}'", name))
                    }
                }
                Err(e) => {
                    Err(format!("Error creating on-demand pod on RunPod for '{}': {:?}", name, e))
                }
            }
        });

        // Handle any errors from the async block
        let pod_id = pod_id.map_err(|e| Box::<dyn std::error::Error>::from(e))?;

        info!("[RunPod] Job {} created on RunPod with pod ID {}", name, pod_id);
        
        Ok(Container {
            metadata: crate::models::ContainerMeta {
                id: id.clone(),
                owner_id: "runpod".to_string(),
                created_at: chrono::Utc::now().timestamp(),
                updated_at: chrono::Utc::now().timestamp(),
                created_by: "runpod".to_string(),
                labels: config.labels.clone(),
            },
            name: name,
            namespace: config.namespace.clone().unwrap_or_else(|| "default".to_string()),
            image: config.image.clone(),
            env_vars: config.env_vars.clone(),
            command: config.command.clone(),
            volumes: config.volumes.clone(),
            accelerators: config.accelerators.clone(),
        })
    }

    fn delete(&self, id: &str, db: &DatabaseConnection) -> Result<(), Box<dyn std::error::Error>> {
        
        
        info!("[RunPod] Attempting to delete container with name: {}", id);
        
        // Create a runtime to handle the async operations
        let rt = tokio::runtime::Runtime::new()?;
        
        rt.block_on(async {
            // First, list all pods to find the one with our name
            match self.runpod_client.list_pods().await {
                Ok(pods_response) => {
                    if let Some(my_pods) = pods_response.data {
                        // Find the pod with matching name
                        let pod_to_delete = my_pods.pods.iter().find(|p| p.name == id);
                        
                        if let Some(pod) = pod_to_delete {
                            info!("[RunPod] Found pod with ID: {} for container: {}", pod.id, id);
                            
                            // Stop the pod
                            match self.runpod_client.stop_pod(&pod.id).await {
                                Ok(_) => {
                                    info!("[RunPod] Successfully stopped pod: {}", pod.id);
                                    
                                    // Update container status in database
                                    if let Err(e) = crate::mutation::Mutation::update_container_status(
                                        &db,
                                        id.clone().to_string(),
                                        "stopped".to_string(),
                                    ).await {
                                        error!("[RunPod] Failed to update container status in database: {}", e);
                                        return Err(e.into());
                                    } else {
                                        info!("[RunPod] Updated container {} status to stopped", id);
                                    }
                                },
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
                },
                Err(e) => {
                    error!("[RunPod] Error listing pods: {}", e);
                    return Err(e.into());
                }
            }
            
            Ok(())
        })
    }

    fn accelerator_map(&self) -> HashMap<String, String> {
        let provider = crate::accelerator::runpod::RunPodProvider::new();
        provider.accelerator_map().clone()
    }
}
