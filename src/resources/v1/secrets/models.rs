use crate::models::{V1ResourceMeta, V1ResourceMetaRequest, V1ResourceReference};
use serde::{Deserialize, Serialize};

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
