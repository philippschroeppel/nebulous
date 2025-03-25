// src/entities/training_job.rs

use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};
use serde_json::Value as Json;
use std::collections::HashMap;

use crate::models::{
    V1Container, V1ContainerResources, V1ContainerStatus, V1EnvVar, V1Meter, V1PortRequest,
    V1ResourceMeta, V1SSHKey, V1VolumePath,
};

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "containers")]
pub struct Model {
    #[sea_orm(primary_key, column_type = "Text", auto_increment = false)]
    pub id: String,
    pub namespace: String,
    pub name: String,
    #[sea_orm(unique, column_type = "Text")]
    pub full_name: String,
    pub owner: String,
    pub owner_ref: Option<String>,
    pub image: String,
    pub env: Option<Json>,
    pub volumes: Option<Json>,
    pub local_volumes: Option<Json>,
    pub accelerators: Option<Vec<String>>,
    pub cpu_request: Option<String>,
    pub memory_request: Option<String>,
    pub status: Option<Json>,
    pub platform: Option<String>,
    pub platforms: Option<Vec<String>>,
    pub resource_name: Option<String>,
    pub resource_namespace: Option<String>,
    pub resource_cost_per_hr: Option<f64>,
    pub command: Option<String>,
    pub args: Option<String>,
    pub labels: Option<Json>,
    pub meters: Option<Json>,
    pub queue: Option<String>,
    pub ports: Option<Json>,
    pub public_ip: bool,
    pub timeout: Option<String>,
    pub resources: Option<Json>,
    pub restart: String,
    pub public_addr: Option<String>,
    pub tailnet_ip: Option<String>,
    pub created_by: Option<String>,
    pub desired_status: Option<String>,
    pub controller_data: Option<Json>,
    pub container_user: Option<String>,
    pub ssh_keys: Option<Json>,
    pub updated_at: DateTimeWithTimeZone,
    pub created_at: DateTimeWithTimeZone,
}

// The Relation enum is required, even if empty.
#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

// The ActiveModelBehavior is required, even if empty.
impl ActiveModelBehavior for ActiveModel {}

impl Model {
    /// Attempt to parse `env` into a vector of `V1EnvVar`.
    pub fn parse_env(&self) -> Result<Option<Vec<V1EnvVar>>, serde_json::Error> {
        if let Some(json_value) = &self.env {
            serde_json::from_value(json_value.clone()).map(Some)
        } else {
            Ok(None)
        }
    }

    /// Attempt to parse `volumes` into a vector of `V1VolumePath`.
    pub fn parse_volumes(&self) -> Result<Option<Vec<V1VolumePath>>, serde_json::Error> {
        if let Some(json_value) = &self.volumes {
            serde_json::from_value(json_value.clone()).map(Some)
        } else {
            Ok(None)
        }
    }

    /// Attempt to parse `status` into a `V1ContainerStatus`.
    pub fn parse_status(&self) -> Result<Option<V1ContainerStatus>, serde_json::Error> {
        if let Some(json_value) = &self.status {
            serde_json::from_value(json_value.clone()).map(Some)
        } else {
            Ok(None)
        }
    }

    /// Attempt to parse `labels` into a `HashMap<String, String>`.
    pub fn parse_labels(&self) -> Result<Option<HashMap<String, String>>, serde_json::Error> {
        if let Some(json_value) = &self.labels {
            serde_json::from_value(json_value.clone()).map(Some)
        } else {
            Ok(None)
        }
    }

    /// Attempt to parse `meters` into a vector of `V1Meter`.
    pub fn parse_meters(&self) -> Result<Option<Vec<V1Meter>>, serde_json::Error> {
        if let Some(json_value) = &self.meters {
            serde_json::from_value(json_value.clone()).map(Some)
        } else {
            Ok(None)
        }
    }

    /// Attempt to parse `controller_data` into any desired struct T that implements Deserialize.
    /// For example, you could parse to a generic `serde_json::Value` or a custom struct.
    pub fn parse_controller_data<T: serde::de::DeserializeOwned>(
        &self,
    ) -> Result<Option<T>, serde_json::Error> {
        if let Some(json_value) = &self.controller_data {
            serde_json::from_value(json_value.clone()).map(Some)
        } else {
            Ok(None)
        }
    }

    /// Attempt to parse `resources` into a `V1ContainerResources`.
    pub fn parse_resources(&self) -> Result<Option<V1ContainerResources>, serde_json::Error> {
        if let Some(json_value) = &self.resources {
            serde_json::from_value(json_value.clone()).map(Some)
        } else {
            Ok(None)
        }
    }

    /// Attempt to parse `ssh_keys` into a vector of `V1SSHKey`.
    pub fn parse_ssh_keys(&self) -> Result<Option<Vec<V1SSHKey>>, serde_json::Error> {
        if let Some(json_value) = &self.ssh_keys {
            serde_json::from_value(json_value.clone()).map(Some)
        } else {
            Ok(None)
        }
    }

    /// Attempt to parse `ports` into a vector of `V1PortRequest`.
    pub fn parse_ports(&self) -> Result<Option<Vec<V1PortRequest>>, serde_json::Error> {
        if let Some(json_value) = &self.ports {
            serde_json::from_value(json_value.clone()).map(Some)
        } else {
            Ok(None)
        }
    }

    /// Construct a full V1Container from the current model row.
    /// Returns a serde_json Error if any JSON parsing in subfields fails.
    pub fn to_v1_container(&self) -> Result<V1Container, serde_json::Error> {
        let env = self.parse_env()?;
        let volumes = self.parse_volumes()?;
        let status = self.parse_status()?;
        let labels = self.parse_labels()?;
        let meters = self.parse_meters()?;
        let resources = self.parse_resources()?;
        let ssh_keys = self.parse_ssh_keys()?;
        let ports = self.parse_ports()?;
        // Build metadata; fill with defaults or unwrap as needed
        let metadata = crate::models::V1ResourceMeta {
            name: self.name.clone(),
            namespace: self.namespace.clone(),
            id: self.id.clone(),
            owner: self.owner.clone(),
            owner_ref: self.owner_ref.clone(),
            created_at: self.created_at.timestamp(),
            updated_at: self.updated_at.timestamp(),
            created_by: self.created_by.clone().unwrap_or_default(),
            labels,
        };

        // Construct final V1Container
        let container = crate::models::V1Container {
            kind: "Container".to_owned(), // or use default_container_kind() if needed
            platform: self.platform.clone().unwrap_or_default(),
            metadata,
            image: self.image.clone(),
            env,
            command: self.command.clone(),
            args: self.args.clone(),
            volumes,
            accelerators: self.accelerators.clone(),
            meters,
            restart: self.restart.clone(),
            queue: self.queue.clone(),
            timeout: self.timeout.clone(),
            status,
            resources,
            ssh_keys,
            ports: ports.clone(),
            public_ip: self.public_ip.clone(),
        };

        Ok(container)
    }
}
