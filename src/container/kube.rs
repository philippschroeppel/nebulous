use crate::container::base::ContainerPlatform;
use crate::models::{Container, ContainerMeta, ContainerRequest};
use k8s_openapi::api::batch::v1::{Job, JobSpec};
use k8s_openapi::api::core::v1::{
    Container as K8sContainer, ContainerPort, EnvVar, PodSpec, PodTemplateSpec,
    ResourceRequirements, Volume, VolumeMount,
};
use k8s_openapi::apimachinery::pkg::apis::meta::v1::ObjectMeta;
use kube::{api::PostParams, Api, Client};
use petname;
use sea_orm::{ActiveModelTrait, DatabaseConnection, Set};
use short_uuid::ShortUuid;
use std::collections::{BTreeMap, HashMap};
use tracing::{error, info};

/// A `ContainerPlatform` implementation that schedules container jobs on Kubernetes.
#[derive(Clone)]
pub struct KubePlatform {
    namespace: String,
    kubeconfig_path: Option<String>,
    context: Option<String>,
}

impl KubePlatform {
    pub fn new() -> Self {
        // Read the namespace from environment variables or use default
        let namespace = std::env::var("KUBE_NAMESPACE").unwrap_or_else(|_| "default".to_string());
        let kubeconfig_path = std::env::var("KUBECONFIG").ok();
        let context = std::env::var("KUBE_CONTEXT").ok();

        KubePlatform {
            namespace,
            kubeconfig_path,
            context,
        }
    }

    /// Create a new KubePlatform with a specific namespace
    pub fn with_namespace(namespace: String) -> Self {
        KubePlatform {
            namespace,
            kubeconfig_path: None,
            context: None,
        }
    }

    /// Create a new KubePlatform with custom configuration
    pub fn with_config(
        namespace: String,
        kubeconfig_path: Option<String>,
        context: Option<String>,
    ) -> Self {
        KubePlatform {
            namespace,
            kubeconfig_path,
            context,
        }
    }

    /// Get a configured Kubernetes client
    async fn get_client(&self) -> Result<Client, kube::Error> {
        if let Some(kubeconfig_path) = &self.kubeconfig_path {
            info!(
                "[Kubernetes] Using kubeconfig from path: {}",
                kubeconfig_path
            );
            // Load kubeconfig from the specified path
            let kubeconfig = match kube::config::Kubeconfig::read_from(kubeconfig_path) {
                Ok(config) => config,
                Err(e) => {
                    return Err(kube::Error::Api(kube::error::ErrorResponse {
                        status: "Failure".to_string(),
                        message: format!("Failed to load kubeconfig: {}", e),
                        reason: "InvalidConfiguration".to_string(),
                        code: 500,
                    }))
                }
            };

            // Create config with optional context
            let config = match &self.context {
                Some(context) => {
                    info!("[Kubernetes] Using context: {}", context);
                    let options = kube::config::KubeConfigOptions {
                        context: Some(context.clone()),
                        ..Default::default()
                    };
                    match kube::config::Config::from_custom_kubeconfig(kubeconfig, &options).await {
                        Ok(config) => config,
                        Err(e) => {
                            return Err(kube::Error::Api(kube::error::ErrorResponse {
                                status: "Failure".to_string(),
                                message: format!("Failed to create config with context: {}", e),
                                reason: "InvalidConfiguration".to_string(),
                                code: 500,
                            }))
                        }
                    }
                }
                None => {
                    let options = kube::config::KubeConfigOptions::default();
                    match kube::config::Config::from_custom_kubeconfig(kubeconfig, &options).await {
                        Ok(config) => config,
                        Err(e) => {
                            return Err(kube::Error::Api(kube::error::ErrorResponse {
                                status: "Failure".to_string(),
                                message: format!("Failed to create config: {}", e),
                                reason: "InvalidConfiguration".to_string(),
                                code: 500,
                            }))
                        }
                    }
                }
            };

            Client::try_from(config)
        } else {
            // Use the default configuration
            Client::try_default().await
        }
    }

    /// Watch a job and update its status in the database
    pub async fn watch_job_status(
        &self,
        job_name: &str,
        container_id: &str,
    ) -> Result<(), Box<dyn std::error::Error>> {
        info!(
            "[Kubernetes] Starting to watch job {} for container_id {}",
            job_name, container_id
        );

        // Initial status check
        let mut last_status = String::new();
        let mut consecutive_errors = 0;
        const MAX_ERRORS: usize = 5;

        // Get a database connection from the pool
        let db = sea_orm::Database::connect(
            std::env::var("DATABASE_URL").expect("DATABASE_URL must be set"),
        )
        .await?;

        // Get the Kubernetes client
        let client = Client::try_default().await?;
        let jobs: Api<Job> = Api::namespaced(client.clone(), &self.namespace);

        // Poll the job status every 30 seconds
        loop {
            match jobs.get(job_name).await {
                Ok(job) => {
                    consecutive_errors = 0;

                    // Extract status information
                    let current_status = if let Some(status) = &job.status {
                        if let Some(completion_time) = &status.completion_time {
                            if status.succeeded.unwrap_or(0) > 0 {
                                "completed".to_string()
                            } else {
                                "failed".to_string()
                            }
                        } else if status.active.unwrap_or(0) > 0 {
                            "running".to_string()
                        } else if status.failed.unwrap_or(0) > 0 {
                            "failed".to_string()
                        } else {
                            "pending".to_string()
                        }
                    } else {
                        "pending".to_string()
                    };

                    // If status changed, update the database
                    if current_status != last_status {
                        info!(
                            "[Kubernetes] Job {} status changed: {} -> {}",
                            job_name, last_status, current_status
                        );
                        last_status = current_status.clone();

                        // Update the database with the new status
                        match crate::mutation::Mutation::update_container_status(
                            &db,
                            container_id.to_string(),
                            current_status.clone(),
                        )
                        .await
                        {
                            Ok(_) => {
                                info!(
                                    "[Kubernetes] Updated container {} status to {}",
                                    container_id, current_status
                                )
                            }
                            Err(e) => {
                                error!(
                                    "[Kubernetes] Failed to update job status in database: {}",
                                    e
                                )
                            }
                        }

                        // If the job is in a terminal state, exit the loop
                        if current_status == "completed" || current_status == "failed" {
                            info!(
                                "[Kubernetes] Job {} reached terminal state: {}",
                                job_name, current_status
                            );
                            break;
                        }
                    }
                }
                Err(e) => {
                    error!("[Kubernetes] Error fetching job status: {}", e);
                    consecutive_errors += 1;

                    // If we've had too many consecutive errors, mark the job as failed
                    if consecutive_errors >= MAX_ERRORS {
                        error!("[Kubernetes] Too many consecutive errors, marking job as failed");

                        if let Err(e) = crate::mutation::Mutation::update_container_status(
                            &db,
                            container_id.to_string(),
                            "failed".to_string(),
                        )
                        .await
                        {
                            error!(
                                "[Kubernetes] Failed to update job status in database: {}",
                                e
                            );
                        }

                        break;
                    }
                }
            }

            // Wait before checking again
            tokio::time::sleep(tokio::time::Duration::from_secs(30)).await;
        }

        info!(
            "[Kubernetes] Finished watching job {} for container_id {}",
            job_name, container_id
        );
        Ok(())
    }

    /// Get common environment variables for all containers
    fn get_common_env_vars(&self) -> HashMap<String, String> {
        let mut env_vars = HashMap::new();

        // Add common environment variables here
        env_vars.insert("PLATFORM".to_string(), "kubernetes".to_string());

        env_vars
    }
}

impl ContainerPlatform for KubePlatform {
    /// Run a container on Kubernetes by creating a Job
    fn run(
        &self,
        config: &ContainerRequest,
        db: &DatabaseConnection,
        owner_id: &str,
    ) -> Result<Container, Box<dyn std::error::Error>> {
        let name = config.name.clone().unwrap_or_else(|| {
            // Generate a random human-friendly name using petname
            petname::petname(3, "-").unwrap()
        });
        info!("[Kubernetes] Using name: {}", name);

        // Create a runtime to handle the async call
        let rt = tokio::runtime::Runtime::new().expect("Failed to create Tokio runtime");

        // Determine GPU requirements
        let mut gpu_count = 0;
        let mut gpu_type = "nvidia-tesla-t4"; // Default GPU type

        // Parse accelerators if provided
        if let Some(accelerators) = &config.accelerators {
            if !accelerators.is_empty() {
                // Parse the first accelerator in the list (format: "count:type")
                let parts: Vec<&str> = accelerators[0].split(':').collect();
                if parts.len() == 2 {
                    if let Ok(count) = parts[0].parse::<i32>() {
                        gpu_count = count;
                    }

                    // Convert from our accelerator name to Kubernetes GPU type
                    if let Some(k8s_gpu_name) = self.accelerator_map().get(parts[1]) {
                        gpu_type = k8s_gpu_name;
                        info!(
                            "[Kubernetes] Using accelerator: {} (count: {})",
                            gpu_type, gpu_count
                        );
                    } else {
                        error!(
                            "[Kubernetes] Unknown accelerator type: {}, using default",
                            parts[1]
                        );
                    }
                }
            }
        }

        // Prepare environment variables
        let mut env_vars = Vec::new();

        // Add common environment variables
        for (key, value) in self.get_common_env_vars() {
            env_vars.push(EnvVar {
                name: key,
                value: Some(value),
                ..Default::default()
            });
        }

        // Add ORIGN_SYNC_CONFIG environment variable with serialized volumes configuration
        if let Ok(serialized_volumes) = serde_yaml::to_string(&config.volumes) {
            env_vars.push(EnvVar {
                name: "ORIGN_SYNC_CONFIG".to_string(),
                value: Some(serialized_volumes),
                ..Default::default()
            });
            info!("[Kubernetes] Added ORIGN_SYNC_CONFIG environment variable");
        } else {
            error!("[Kubernetes] Failed to serialize volumes configuration");
        }

        // Add user-provided environment variables
        if let Some(user_env_vars) = &config.env_vars {
            for (key, value) in user_env_vars {
                env_vars.push(EnvVar {
                    name: key.clone(),
                    value: Some(value.clone()),
                    ..Default::default()
                });
            }
        }

        // Prepare volume mounts
        let volume_mounts = vec![
            VolumeMount {
                name: "huggingface-cache".to_string(),
                mount_path: "/huggingface".to_string(),
                ..Default::default()
            },
            VolumeMount {
                name: "nebu-pvc".to_string(),
                mount_path: "/nebu".to_string(),
                ..Default::default()
            },
        ];

        // Prepare volumes
        let volumes = vec![
            Volume {
                name: "huggingface-cache".to_string(),
                persistent_volume_claim: Some(
                    k8s_openapi::api::core::v1::PersistentVolumeClaimVolumeSource {
                        claim_name: "huggingface-cache-pvc".to_string(),
                        ..Default::default()
                    },
                ),
                ..Default::default()
            },
            Volume {
                name: "nebu-pvc".to_string(),
                persistent_volume_claim: Some(
                    k8s_openapi::api::core::v1::PersistentVolumeClaimVolumeSource {
                        claim_name: "nebu-pvc".to_string(),
                        ..Default::default()
                    },
                ),
                ..Default::default()
            },
        ];

        // Prepare node selector for GPU scheduling
        let mut node_selector = BTreeMap::new();
        node_selector.insert("role".to_string(), "gpu".to_string());

        if gpu_count > 0 {
            node_selector.insert("gpu-type".to_string(), "nvidia".to_string());
        }

        // Prepare resource requirements
        let mut resource_requirements = ResourceRequirements::default();

        if gpu_count > 0 {
            let mut limits = BTreeMap::new();
            limits.insert(
                format!("nvidia.com/gpu"),
                k8s_openapi::apimachinery::pkg::api::resource::Quantity(gpu_count.to_string()),
            );
            resource_requirements.limits = Some(limits);
        }

        // Create the container
        let container = K8sContainer {
            name: name.clone(),
            image: Some(config.image.clone()),
            command: config
                .command
                .as_ref()
                .map(|cmd| cmd.split(" ").map(String::from).collect()),
            ports: Some(vec![ContainerPort {
                container_port: 8000,
                ..Default::default()
            }]),
            env: Some(env_vars),
            resources: Some(resource_requirements),
            volume_mounts: Some(volume_mounts),
            ..Default::default()
        };

        // Create the pod spec
        let pod_spec = PodSpec {
            containers: vec![container],
            restart_policy: Some("Never".to_string()),
            volumes: Some(volumes),
            node_selector: Some(node_selector),
            ..Default::default()
        };

        // Create the pod template
        let template = PodTemplateSpec {
            metadata: Some(ObjectMeta {
                labels: Some({
                    let mut lbls = BTreeMap::new();
                    lbls.insert("app".to_string(), name.clone());
                    lbls
                }),
                ..Default::default()
            }),
            spec: Some(pod_spec),
        };

        // Create the job spec
        let job_spec = JobSpec {
            template,
            backoff_limit: Some(0),
            ..Default::default()
        };

        // Create the job
        let job = Job {
            metadata: ObjectMeta {
                name: Some(name.clone()),
                ..Default::default()
            },
            spec: Some(job_spec),
            ..Default::default()
        };

        let id = ShortUuid::generate().to_string();

        // Submit the job to Kubernetes
        rt.block_on(async {
            match self.get_client().await {
                Ok(client) => {
                    let jobs: Api<Job> = Api::namespaced(client, &self.namespace);
                    match jobs.create(&PostParams::default(), &job).await {
                        Ok(_) => {
                            info!("[Kubernetes] Successfully created Job '{}'", name);

                            // Create the container record in the database
                            let container = crate::entities::containers::ActiveModel {
                                id: Set(id.clone()),
                                namespace: Set(config
                                    .namespace
                                    .clone()
                                    .unwrap_or_else(|| "default".to_string())),
                                name: Set(name.clone()),
                                owner_id: Set(owner_id.to_string()),
                                image: Set(config.image.clone()),
                                env_vars: Set(config
                                    .env_vars
                                    .clone()
                                    .map(|vars| serde_json::json!(vars))),
                                volumes: Set(config
                                    .volumes
                                    .clone()
                                    .map(|vols| serde_json::json!(vols))),
                                accelerators: Set(config.accelerators.clone()),
                                cpu_request: Set(None),
                                memory_request: Set(None),
                                status: Set(Some("pending".to_string())),
                                platform: Set(Some("kubernetes".to_string())),
                                resource_name: Set(Some(name.clone())),
                                resource_namespace: Set(Some(self.namespace.clone())),
                                command: Set(config.command.clone()),
                                labels: Set(config
                                    .labels
                                    .clone()
                                    .map(|labels| serde_json::json!(labels))),
                                created_by: Set(Some("kubernetes".to_string())),
                                updated_at: Set(chrono::Utc::now().into()),
                                created_at: Set(chrono::Utc::now().into()),
                            };

                            if let Err(e) = container.insert(db).await {
                                error!(
                                    "[Kubernetes] Failed to create container in database: {:?}",
                                    e
                                );
                            } else {
                                info!("[Kubernetes] Created container {} in database", name);
                            }

                            // Start watching the job status
                            let name_clone = name.clone();
                            let db_clone = db.clone();
                            let self_clone = self.clone();

                            tokio::spawn(async move {
                                if let Err(e) =
                                    self_clone.watch_job_status(&name_clone, &name_clone).await
                                {
                                    error!("[Kubernetes] Error watching job status: {:?}", e);
                                }
                            });
                        }
                        Err(e) => {
                            error!("[Kubernetes] Error creating Job '{}': {:?}", name, e);
                        }
                    }
                }
                Err(e) => {
                    error!("[Kubernetes] Failed to create K8s client: {:?}", e);
                }
            }
        });

        info!("[Kubernetes] Job {} created on Kubernetes", name);
        Ok(Container {
            metadata: ContainerMeta {
                id: id.clone(),
                owner_id: owner_id.to_string(),
                created_at: chrono::Utc::now().timestamp(),
                updated_at: chrono::Utc::now().timestamp(),
                created_by: "kubernetes".to_string(),
                labels: config.labels.clone(),
            },
            name: name.clone(),
            namespace: config
                .namespace
                .clone()
                .unwrap_or_else(|| "default".to_string()),
            image: config.image.clone(),
            env_vars: config.env_vars.clone(),
            command: config.command.clone(),
            volumes: config.volumes.clone(),
            accelerators: config.accelerators.clone(),
        })
    }

    fn delete(&self, id: &str, db: &DatabaseConnection) -> Result<(), Box<dyn std::error::Error>> {
        Ok(())
    }

    fn accelerator_map(&self) -> HashMap<String, String> {
        return HashMap::new();
    }
}
