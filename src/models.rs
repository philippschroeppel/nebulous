use openai_api_rs::v1::chat_completion::{ChatCompletionRequest, ChatCompletionResponse};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::fmt;

#[derive(Debug, Serialize, Deserialize, Clone, Default, PartialEq)]
pub struct V1ErrorResponse {
    #[serde(rename = "type", default = "default_error_response_type")]
    pub response_type: String,
    pub request_id: String,
    pub error: String,
    pub traceback: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default, PartialEq)]
pub struct V1Meter {
    pub cost: Option<f64>,
    pub costp: Option<f64>,
    pub currency: String,
    pub unit: String,
    pub metric: String,
    pub json_path: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default, PartialEq)]
pub struct V1EnvVar {
    pub key: String,
    pub value: Option<String>,
    pub secret_name: Option<String>,
}

fn default_error_response_type() -> String {
    "ErrorResponse".to_string()
}

#[derive(Debug, Serialize, Deserialize, Clone, Default, PartialEq)]
pub struct V1ContainerMetaRequest {
    pub name: Option<String>,
    pub namespace: Option<String>,
    pub labels: Option<HashMap<String, String>>,
    pub owner_id: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default, PartialEq)]
pub struct V1ContainerHealthCheck {
    pub interval: Option<String>,
    pub timeout: Option<String>,
    pub retries: Option<i32>,
    pub start_period: Option<String>,
    pub path: Option<String>,
    pub port: Option<i32>,
    pub protocol: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default, PartialEq)]
pub struct V1ContainerRequest {
    #[serde(default = "default_container_kind")]
    pub kind: String,
    pub platform: Option<String>,
    pub metadata: Option<V1ResourceMetaRequest>,
    pub image: String,
    pub env: Option<Vec<V1EnvVar>>,
    pub command: Option<String>,
    pub args: Option<String>,
    pub volumes: Option<Vec<V1VolumePath>>,
    // pub local_volumes: Option<Vec<V1VolumePath>>,
    pub accelerators: Option<Vec<String>>,
    pub resources: Option<V1ContainerResources>,
    pub meters: Option<Vec<V1Meter>>,
    #[serde(default = "default_restart")]
    pub restart: String,
    pub queue: Option<String>,
    pub timeout: Option<String>,
    pub health_check: Option<V1ContainerHealthCheck>,
    pub ssh_keys: Option<Vec<V1SSHKey>>,
    pub ports: Option<Vec<V1PortRequest>>,
    pub proxy_port: Option<i16>,
    pub authz: Option<V1AuthzConfig>,
}

pub enum RestartPolicy {
    Always,
    Never,
}

fn default_restart() -> String {
    RestartPolicy::Never.to_string()
}

impl fmt::Display for RestartPolicy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RestartPolicy::Always => write!(f, "Always"),
            RestartPolicy::Never => write!(f, "Never"),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, Default, PartialEq)]
pub struct V1ContainerResources {
    pub min_cpu: Option<f64>,
    pub min_memory: Option<f64>,
    pub max_cpu: Option<f64>,
    pub max_memory: Option<f64>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default, PartialEq)]
pub struct V1ResourceMeta {
    pub name: String,
    pub namespace: String,
    pub id: String,
    pub owner: String,
    pub created_at: i64,
    pub updated_at: i64,
    pub created_by: String,
    pub owner_ref: Option<String>,
    pub labels: Option<HashMap<String, String>>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default, PartialEq)]
pub struct V1ResourceMetaRequest {
    pub name: Option<String>,
    pub namespace: Option<String>,
    pub labels: Option<HashMap<String, String>>,
    pub owner: Option<String>,
    pub owner_ref: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default, PartialEq)]
pub struct V1Port {
    pub port: u16,
    pub protocol: Option<String>,
    pub public_ip: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default, PartialEq)]
pub struct V1PortRequest {
    pub port: u16,
    pub protocol: Option<String>,
    pub public: Option<bool>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default, PartialEq)]
pub struct V1ContainerStatus {
    pub status: Option<String>,
    pub message: Option<String>,
    pub accelerator: Option<String>,
    pub public_ports: Option<Vec<V1Port>>,
    pub cost_per_hr: Option<f64>,
    pub tailnet_url: Option<String>,
    pub ready: Option<bool>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default, PartialEq)]
pub struct V1SSHKey {
    pub public_key: Option<String>,
    pub public_key_secret: Option<String>,
    pub copy_local: Option<bool>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default, PartialEq)]
pub struct V1Container {
    #[serde(default = "default_container_kind")]
    pub kind: String,
    pub platform: String,
    pub metadata: V1ResourceMeta,
    pub image: String,
    pub env: Option<Vec<V1EnvVar>>,
    pub command: Option<String>,
    pub args: Option<String>,
    pub volumes: Option<Vec<V1VolumePath>>,
    pub accelerators: Option<Vec<String>>,
    pub meters: Option<Vec<V1Meter>>,
    pub restart: String,
    pub queue: Option<String>,
    pub timeout: Option<String>,
    pub resources: Option<V1ContainerResources>,
    pub health_check: Option<V1ContainerHealthCheck>,
    pub status: Option<V1ContainerStatus>,
    pub ssh_keys: Option<Vec<V1SSHKey>>,
    pub ports: Option<Vec<V1PortRequest>>,
    pub proxy_port: Option<i16>,
    pub authz: Option<V1AuthzConfig>,
}

impl V1Container {
    /// Convert this container into a V1ResourceReference.
    pub fn to_resource_reference(&self) -> V1ResourceReference {
        V1ResourceReference {
            kind: self.kind.clone(),
            name: self.metadata.name.clone(),
            namespace: self.metadata.namespace.clone(),
        }
    }
}

// Add this function to provide a default kind value
fn default_container_kind() -> String {
    "Container".to_string()
}

#[derive(Debug, Serialize, Deserialize, Clone, Default, PartialEq)]
pub struct V1Containers {
    pub containers: Vec<V1Container>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default, PartialEq)]
pub struct V1UpdateContainer {
    pub image: Option<String>,
    pub env: Option<Vec<V1EnvVar>>,
    pub command: Option<String>,
    pub args: Option<String>,
    pub volumes: Option<Vec<V1VolumePath>>,
    pub accelerators: Option<Vec<String>>,
    pub labels: Option<HashMap<String, String>>,
    pub cpu_request: Option<String>,
    pub memory_request: Option<String>,
    pub platform: Option<String>,
    pub health_check: Option<V1ContainerHealthCheck>,
    pub meters: Option<Vec<V1Meter>>,
    pub restart: Option<String>,
    pub queue: Option<String>,
    pub timeout: Option<String>,
    pub resources: Option<V1ContainerResources>,
    pub proxy_port: Option<i16>,
    pub no_delete: Option<bool>,
    pub authz: Option<V1AuthzConfig>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default, PartialEq)]
pub struct V1VolumeConfig {
    pub paths: Vec<V1VolumePath>,
    #[serde(default = "default_cache_dir")]
    pub cache_dir: String,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default, PartialEq)]
pub enum V1VolumeDriver {
    #[default]
    RCLONE_SYNC,
    RCLONE_COPY,
    RCLONE_BISYNC,
    RCLONE_MOUNT,
}

impl std::str::FromStr for V1VolumeDriver {
    type Err = &'static str;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "RCLONE_BISYNC" => Ok(V1VolumeDriver::RCLONE_BISYNC),
            "RCLONE_SYNC" => Ok(V1VolumeDriver::RCLONE_SYNC),
            "RCLONE_COPY" => Ok(V1VolumeDriver::RCLONE_COPY),
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

#[derive(Debug, Serialize, Deserialize, Clone, Default, PartialEq)]
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
    false
}

// Add this function to provide a default cache directory
fn default_cache_dir() -> String {
    // Use a sensible default location for the cache
    format!("/nebu/cache")
}

#[derive(Debug, Serialize, Deserialize, Clone, Default, PartialEq)]
pub struct V1ContainerSearch {
    pub namespace: Option<String>,
    pub image: Option<String>,
    pub env: Option<Vec<V1EnvVar>>,
    pub command: Option<String>,
    pub args: Option<String>,
    pub volumes: Option<Vec<V1VolumePath>>,
    pub accelerators: Option<Vec<String>>,
    pub labels: Option<HashMap<String, String>>,
    pub cpu_request: Option<String>,
    pub memory_request: Option<String>,
    pub platform: Option<String>,
    pub health_check: Option<V1ContainerHealthCheck>,
    pub meters: Option<Vec<V1Meter>>,
    pub restart: Option<String>,
    pub queue: Option<String>,
    pub timeout: Option<String>,
    pub resources: Option<V1ContainerResources>,
    pub proxy_port: Option<i16>,
    pub authz: Option<V1AuthzConfig>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default, PartialEq)]
pub struct V1ProcessorStatus {
    pub status: Option<String>,
    pub message: Option<String>,
    pub pressure: Option<i32>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default, PartialEq)]
pub struct V1ScaleUp {
    pub above_pressure: Option<i32>,
    pub duration: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default, PartialEq)]
pub struct V1ScaleDown {
    pub below_pressure: Option<i32>,
    pub duration: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default, PartialEq)]
pub struct V1ScaleZero {
    pub duration: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default, PartialEq)]
pub struct V1Scale {
    pub up: Option<V1ScaleUp>,
    pub down: Option<V1ScaleDown>,
    pub zero: Option<V1ScaleZero>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default, PartialEq)]
pub struct V1Processor {
    #[serde(default = "default_processor_kind")]
    pub kind: String,
    pub metadata: V1ResourceMeta,
    pub container: Option<V1Container>,
    pub stream: Option<String>,
    pub schema: Option<Value>,
    pub common_schema: Option<String>,
    pub min_replicas: Option<i32>,
    pub max_replicas: Option<i32>,
    pub scale: Option<V1Scale>,
    pub status: Option<V1ProcessorStatus>,
}

impl V1Processor {
    /// Convert this processor into a V1ResourceReference.
    pub fn to_resource_reference(&self) -> V1ResourceReference {
        V1ResourceReference {
            kind: self.kind.clone(),
            name: self.metadata.name.clone(),
            namespace: self.metadata.namespace.clone(),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, Default, PartialEq)]
pub struct V1ProcessorRequest {
    #[serde(default = "default_processor_kind")]
    pub kind: String,
    pub metadata: V1ResourceMetaRequest,
    pub container: Option<V1Container>,
    pub stream: Option<String>,
    pub schema: Option<Value>,
    pub common_schema: Option<String>,
    pub min_replicas: Option<i32>,
    pub max_replicas: Option<i32>,
    pub scale: Option<V1Scale>,
}

fn default_processor_kind() -> String {
    "Processor".to_string()
}

#[derive(Debug, Serialize, Deserialize, Clone, Default, PartialEq)]
pub struct V1UserProfile {
    pub email: String,
    pub display_name: Option<String>,
    pub handle: Option<String>,
    pub picture: Option<String>,
    pub organization: Option<String>,
    pub role: Option<String>,
    pub external_id: Option<String>,
    pub actor: Option<String>,
    // structure is {"org_id": {"org_name": <name>, "org_role": <role>}}
    pub organizations: Option<HashMap<String, HashMap<String, String>>>,
    pub created: Option<i64>,
    pub updated: Option<i64>,
    pub token: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default, PartialEq)]
pub struct V1ContainerList {
    pub containers: Vec<V1Container>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default, PartialEq)]
pub struct V1CreateAgentKeyRequest {
    pub agent_id: String,
    pub name: String,
    pub duration: i32,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default, PartialEq)]
pub struct V1AgentKey {
    pub name: String,
    pub key: Option<String>,
    pub created: Option<i64>,
    pub valid_for: Option<i64>,
    pub org: Option<String>,
    pub role: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Default, Clone, PartialEq)]
pub struct V1Secret {
    #[serde(default = "default_secret_kind")]
    pub kind: String,
    pub metadata: V1ResourceMeta,
    pub value: Option<String>,
    pub expires_at: Option<i32>,
}

impl V1Secret {
    /// Convert this secret into a V1ResourceReference.
    pub fn to_resource_reference(&self) -> V1ResourceReference {
        V1ResourceReference {
            kind: self.kind.clone(),
            name: self.metadata.name.clone(),
            namespace: self.metadata.namespace.clone(),
        }
    }
}

fn default_secret_kind() -> String {
    "Secret".to_string()
}

#[derive(Debug, Serialize, Deserialize, Clone, Default, PartialEq)]
pub struct V1Secrets {
    pub secrets: Vec<V1Secret>,
}

/// Request body used for creating or updating a secret
#[derive(Serialize, Deserialize, Debug, Default, PartialEq)]
pub struct V1SecretRequest {
    pub metadata: V1ResourceMetaRequest,
    pub value: String,
    pub expires_at: Option<i32>,
}

//
// Authz
//

#[derive(Debug, Serialize, Deserialize, Clone, Default, PartialEq)]
pub struct V1AuthzConfig {
    pub enabled: bool,
    pub default_action: String,
    #[serde(rename = "auth_type")]
    pub auth_type: String,
    pub jwt: Option<V1AuthzJwt>,
    pub rules: Option<Vec<V1AuthzRule>>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default, PartialEq)]
pub struct V1AuthzJwt {
    pub secret_ref: Option<V1AuthzSecretRef>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default, PartialEq)]
pub struct V1AuthzSecretRef {
    pub name: String,
    pub key: String,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default, PartialEq)]
pub struct V1AuthzPathMatch {
    pub path: Option<String>,
    pub pattern: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default, PartialEq)]
pub struct V1AuthzRule {
    pub name: String,
    /// Use serde's rename to handle the reserved keyword 'match'.
    #[serde(rename = "match")]
    pub rule_match: Option<V1AuthzRuleMatch>,
    pub allow: bool,
    /// Some rules may not require field matching, so make it optional.
    pub field_match: Option<Vec<V1AuthzFieldMatch>>,
    pub path_match: Option<Vec<V1AuthzPathMatch>>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default, PartialEq)]
pub struct V1AuthzRuleMatch {
    pub roles: Option<Vec<String>>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default, PartialEq)]
pub struct V1AuthzFieldMatch {
    pub json_path: Option<String>,
    pub pattern: Option<String>,
}

#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct V1ResourceReference {
    pub kind: String,
    pub name: String,
    pub namespace: String,
}

impl V1ResourceReference {
    /// Convert a `V1ResourceReference` to a string, encoded as `name.namespace.kind`.
    pub fn to_string_encoded(&self) -> String {
        format!("{}.{}.{}", self.name, self.namespace, self.kind)
    }

    /// Parse a `V1ResourceReference` from a string in the format `name.namespace.kind`.
    pub fn from_str_encoded(encoded: &str) -> Result<Self, String> {
        let parts: Vec<&str> = encoded.split('.').collect();
        if parts.len() != 3 {
            return Err(format!(
                "Invalid reference string: expected 3 parts, got {}",
                parts.len()
            ));
        }
        Ok(Self {
            name: parts[0].to_string(),
            namespace: parts[1].to_string(),
            kind: parts[2].to_string(),
        })
    }
}

//
// Stream models
//

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct V1StreamMessage {
    #[serde(default = "kind_v1_stream_message")]
    pub kind: String,
    pub id: String,
    #[serde(default)]
    pub content: Value,
    pub created_at: i64,
    pub return_stream: Option<String>,
    pub user_id: Option<String>,
    pub organizations: Option<Value>,
    pub handle: Option<String>,
    pub adapter: Option<String>,
}

fn kind_v1_stream_message() -> String {
    "V1StreamMessage".to_string()
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct V1StreamResponseMessage {
    #[serde(default = "kind_v1_stream_response_message")]
    pub kind: String,
    pub id: String,
    #[serde(default)]
    pub content: Value,
    pub created_at: i64,
    pub user_id: Option<String>,
}

fn kind_v1_stream_response_message() -> String {
    "V1StreamResponseMessage".to_string()
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct V1OpenAIStreamMessage {
    #[serde(default = "kind_v1_openai_stream_message")]
    pub kind: String,
    pub id: String,
    pub content: ChatCompletionRequest,
    pub created_at: i64,
    pub return_stream: Option<String>,
    pub user_id: Option<String>,
    pub organizations: Option<Value>,
    pub handle: Option<String>,
    pub adapter: Option<String>,
}

fn kind_v1_openai_stream_message() -> String {
    "V1OpenAIStreamMessage".to_string()
}

#[derive(Debug, Serialize, Deserialize)]
pub struct V1OpenAIStreamResponse {
    #[serde(default = "kind_v1_openai_stream_response")]
    pub kind: String,
    pub id: String,
    pub content: ChatCompletionResponse,
    pub created_at: i64,
    pub user_id: Option<String>,
}

fn kind_v1_openai_stream_response() -> String {
    "V1OpenAIStreamResponse".to_string()
}
