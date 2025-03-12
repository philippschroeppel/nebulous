use serde::{Deserialize, Serialize};
use std::collections::HashMap;
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
    pub cost: String,
    pub currency: String,
    pub metric: String,
}

fn default_error_response_type() -> String {
    "ErrorResponse".to_string()
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct V1ContainerRequest {
    #[serde(default = "default_container_kind")]
    pub kind: String,
    pub name: Option<String>,
    pub namespace: Option<String>,
    pub platform: Option<String>,
    pub labels: Option<HashMap<String, String>>,
    pub image: String,
    pub env_vars: Option<HashMap<String, String>>,
    pub command: Option<String>,
    pub volumes: Option<V1VolumeConfig>,
    pub accelerators: Option<Vec<String>>,
    pub meters: Option<Vec<V1Meter>>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct V1ContainerMeta {
    pub id: String,
    pub owner_id: String,
    pub created_at: i64,
    pub updated_at: i64,
    pub created_by: String,
    pub labels: Option<HashMap<String, String>>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct V1Container {
    #[serde(default = "default_container_kind")]
    pub kind: String,
    pub metadata: V1ContainerMeta,
    pub name: String,
    pub namespace: String,
    pub image: String,
    pub env_vars: Option<HashMap<String, String>>,
    pub command: Option<String>,
    pub volumes: Option<V1VolumeConfig>,
    pub accelerators: Option<Vec<String>>,
    pub meters: Option<Vec<V1Meter>>,
    pub status: Option<String>,
}
// Add this function to provide a default kind value
fn default_container_kind() -> String {
    "Container".to_string()
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct V1UpdateContainer {
    pub image: Option<String>,
    pub env_vars: Option<HashMap<String, String>>,
    pub command: Option<String>,
    pub volumes: Option<V1VolumeConfig>,
    pub accelerators: Option<Vec<String>>,
    pub labels: Option<HashMap<String, String>>,
    pub cpu_request: Option<String>,
    pub memory_request: Option<String>,
    pub platform: Option<String>,
    pub meters: Option<Vec<V1Meter>>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct V1VolumeConfig {
    pub paths: Vec<V1VolumePath>,
    #[serde(default = "default_cache_dir")]
    pub cache_dir: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct V1VolumePath {
    pub source_path: String,
    pub destination_path: String,
    #[serde(default)]
    pub resync: bool,
    #[serde(default = "default_bidirectional")]
    pub bidirectional: bool,
    #[serde(default = "default_continuous")]
    pub continuous: bool,
}

// Add default functions for new fields
fn default_bidirectional() -> bool {
    true
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
