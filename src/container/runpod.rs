use crate::accelerator::base::AcceleratorProvider;
use crate::accelerator::runpod::RunPodProvider;
use crate::container::base::{ContainerPlatform, ContainerStatus};
use crate::entities::containers;
use crate::models::{
    RestartPolicy, V1Container, V1ContainerRequest, V1ContainerStatus, V1Meter, V1UserProfile,
    V1VolumeConfig, V1VolumePath,
};
use crate::mutation::Mutation;
use crate::volumes::rclone::{SymlinkConfig, VolumeConfig, VolumePath};
use petname;
use reqwest::{Error, StatusCode};
use runpod::*;
use sea_orm::{ActiveModelTrait, DatabaseConnection, Set};
use shell_quote::{Bash, QuoteRefExt};
use short_uuid::ShortUuid;
use std::collections::HashMap;
use std::str::FromStr;
use std::time::Duration;
use tracing::{debug, error, info};

/// A `TrainingPlatform` implementation that schedules training jobs on RunPod.
#[derive(Clone)]
pub struct RunpodPlatform {
    runpod_client: RunpodClient,
}

impl RunpodPlatform {
    pub fn new() -> Self {
        // Read the API key from environment variables
        let api_key = std::env::var("RUNPOD_API_KEY")
            .expect("[Runpod Controller] Missing RUNPOD_API_KEY environment variable");

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
            "[Runpod Controller] Existing network volumes in datacenter {}: {:?}",
            datacenter_id, existing_volume_names
        );

        // Check if our volume already exists
        let volume_id = if existing_volume_names.contains(&volume_name.to_string()) {
            info!(
                "[Runpod Controller] Network volume '{}' already exists in datacenter {}",
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
                "[Runpod Controller] Creating network volume '{}' with size {}GB in datacenter {}",
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
                        "[Runpod Controller] Successfully created network volume '{}' (id = {})",
                        volume_name, volume.id
                    );
                    volume.id
                }
                Err(e) => {
                    error!(
                        "[Runpod Controller] Error creating network volume '{}': {:?}",
                        volume_name, e
                    );
                    return Err(e.into());
                }
            }
        };

        Ok(volume_id)
    }

    /// Report metrics to OpenMeter for a running container
    async fn report_meters(&self, container_id: String, seconds: u64, meters: &serde_json::Value) {
        // Parse the meters from the container model
        let meters_vec: Vec<V1Meter> = match serde_json::from_value(meters.clone()) {
            Ok(parsed) => parsed,
            Err(e) => {
                error!("[Runpod Controller] Failed to parse meters: {}", e);
                return;
            }
        };

        if meters_vec.is_empty() {
            return;
        }

        // Get OpenMeter configuration from environment
        let openmeter_url = match std::env::var("OPENMETER_URL") {
            Ok(url) => url,
            Err(_) => {
                error!("[Runpod Controller] OPENMETER_URL environment variable not set");
                return;
            }
        };

        let openmeter_token = match std::env::var("OPENMETER_TOKEN") {
            Ok(token) => token,
            Err(_) => {
                error!("[Runpod Controller] OPENMETER_TOKEN environment variable not set");
                return;
            }
        };

        // Create OpenMeter client
        let meter_client = openmeter::MeterClient::new(openmeter_url, openmeter_token);

        // Create and send events for each meter
        for meter in meters_vec {
            let event_id = format!("container-{}-{}", container_id, uuid::Uuid::new_v4());

            // Create event data based on meter type
            let data = match meter.metric.as_str() {
                "runtime" => {
                    // For runtime, we'll report 1 unit (e.g., 1 minute of runtime)
                    serde_json::json!({
                        "value": seconds as f64,
                        "metric": meter.metric,
                        "container_id": container_id,
                        "currency": meter.currency,
                        "cost": meter.cost
                    })
                }
                // Add other metric types as needed
                _ => {
                    serde_json::json!({
                        "value": seconds as f64,
                        "metric": meter.metric,
                        "container_id": container_id,
                        "currency": meter.currency,
                        "cost": meter.cost
                    })
                }
            };

            // Create CloudEvent
            let cloud_event = openmeter::CloudEvent {
                id: event_id,
                source: "nebulous-runpod-controller".to_string(),
                specversion: "1.0".to_string(),
                r#type: meter.metric.clone(),
                subject: container_id.clone(),
                time: Some(chrono::Utc::now().to_rfc3339()),
                dataschema: None,
                datacontenttype: Some("application/json".to_string()),
                data: Some(data),
            };

            // Send the event to OpenMeter
            match meter_client.ingest_events(&[cloud_event]).await {
                Ok(_) => {
                    debug!(
                        "[Runpod Controller] Successfully reported meter {} for container {}",
                        meter.metric, container_id
                    );
                }
                Err(e) => {
                    error!(
                        "[Runpod Controller] Failed to report meter {} for container {}: {}",
                        meter.metric, container_id, e
                    );
                }
            }
        }
    }

    /// Watch a pod and update its status in the database
    pub async fn watch(
        &self,
        db: &DatabaseConnection,
        container: containers::Model,
    ) -> Result<(), Box<dyn std::error::Error>> {
        info!(
            "[Runpod Controller] Starting to watch pod for container {}",
            container.id
        );
        let container_id = container.id.to_string();
        let pause_seconds = 5;
        let duration = Duration::from_secs(pause_seconds);

        // Get initial status from database
        let (mut last_status, resource_name) =
            match crate::query::Query::find_container_by_id(db, container_id.to_string()).await {
                Ok(container) => {
                    info!(
                        "[Runpod Controller] Initial database container for {}: {:?}",
                        container_id, container
                    );

                    // Fix: Parse the status JSON properly
                    let status =
                        container
                            .as_ref()
                            .and_then(|c| c.status.clone())
                            .and_then(|status_json| {
                                // Try to extract the status field from the JSON
                                if let Some(status_obj) = status_json.as_object() {
                                    if let Some(status_value) = status_obj.get("status") {
                                        if let Some(status_str) = status_value.as_str() {
                                            return Some(
                                                ContainerStatus::from_str(status_str)
                                                    .unwrap_or(ContainerStatus::Pending),
                                            );
                                        }
                                    }
                                }
                                None
                            });

                    let resource_name = container.as_ref().and_then(|c| c.resource_name.clone());

                    (status, resource_name)
                }
                Err(e) => {
                    error!(
                        "[Runpod Controller] Error fetching initial container from database: {}",
                        e
                    );
                    (None, None)
                }
            };

        info!("[Runpod Controller] Resource name: {:?}", resource_name);
        // Use resource_name from database if available, otherwise use the provided pod_id
        let pod_id_to_watch = resource_name.clone().unwrap_or_default();
        info!(
            "[Runpod Controller] Using pod ID for watching: {}",
            pod_id_to_watch
        );

        let mut consecutive_errors = 0;
        const MAX_ERRORS: usize = 5;

        // Poll the pod status every 20 seconds
        let mut iteration_count = 0;
        loop {
            iteration_count += 1;
            debug!(
                "[DEBUG:runpod.rs:watch] container={} iteration={}",
                container_id, iteration_count
            );

            match self.runpod_client.get_pod(&pod_id_to_watch).await {
                Ok(pod_response) => {
                    debug!(
                        "[DEBUG:runpod.rs:watch] container={} got pod_response: {:?}",
                        container_id, pod_response
                    );
                    consecutive_errors = 0;

                    if let Some(pod_info) = pod_response.data {
                        // Extract status information using desired_status field
                        let current_status = match pod_info.desired_status.as_str() {
                            "RUNNING" => ContainerStatus::Running,
                            "EXITED" => ContainerStatus::Completed,
                            "TERMINATED" => ContainerStatus::Stopped,
                            "DEAD" => ContainerStatus::Failed,
                            "CREATED" => ContainerStatus::Defined,
                            "RESTARTING" => ContainerStatus::Restarting,
                            "PAUSED" => ContainerStatus::Paused,
                            _ => {
                                info!(
                                    "[Runpod Controller] Unknown pod status: {}, defaulting to Pending",
                                    pod_info.desired_status
                                );
                                ContainerStatus::Pending
                            }
                        };

                        // Handle metering if container has meters defined and status is Running
                        if current_status == ContainerStatus::Running {
                            if let Some(meters) = &container.meters {
                                self.report_meters(container_id.clone(), pause_seconds, meters)
                                    .await;
                            }
                        }

                        info!(
                            "[Runpod Controller] Current RunPod status: {}",
                            current_status
                        );
                        info!(
                            "[Runpod Controller] Last database status: {:?}",
                            last_status
                        );

                        // If status changed, update the database
                        if last_status.as_ref() != Some(&current_status) {
                            if let Some(last) = &last_status {
                                info!(
                                    "[Runpod Controller] Pod {:?} status changed: {} -> {}",
                                    resource_name.clone(),
                                    last,
                                    current_status
                                );
                            } else {
                                info!(
                                    "[Runpod Controller] Pod {:?} initial status: {}",
                                    resource_name, current_status
                                );
                            }

                            // Update the database with the new status using the Mutation struct
                            match crate::mutation::Mutation::update_container_status(
                                db,
                                container_id.to_string(),
                                Some(current_status.to_string()),
                                None,
                                None,
                                pod_info.public_ip.clone(),
                            )
                            .await
                            {
                                Ok(_) => {
                                    info!(
                                        "[Runpod Controller] Updated container {:?} status to {}",
                                        container_id, current_status
                                    );
                                    // Update last_status after successful database update
                                    last_status = Some(current_status.clone());
                                }
                                Err(e) => {
                                    error!(
                                    "[Runpod Controller] Failed to update container status in database: {}",
                                    e
                                )
                                }
                            }

                            match crate::mutation::Mutation::update_container_pod_ip(
                                db,
                                container_id.to_string(),
                                pod_info.public_ip.clone(),
                            )
                            .await
                            {
                                Ok(_) => {},
                                Err(e) => error!("[Runpod Controller] Failed to update container pod IP in database: {}", e),
                            }

                            // If the pod is in a terminal state, exit the loop
                            if current_status.is_inactive() {
                                info!(
                                    "[Runpod Controller] Pod {:?} reached terminal state: {}",
                                    resource_name, current_status
                                );
                                break;
                            }
                        }
                    } else {
                        error!(
                            "[Runpod Controller] No pod data returned for pod {:?}",
                            resource_name
                        );

                        // Check if pod was deleted or doesn't exist
                        match self.runpod_client.list_pods().await {
                            Ok(pods_list) => {
                                if let Some(my_pods) = pods_list.data {
                                    info!("[Runpod Controller] My pods: {:?}", my_pods);
                                    if !my_pods
                                        .pods
                                        .iter()
                                        .any(|p| &p.id == resource_name.as_ref().unwrap())
                                    {
                                        info!(
                                            "[Runpod Controller] Pod {:?} no longer exists, marking job as completed",
                                            resource_name
                                        );

                                        // Update job as failed in database
                                        if let Some(ref current_status) = last_status {
                                            // If current status is non-terminal, allow an update to Completed
                                            if !current_status.is_inactive() {
                                                if let Err(e) = crate::mutation::Mutation::update_container_status(
                                                    db,
                                                    container_id.to_string(),
                                                    Some(ContainerStatus::Completed.to_string()),
                                                    Some("Pod no longer exists".to_string()),
                                                    None,
                                                    None,
                                                )
                                                .await
                                                {
                                                    error!(
                                                        "[Runpod Controller] Failed to update job status in database: {}",
                                                        e
                                                    );
                                                }
                                            } else {
                                                // Already in a terminal state—don't overwrite it
                                                info!(
                                                    "[Runpod Controller] Container {} is already in terminal state ({:?}); not updating.",
                                                    container_id, current_status
                                                );
                                            }
                                        } else {
                                            // If we have no prior status, treat it as if it’s not terminal
                                            if let Err(e) =
                                                crate::mutation::Mutation::update_container_status(
                                                    db,
                                                    container_id.to_string(),
                                                    Some(ContainerStatus::Completed.to_string()),
                                                    Some("Pod no longer exists".to_string()),
                                                    None,
                                                    None,
                                                )
                                                .await
                                            {
                                                error!(
                                                    "[Runpod Controller] Failed to update job status in database: {}",
                                                    e
                                                );
                                            }
                                        }

                                        break;
                                    }
                                }
                            }
                            Err(e) => error!("[Runpod Controller] Error listing pods: {}", e),
                        }
                    }
                }
                // ------------------------------------------------
                // handle 404 not found
                // ------------------------------------------------
                Err(e) if is_not_found(&e) => {
                    error!(
                        "[Runpod Controller] Pod {} not found (404). Assuming it is deleted.",
                        pod_id_to_watch
                    );

                    // Check if container still exists in DB
                    match crate::query::Query::find_container_by_id(db, container_id.to_string())
                        .await
                    {
                        Ok(Some(_container)) => {
                            // If the container row is still there, mark it "Stopped" or "Deleted"
                            error!(
                                "[Runpod Controller] Marking container {} as stopped",
                                container_id
                            );
                            if let Err(update_err) =
                                crate::mutation::Mutation::update_container_status(
                                    db,
                                    container_id.to_string(),
                                    Some(ContainerStatus::Stopped.to_string()),
                                    Some("Pod was deleted or does not exist.".to_string()),
                                    None,
                                    None,
                                )
                                .await
                            {
                                error!(
                                    "[Runpod Controller] Failed to update container status in database: {}",
                                    update_err
                                );
                            }
                        }
                        Ok(None) => {
                            info!(
                                "[Runpod Controller] Container {} not found in DB (already removed). Nothing to update.",
                                container_id
                            );
                        }
                        Err(db_err) => {
                            error!(
                                "[Runpod Controller] Failed checking DB for container {}: {}",
                                container_id, db_err
                            );
                        }
                    }

                    // Either way, we can exit this watch since the RunPod resource does not exist
                    break;
                }
                Err(e) => {
                    error!("[Runpod Controller] Error fetching pod status: {}", e);
                    consecutive_errors += 1;

                    // If we've had too many consecutive errors, mark the job as failed
                    if consecutive_errors >= MAX_ERRORS {
                        error!(
                            "[Runpod Controller] Too many consecutive errors, marking container {} as failed",
                            container_id
                        );

                        if let Err(e) = crate::mutation::Mutation::update_container_status(
                            db,
                            container_id.to_string(),
                            Some(ContainerStatus::Failed.to_string()),
                            Some("Too many consecutive errors".to_string()),
                            None,
                            None,
                        )
                        .await
                        {
                            error!(
                                "[Runpod Controller] Failed to update container status in database: {}",
                                e
                            );
                        }

                        break;
                    }
                }
            }
            debug!(
                "[DEBUG:runpod.rs:watch] container={} iteration={} sleeping 20s",
                container_id, iteration_count
            );
            // Wait before checking again
            tokio::time::sleep(duration).await;
        }

        // Unreachable if loop never breaks. If you do break eventually:
        debug!(
            "[DEBUG:runpod.rs:watch] Exiting watch for container {}",
            container_id
        );
        Ok(())
    }

    fn determine_volumes_config(model: Vec<V1VolumePath>) -> VolumeConfig {
        let mut volume_paths = Vec::new();
        let mut symlinks = Vec::new();
        let cache_dir = "/nebu/cache".to_string();

        for path in model {
            // Check if destination path is local (not starting with s3:// or other remote protocol)
            let is_local_destination = !path.dest.starts_with("s3://")
                && !path.dest.starts_with("gs://")
                && !path.dest.starts_with("azure://");

            let dest = if is_local_destination {
                // For local paths, we'll sync to cache directory instead
                let path_without_leading_slash = path.dest.trim_start_matches('/');
                let cache_path = format!("{}/{}", cache_dir, path_without_leading_slash);

                // Add a symlink from the cache path to the original destination path
                symlinks.push(SymlinkConfig {
                    source: cache_path.clone(),
                    symlink_path: path.dest.clone(),
                });

                cache_path
            } else {
                path.dest
            };

            let volume_path = VolumePath {
                source: path.source,
                dest: dest,
                resync: path.resync,
                continuous: path.continuous,
                driver: path.driver,
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

    async fn create(
        &self,
        db: &DatabaseConnection,
        model: containers::Model,
    ) -> Result<String, Box<dyn std::error::Error>> {
        Mutation::update_container_status(
            db,
            model.id.clone(),
            Some(ContainerStatus::Creating.to_string()),
            None,
            None,
            None,
        )
        .await?;

        info!("[Runpod Controller] Using name: {}", model.name);
        let gpu_types_response = match self.runpod_client.list_gpu_types_graphql().await {
            Ok(response) => response,
            Err(e) => {
                error!("[Runpod Controller] Error fetching GPU types: {:?}", e);

                // More detailed error information
                if let Some(status) = e.status() {
                    error!("[Runpod Controller] HTTP Status: {}", status);
                }

                return Err(Box::<dyn std::error::Error>::from(format!(
                    "Error fetching GPU types: {:?}",
                    e
                )));
            }
        };

        let mut runpod_gpu_type_id: String = "NVIDIA_TESLA_T4".to_string(); // Default value
        let mut nebu_gpu_type_id: String = "NVIDIA_TESLA_T4".to_string(); // Default value
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
                    "[Runpod Controller] GPU Type: {}, Display Name: {}, Memory: {}",
                    gpu_type.id, gpu_type.display_name, memory_str
                );
            }
            info!(
                "[Runpod Controller] Available GPU types: {:?}",
                available_gpu_types
            );
        }
        info!(
            "[Runpod Controller] GPU types response: {:?}",
            gpu_types_response
        );

        // Parse accelerators if provided
        if let Some(accelerators) = &model.accelerators {
            if !accelerators.is_empty() {
                let mut found_valid_accelerator = false;

                // Try each accelerator in the list until we find one that works
                for accelerator in accelerators {
                    // Parse the accelerator (format: "count:type")
                    info!("[Runpod Controller] Accelerator: {}", accelerator);
                    let parts: Vec<&str> = accelerator.split(':').collect();
                    if parts.len() == 2 {
                        if let Ok(count) = parts[0].parse::<i32>() {
                            // Convert from our accelerator name to RunPod's GPU type ID
                            if let Some(runpod_gpu_name) = self.accelerator_map().get(parts[1]) {
                                info!("[Runpod Controller] RunPod GPU name: {}", runpod_gpu_name);
                                // Check if this GPU type is available on RunPod
                                if available_gpu_types.is_empty()
                                    || available_gpu_types.contains(runpod_gpu_name)
                                {
                                    // This accelerator is available, use it
                                    requested_gpu_count = count;
                                    runpod_gpu_type_id = runpod_gpu_name.clone();
                                    nebu_gpu_type_id = parts[1].to_string();
                                    found_valid_accelerator = true;

                                    info!(
                                        "[Runpod Controller] Using accelerator: {} (count: {})",
                                        runpod_gpu_type_id, requested_gpu_count
                                    );

                                    // We found a valid accelerator, stop looking
                                    break;
                                } else {
                                    info!(
                                        "[Runpod Controller] Accelerator type '{}' is not available, trying next option",
                                        runpod_gpu_name
                                    );
                                }
                            } else {
                                info!(
                                    "[Runpod Controller] Unknown accelerator type: {}, trying next option",
                                    parts[1]
                                );
                            }
                        }
                    }
                }

                // If we couldn't find any valid accelerator, return an error
                if !found_valid_accelerator {
                    error!(
                        "[Runpod Controller] None of the requested accelerator types are available. Available types: {:?}",
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

        //     info!("[Runpod Controller] Available datacenters and GPU types:");
        //     for datacenter in &datacenters {
        //         info!(
        //             "[Runpod Controller] Datacenter: {} ({})",
        //             datacenter.name, datacenter.id
        //         );

        //         // Remove the Option check since gpu_types is already a Vec
        //         for gpu_type in &datacenter.gpu_types {
        //             info!(
        //                 "[Runpod Controller]  ID: {}, Name: {}, Memory: {} GB",
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

        //     info!("[Runpod Controller] Using GPU type: {}", runpod_gpu_type_id);

        //     // Determine datacenter ID based on selected GPU type
        //     if let Some(gpu_info) = all_gpu_types.iter().find(|g| g.id == runpod_gpu_type_id) {
        //         datacenter_id = gpu_info.data_center_id.clone();
        //     }
        // } else if let Some(errors) = gpu_types_response.errors {
        //     let error_messages: Vec<String> = errors.iter().map(|e| e.message.clone()).collect();
        //     error!("[Runpod Controller] GraphQL errors: {:?}", error_messages);
        //     return Err(Box::<dyn std::error::Error>::from(format!(
        //         "Error fetching GPU types: GraphQL errors: {:?}",
        //         error_messages
        //     )));
        // } else {
        //     error!("[Runpod Controller] No data or errors returned from GPU types query");
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

        for (key, value) in self.get_common_env_vars(&model, db).await {
            env_vec.push(runpod::EnvVar { key, value });
        }

        // Add NEBU_SYNC_CONFIG environment variable with serialized volumes configuration
        match model.parse_volumes() {
            Ok(Some(volumes)) => {
                // We got a valid Vec of V1VolumePath. Proceed as before.
                let volume_config = RunpodPlatform::determine_volumes_config(volumes);
                info!("[Runpod Controller] Volume config: {:?}", volume_config);

                match serde_yaml::to_string(&volume_config) {
                    Ok(serialized_volumes) => {
                        env_vec.push(runpod::EnvVar {
                            key: "NEBU_SYNC_CONFIG".to_string(),
                            value: serialized_volumes,
                        });
                        info!("[Runpod Controller] Added NEBU_SYNC_CONFIG environment variable");
                    }
                    Err(e) => {
                        error!(
                            "[Runpod Controller] Failed to serialize volumes configuration: {}",
                            e
                        );
                        // Continue without this env var rather than failing the whole operation
                    }
                }
            }
            Ok(None) => {
                // parse_volumes() returned Ok, but no volumes were present
                info!("[Runpod Controller] No volumes configured, skipping NEBU_SYNC_CONFIG");
            }
            Err(e) => {
                // A serialization error occurred, so handle it (log, return, etc.)
                error!("[Runpod Controller] Failed to parse volumes: {}", e);
                // If you’d prefer to ignore the parse failure and still run, you could do so here
            }
        }
        info!("[Runpod Controller] Environment variables: {:?}", env_vec);

        match model.parse_env_vars() {
            Ok(Some(env_vars)) => {
                // We have a valid, non-empty list of environment variables.
                for env_var in env_vars {
                    env_vec.push(runpod::EnvVar {
                        key: env_var.key.clone(),
                        value: env_var.value.clone(),
                    });
                }
                info!("[Runpod Controller] Successfully parsed and added environment variables from model");
            }
            Ok(None) => {
                // parse_env_vars() returned Ok, but the database column was None or empty.
                info!("[Runpod Controller] No environment variables configured in database");
            }
            Err(e) => {
                // A serialization error occurred while parsing
                error!(
                    "[Runpod Controller] Failed to parse environment variables: {}",
                    e
                );
                // Decide how you want to handle the error (return early, ignore, etc.)
            }
        }
        info!("[Runpod Controller] Environment variables: {:?}", env_vec);

        let docker_command = self.build_command(&model);
        info!("[Runpod Controller] Docker command: {:?}", docker_command);

        // 5) Create an on-demand instance instead of a spot instance
        let create_request = CreateOnDemandPodRequest {
            cloud_type: Some("SECURE".to_string()),
            gpu_count: Some(requested_gpu_count),
            volume_in_gb: Some(500),
            container_disk_in_gb: Some(1000),
            min_vcpu_count: Some(8),
            min_memory_in_gb: Some(30),
            gpu_type_id: Some(runpod_gpu_type_id),
            name: Some(model.id.clone()),
            image_name: Some(model.image.clone()),
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
            "[Runpod Controller] Creating on-demand pod with request: {:?}",
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
                        "[Runpod Controller] Successfully created On-Demand Pod '{}' (id = {}) on RunPod!",
                        model.id, pod.id
                    );

                    Mutation::update_container_resource_name(db, model.id.clone(), pod.id.clone())
                        .await?;
                    Mutation::update_container_status(
                        db,
                        model.id.clone(),
                        Some(ContainerStatus::Created.to_string()),
                        None,
                        Some(nebu_gpu_type_id),
                        pod.public_ip,
                    )
                    .await?;
                    pod.id
                } else {
                    return Err(format!(
                        "On-Demand Pod creation returned empty data for job '{}'",
                        model.id
                    )
                    .into());
                }
            }
            Err(e) => {
                return Err(format!(
                    "Error creating on-demand pod on RunPod for '{}': {:?}",
                    model.id, e
                )
                .into());
            }
        };

        info!(
            "[Runpod Controller] Job {} created on RunPod with pod ID {}",
            model.id, pod_id
        );

        Ok(pod_id)
    }

    fn build_command(&self, model: &containers::Model) -> Option<Vec<String>> {
        let cmd = model.command.clone()?;

        // Statements to install curl if missing
        let curl_install = r#"
            echo "[DEBUG] Installing curl (if not present)..."
            if ! command -v curl &> /dev/null; then
                apt-get update && apt-get install -y curl \
                || (apk update && apk add --no-cache curl) \
                || echo 'Failed to install curl'
            fi
        "#;

        // Statements to install nebu if missing
        let nebu_install = r#"
            echo "[DEBUG] Installing nebu (if not present)..."
            if ! command -v nebu &> /dev/null; then
                curl -s https://raw.githubusercontent.com/agentsea/nebulous/main/remote_install.sh | bash \
                || echo 'Failed to install nebu'
            fi
        "#;

        // Build the main set of commands without braces
        let base_script = format!(
            r#"
    set -x
    echo "[DEBUG] Starting setup..."
    {curl_install}
    echo "[DEBUG] Done installing curl..."
    
    {nebu_install}
    echo "[DEBUG] Done installing nebu; checking version..."
    nebu --version
    
    echo "[DEBUG] Invoking nebu sync..."
    nebu sync --config /nebu/sync.yaml --interval-seconds 5 \
        --create-if-missing --watch --background --block-once --config-from-env
    
    echo "[DEBUG] All done with base_command; now your user command: {cmd}"
    {cmd}
    "#,
            curl_install = curl_install,
            nebu_install = nebu_install,
            cmd = cmd
        );

        // If the user wants "Never" to restart, append the cleanup
        let final_script = if model.restart == RestartPolicy::Never.to_string() {
            // Separate line so the shell sees it as a new command
            format!("{base_script}\nnebu delete containers {}", model.id)
        } else {
            base_script
        };

        // A minimal approach to quoting in single quotes:
        let escaped = final_script.replace('\'', "'\"'\"'");
        let quoted_script = format!("{}", escaped);

        // Return the final ["bash", "-c", "..."] command
        Some(vec!["bash".to_string(), "-c".to_string(), quoted_script])
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
    async fn declare(
        &self,
        config: &V1ContainerRequest,
        db: &DatabaseConnection,
        user_profile: &V1UserProfile,
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
        info!("[Runpod Controller] Using name: {}", name);
        let gpu_types_response = match self.runpod_client.list_gpu_types_graphql().await {
            Ok(response) => response,
            Err(e) => {
                error!("[Runpod Controller] Error fetching GPU types: {:?}", e);

                // More detailed error information
                if let Some(status) = e.status() {
                    error!("[Runpod Controller] HTTP Status: {}", status);
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
                    "[Runpod Controller] GPU Type: {}, Display Name: {}, Memory: {}",
                    gpu_type.id, gpu_type.display_name, memory_str
                );
            }
            info!(
                "[Runpod Controller] Available GPU types: {:?}",
                available_gpu_types
            );
        }
        info!(
            "[Runpod Controller] GPU types response: {:?}",
            gpu_types_response
        );

        // Parse accelerators if provided
        if let Some(accelerators) = &config.accelerators {
            if !accelerators.is_empty() {
                let mut found_valid_accelerator = false;

                // Try each accelerator in the list until we find one that works
                for accelerator in accelerators {
                    // Parse the accelerator (format: "count:type")
                    info!("[Runpod Controller] Accelerator: {}", accelerator);
                    let parts: Vec<&str> = accelerator.split(':').collect();
                    if parts.len() == 2 {
                        if let Ok(count) = parts[0].parse::<i32>() {
                            // Convert from our accelerator name to RunPod's GPU type ID
                            if let Some(runpod_gpu_name) = self.accelerator_map().get(parts[1]) {
                                info!("[Runpod Controller] RunPod GPU name: {}", runpod_gpu_name);
                                // Check if this GPU type is available on RunPod
                                if available_gpu_types.is_empty()
                                    || available_gpu_types.contains(runpod_gpu_name)
                                {
                                    // This accelerator is available, use it
                                    requested_gpu_count = count;
                                    runpod_gpu_type_id = runpod_gpu_name.clone();
                                    found_valid_accelerator = true;

                                    info!(
                                        "[Runpod Controller] Using accelerator: {} (count: {})",
                                        runpod_gpu_type_id, requested_gpu_count
                                    );

                                    // We found a valid accelerator, stop looking
                                    break;
                                } else {
                                    info!(
                                        "[Runpod Controller] Accelerator type '{}' is not available, trying next option",
                                        runpod_gpu_name
                                    );
                                }
                            } else {
                                info!(
                                    "[Runpod Controller] Unknown accelerator type: {}, trying next option",
                                    parts[1]
                                );
                            }
                        }
                    }
                }

                // If we couldn't find any valid accelerator, return an error
                if !found_valid_accelerator {
                    error!(
                        "[Runpod Controller] None of the requested accelerator types are available. Available types: {:?}",
                        available_gpu_types
                    );
                    return Err(Box::<dyn std::error::Error>::from(
                        "None of the requested accelerator types are available on RunPod"
                            .to_string(),
                    ));
                }
            }
        }

        let id = ShortUuid::generate().to_string();
        info!("[Runpod Controller] ID: {}", id);

        self.store_agent_key_secret(db, user_profile, &id, owner_id)
            .await?;

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
            status: Set(Some(serde_json::json!(V1ContainerStatus {
                status: Some(ContainerStatus::Defined.to_string()),
                message: None,
                accelerator: None,
                public_ip: None,
            }))),
            platform: Set(Some("runpod".to_string())),
            meters: Set(config
                .meters
                .clone()
                .map(|meters| serde_json::json!(meters))),
            resource_name: Set(None),
            resource_namespace: Set(None),
            command: Set(config.command.clone()),
            labels: Set(config
                .metadata
                .as_ref()
                .and_then(|meta| meta.labels.clone().map(|labels| serde_json::json!(labels)))),
            restart: Set(config.restart.clone()),
            queue: Set(config.queue.clone()),
            resources: Set(config
                .resources
                .clone()
                .map(|resources| serde_json::json!(resources))),
            desired_status: Set(Some(ContainerStatus::Running.to_string())),
            ssh_keys: Set(config.ssh_keys.clone().map(|keys| serde_json::json!(keys))),
            public_ip: Set(None),
            private_ip: Set(None),
            created_by: Set(Some(owner_id.to_string())),
            updated_at: Set(chrono::Utc::now().into()),
            created_at: Set(chrono::Utc::now().into()),
            controller_data: Set(None),
        };

        if let Err(e) = container.insert(db).await {
            error!(
                "[Runpod Controller] Failed to create container in database: {:?}",
                e
            );
            return Err(format!("Failed to create container in database: {:?}", e).into());
        } else {
            info!("[Runpod Controller] Created container {} in database ", id);
        }

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
            queue: config.queue.clone(),
            ssh_keys: config.ssh_keys.clone(),
            status: Some(V1ContainerStatus {
                status: Some(ContainerStatus::Defined.to_string()),
                message: None,
                accelerator: None,
                public_ip: None,
            }),
            restart: config.restart.clone(),
            resources: config.resources.clone(),
        })
    }

    async fn reconcile(
        &self,
        container: &containers::Model,
        db: &DatabaseConnection,
    ) -> Result<(), Box<dyn std::error::Error>> {
        debug!(
            "[DEBUG:runpod.rs:reconcile] Entering reconcile for container {}",
            container.id
        );

        // If this container is assigned to a queue,
        // ensure no other container in that same queue is running/active.
        if let Some(queue_name) = &container.queue {
            // We check if the queue is free. We'll skip starting if not free.
            let queue_is_free =
                crate::query::Query::is_queue_free(db, queue_name, &container.id).await?;
            if !queue_is_free {
                // The queue is blocked by another container.
                // Optionally move this container to "Queued" status, or just do nothing.
                info!(
                    "[Runpod Controller] Container {} is blocked by an active container in queue '{}'; skipping start.",
                    container.id, queue_name
                );

                // If you'd like to explicitly set the container status to Queued in DB:
                // (Only do this if not already in some other terminal or running state.)
                if let Ok(Some(parsed_status)) = container.parse_status() {
                    let current_status = parsed_status.status.unwrap_or_default();
                    // If it's not already queued (or in a terminal state), set to queued:
                    if current_status != ContainerStatus::Queued.to_string()
                        && !ContainerStatus::from_str(&current_status)
                            .unwrap_or(ContainerStatus::Invalid)
                            .is_inactive()
                    {
                        crate::mutation::Mutation::update_container_status(
                            db,
                            container.id.clone(),
                            Some(ContainerStatus::Queued.to_string()),
                            Some("Blocked by another running container in queue".to_string()),
                            None,
                            None,
                        )
                        .await
                        .map_err(|e| format!("Failed to set container to Queued: {}", e))?;
                    }
                }

                return Ok(()); // do not proceed to create or watch
            }
        }

        if let Ok(Some(parsed_status)) = container.parse_status() {
            let status_str = parsed_status
                .status
                .unwrap_or(ContainerStatus::Invalid.to_string());
            debug!(
                "[DEBUG:runpod.rs:reconcile] Container {} has status {}",
                container.id, status_str
            );

            let status = ContainerStatus::from_str(&status_str).unwrap_or(ContainerStatus::Invalid);

            if status.needs_start() {
                info!(
                    "[Runpod Controller] Container {} needs to be started",
                    container.id
                );
                if let Some(ds) = &container.desired_status {
                    if ds == &ContainerStatus::Running.to_string() {
                        info!("[Runpod Controller] Container {} has a desired status of 'running', creating...", container.id);
                        self.create(db, container.clone()).await?;
                    }
                } else {
                    info!("[Runpod Controller] Container {} does not have a desired status of 'running'", container.id);
                }
            }

            if status.needs_watch() {
                info!(
                    "[Runpod Controller] Container {} needs to be watched",
                    container.id
                );
                self.watch(db, container.clone()).await?;
            }
        }
        debug!(
            "[DEBUG:runpod.rs:reconcile] Completed reconcile for container {}",
            container.id
        );
        Ok(())
    }

    async fn delete(
        &self,
        id: &str,
        db: &DatabaseConnection,
    ) -> Result<(), Box<dyn std::error::Error>> {
        info!(
            "[Runpod Controller] Attempting to delete container with name: {}",
            id
        );

        // First, list all pods to find the one with our name
        match self.runpod_client.list_pods().await {
            Ok(pods_response) => {
                if let Some(my_pods) = pods_response.data {
                    // Find the pod with matching name
                    let pod_to_delete = my_pods.pods.iter().find(|p| p.name == id);

                    if let Some(pod) = pod_to_delete {
                        info!(
                            "[Runpod Controller] Found pod with ID: {} for container: {}",
                            pod.id, id
                        );

                        // Stop the pod
                        match self.runpod_client.delete_pod(&pod.id).await {
                            Ok(_) => {
                                info!("[Runpod Controller] Successfully stopped pod: {}", pod.id);

                                // Update container status in database
                                if let Err(e) = crate::mutation::Mutation::update_container_status(
                                    &db,
                                    id.clone().to_string(),
                                    Some(ContainerStatus::Stopped.to_string()),
                                    None,
                                    None,
                                    None,
                                )
                                .await
                                {
                                    error!("[Runpod Controller] Failed to update container status in database: {}", e);
                                    return Err(e.into());
                                } else {
                                    info!("[Runpod Controller] Updated container {} status to stopped", id);
                                }
                            }
                            Err(e) => {
                                error!("[Runpod Controller] Failed to stop pod {}: {}", pod.id, e);
                                return Err(e.into());
                            }
                        }
                    } else {
                        info!("[Runpod Controller] No pod found with name: {}", id);
                    }
                } else {
                    error!("[Runpod Controller] No pods data returned from RunPod API");
                    return Err("No pods data returned from RunPod API".into());
                }
            }
            Err(e) => {
                error!("[Runpod Controller] Error listing pods: {}", e);
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

/// Returns true if the given error indicates a 404 Not Found response.
pub fn is_not_found(err: &reqwest::Error) -> bool {
    err.status() == Some(reqwest::StatusCode::NOT_FOUND)
}
