use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct V1ErrorResponse {
    #[serde(rename = "type", default = "default_error_response_type")]
    pub response_type: String,
    pub request_id: String,
    pub error: String,
    pub traceback: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct V1Meter {
    pub cost: Option<f64>,
    pub costp: Option<f64>,
    pub currency: String,
    pub unit: String,
    pub metric: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct V1EnvVar {
    pub key: String,
    pub value: String,
}

fn default_error_response_type() -> String {
    "ErrorResponse".to_string()
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct V1ContainerMetaRequest {
    pub name: Option<String>,
    pub namespace: Option<String>,
    pub labels: Option<HashMap<String, String>>,
    pub owner_id: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct V1ContainerRequest {
    #[serde(default = "default_container_kind")]
    pub kind: String,
    pub platform: Option<String>,
    pub metadata: Option<V1ContainerMetaRequest>,
    pub image: String,
    pub env_vars: Option<Vec<V1EnvVar>>,
    pub command: Option<String>,
    pub volumes: Option<Vec<V1VolumePath>>,
    pub accelerators: Option<Vec<String>>,
    pub resources: Option<V1ContainerResources>,
    pub meters: Option<Vec<V1Meter>>,
    #[serde(default = "default_restart")]
    pub restart: String,
    pub queue: Option<String>,
    pub ssh_keys: Option<Vec<V1SSHKey>>,
}

pub enum RestartPolicy {
    Always,
    Never,
}

fn default_restart() -> String {
    RestartPolicy::Always.to_string()
}

impl fmt::Display for RestartPolicy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RestartPolicy::Always => write!(f, "Always"),
            RestartPolicy::Never => write!(f, "Never"),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct V1ContainerResources {
    pub min_cpu: Option<f64>,
    pub min_memory: Option<f64>,
    pub max_cpu: Option<f64>,
    pub max_memory: Option<f64>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct V1ContainerMeta {
    pub name: String,
    pub namespace: String,
    pub id: String,
    pub owner_id: String,
    pub created_at: i64,
    pub updated_at: i64,
    pub created_by: String,
    pub labels: Option<HashMap<String, String>>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct V1ContainerStatus {
    pub status: Option<String>,
    pub message: Option<String>,
    pub accelerator: Option<String>,
    pub public_ip: Option<String>,
    pub cost_per_hr: Option<f64>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct V1SSHKey {
    pub public_key: Option<String>,
    pub public_key_secret: Option<String>,
    pub copy_local: Option<bool>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct V1Container {
    #[serde(default = "default_container_kind")]
    pub kind: String,
    pub metadata: V1ContainerMeta,
    pub image: String,
    pub env_vars: Option<Vec<V1EnvVar>>,
    pub command: Option<String>,
    pub volumes: Option<Vec<V1VolumePath>>,
    pub accelerators: Option<Vec<String>>,
    pub meters: Option<Vec<V1Meter>>,
    pub restart: String,
    pub queue: Option<String>,
    pub resources: Option<V1ContainerResources>,
    pub status: Option<V1ContainerStatus>,
    pub ssh_keys: Option<Vec<V1SSHKey>>,
}
// Add this function to provide a default kind value
fn default_container_kind() -> String {
    "Container".to_string()
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct V1UpdateContainer {
    pub image: Option<String>,
    pub env_vars: Option<Vec<V1EnvVar>>,
    pub command: Option<String>,
    pub volumes: Option<Vec<V1VolumePath>>,
    pub accelerators: Option<Vec<String>>,
    pub labels: Option<HashMap<String, String>>,
    pub cpu_request: Option<String>,
    pub memory_request: Option<String>,
    pub platform: Option<String>,
    pub meters: Option<Vec<V1Meter>>,
    pub restart: Option<String>,
    pub queue: Option<String>,
    pub resources: Option<V1ContainerResources>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct V1VolumeConfig {
    pub paths: Vec<V1VolumePath>,
    #[serde(default = "default_cache_dir")]
    pub cache_dir: String,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default, PartialEq)]
pub enum V1VolumeDriver {
    #[default]
    RCLONE_SYNC,
    RCLONE_BISYNC,
    RCLONE_MOUNT,
}

impl std::str::FromStr for V1VolumeDriver {
    type Err = &'static str;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "RCLONE_BISYNC" => Ok(V1VolumeDriver::RCLONE_BISYNC),
            "RCLONE_SYNC" => Ok(V1VolumeDriver::RCLONE_SYNC),
            "RCLONE_MOUNT" => Ok(V1VolumeDriver::RCLONE_MOUNT),
            _ => Err("Unrecognized VolumeType"),
        }
    }
}

impl fmt::Display for V1VolumeDriver {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_string())
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct V1VolumePath {
    pub source: String,
    pub dest: String,
    #[serde(default)]
    pub resync: bool,
    #[serde(default = "default_continuous")]
    pub continuous: bool,
    #[serde(default = "default_volume_driver")]
    pub driver: V1VolumeDriver,
}

fn default_volume_driver() -> V1VolumeDriver {
    V1VolumeDriver::RCLONE_SYNC
}

fn default_continuous() -> bool {
    true
}

// Add this function to provide a default cache directory
fn default_cache_dir() -> String {
    // Use a sensible default location for the cache
    format!("/nebu/cache")
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct V1UserProfile {
    pub email: String,
    pub display_name: Option<String>,
    pub handle: Option<String>,
    pub picture: Option<String>,
    pub organization: Option<String>,
    pub role: Option<String>,
    pub external_id: Option<String>,
    pub actor: Option<String>,
    pub organizations: Option<HashMap<String, HashMap<String, String>>>,
    pub created: Option<i64>,
    pub updated: Option<i64>,
    pub token: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct V1ContainerList {
    pub containers: Vec<V1Container>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct V1CreateAgentKeyRequest {
    pub agent_id: String,
    pub name: String,
    pub duration: i32,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct V1AgentKey {
    pub name: String,
    pub key: Option<String>,
    pub created: Option<i64>,
    pub valid_for: Option<i64>,
    pub org: Option<String>,
    pub role: Option<String>,
}
