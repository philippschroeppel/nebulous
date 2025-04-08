use openai_api_rs::v1::chat_completion::{ChatCompletionRequest, ChatCompletionResponse};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

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

#[derive(Debug, Serialize, Deserialize, Clone, Default, PartialEq)]
pub struct V1Meter {
    pub cost: Option<f64>,
    pub costp: Option<f64>,
    pub currency: String,
    pub unit: String,
    pub metric: String,
    pub json_path: Option<String>,
}

//
// Stream models
//

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct V1StreamData {
    #[serde(default)]
    pub content: Value,
    pub wait: Option<bool>,
}

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
    pub orgs: Option<Value>,
    pub handle: Option<String>,
    pub adapter: Option<String>,
}

fn kind_v1_stream_message() -> String {
    "StreamMessage".to_string()
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct V1StreamResponseMessage {
    #[serde(default = "kind_v1_stream_response_message")]
    pub kind: String,
    pub id: String,
    #[serde(default)]
    pub content: Value,
    pub status: Option<String>,
    pub created_at: i64,
    pub user_id: Option<String>,
}

fn kind_v1_stream_response_message() -> String {
    "StreamResponseMessage".to_string()
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
    pub orgs: Option<Value>,
    pub handle: Option<String>,
    pub adapter: Option<String>,
}

fn kind_v1_openai_stream_message() -> String {
    "OpenAIStreamMessage".to_string()
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
    "OpenAIStreamResponse".to_string()
}
