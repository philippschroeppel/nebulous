// src/entities/training_job.rs

use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};
use serde_json::Value as Json;
use std::collections::HashMap;

use crate::resources::v1::containers::models::V1Container;
use crate::resources::v1::processors::models::{V1Processor, V1ProcessorStatus, V1Scale};

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "processors")]
pub struct Model {
    #[sea_orm(primary_key, column_type = "Text", auto_increment = false)]
    pub id: String,
    pub namespace: String,
    pub name: String,
    #[sea_orm(unique, column_type = "Text")]
    pub full_name: String,
    pub labels: Option<Json>,
    pub owner: String,
    pub container: Option<Json>,
    pub cluster: Option<Json>,
    pub scale: Option<Json>,
    pub min_replicas: Option<i32>,
    pub max_replicas: Option<i32>,
    pub desired_replicas: Option<i32>,
    pub stream: Option<String>,
    pub schema: Option<Json>,
    pub common_schema: Option<String>,
    pub status: Option<Json>,
    pub resource_name: Option<String>,
    pub resource_namespace: Option<String>,
    pub created_by: Option<String>,
    pub desired_status: Option<String>,
    pub controller_data: Option<Json>,
    pub updated_at: DateTimeWithTimeZone,
    pub created_at: DateTimeWithTimeZone,
}

// The Relation enum is required, even if empty.
#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

// The ActiveModelBehavior is required, even if empty.
impl ActiveModelBehavior for ActiveModel {}

impl Model {
    /// Attempt to parse `status` into a `V1ProcessorStatus`.
    pub fn parse_status(&self) -> Result<Option<V1ProcessorStatus>, serde_json::Error> {
        if let Some(json_value) = &self.status {
            serde_json::from_value(json_value.clone()).map(Some)
        } else {
            Ok(None)
        }
    }

    pub fn parse_scale(&self) -> Result<Option<V1Scale>, serde_json::Error> {
        if let Some(json_value) = &self.scale {
            serde_json::from_value(json_value.clone()).map(Some)
        } else {
            Ok(None)
        }
    }

    pub fn parse_container(&self) -> Result<Option<V1Container>, serde_json::Error> {
        if let Some(json_value) = &self.container {
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

    /// Construct a full V1Container from the current model row.
    /// Returns a serde_json Error if any JSON parsing in subfields fails.
    pub fn to_v1_processor(&self) -> Result<V1Processor, serde_json::Error> {
        let scale = self.parse_scale()?;
        let container = self.parse_container()?;
        let status = self.parse_status()?;
        let labels = self.parse_labels()?;

        // Build metadata; fill with defaults or unwrap as needed
        let metadata = crate::models::V1ResourceMeta {
            name: self.name.clone(),
            namespace: self.namespace.clone(),
            id: self.id.clone(),
            owner: self.owner.clone(),
            owner_ref: None,
            created_at: self.created_at.timestamp(),
            updated_at: self.updated_at.timestamp(),
            created_by: self.created_by.clone().unwrap_or_default(),
            labels,
        };

        // Construct final V1Container
        let processor = V1Processor {
            kind: "Processor".to_owned(), // or use default_container_kind() if needed
            metadata,
            stream: self.stream.clone(),
            schema: self.schema.clone(),
            common_schema: self.common_schema.clone(),
            min_replicas: self.min_replicas,
            max_replicas: self.max_replicas,
            scale,
            container,
            status,
        };

        Ok(processor)
    }
}
