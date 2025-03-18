use crate::entities::containers;
use crate::models::{V1ContainerStatus, V1UpdateContainer};
use sea_orm::prelude::Json;
use sea_orm::*;
use serde_json::json;

pub struct Mutation;

impl Mutation {
    pub async fn create_container(
        db: &DatabaseConnection,
        form_data: containers::ActiveModel,
    ) -> Result<containers::Model, DbErr> {
        form_data.insert(db).await
    }

    /// Mutation to update the resource_name field in a container
    pub async fn update_container_resource_name(
        db: &DatabaseConnection,
        id: String,
        resource_name: String,
    ) -> Result<containers::Model, DbErr> {
        let container = containers::Entity::find_by_id(id)
            .one(db)
            .await?
            .ok_or(DbErr::Custom("Container not found".to_string()))?;

        let mut container: containers::ActiveModel = container.into();

        container.resource_name = Set(Some(resource_name));
        container.updated_at = Set(chrono::Utc::now().into());

        container.update(db).await
    }

    /// Mutation to update the resource_cost_per_hr field in a container
    pub async fn update_container_resource_cost_per_hr(
        db: &DatabaseConnection,
        id: String,
        resource_cost_per_hr: f64,
    ) -> Result<containers::Model, DbErr> {
        let container = containers::Entity::find_by_id(id)
            .one(db)
            .await?
            .ok_or(DbErr::Custom("Container not found".to_string()))?;

        let mut container: containers::ActiveModel = container.into();

        container.resource_cost_per_hr = Set(Some(resource_cost_per_hr));
        container.updated_at = Set(chrono::Utc::now().into());

        container.update(db).await
    }

    /// Mutation to update the container "pod IP"
    pub async fn update_container_pod_ip(
        db: &DatabaseConnection,
        id: String,
        pod_ip: Option<String>,
    ) -> Result<containers::Model, DbErr> {
        let container = containers::Entity::find_by_id(id)
            .one(db)
            .await?
            .ok_or(DbErr::Custom("Container not found".to_string()))?;

        let mut container: containers::ActiveModel = container.into();

        container.public_addr = Set(pod_ip);
        container.updated_at = Set(chrono::Utc::now().into());

        container.update(db).await
    }

    // Mutation to update only the container status
    pub async fn update_container_status(
        db: &DatabaseConnection,
        id: String,
        status: Option<String>,
        message: Option<String>,
        accelerator: Option<String>,
        public_ip: Option<String>,
    ) -> Result<containers::Model, DbErr> {
        let container = containers::Entity::find_by_id(id)
            .one(db)
            .await?
            .ok_or(DbErr::Custom("Container not found".to_string()))?;

        let mut container: containers::ActiveModel = container.into();

        // 1. Parse any existing status from the database
        let mut existing_status = match &container.status {
            ActiveValue::Set(Some(val)) => {
                serde_json::from_value::<V1ContainerStatus>(val.clone()).unwrap_or_default()
            }
            _ => V1ContainerStatus::default(),
        };

        // 2. Merge in only the new fields
        if let Some(s) = status {
            existing_status.status = Some(s);
        }
        if let Some(m) = message {
            existing_status.message = Some(m);
        }
        if let Some(a) = accelerator {
            existing_status.accelerator = Some(a);
        }
        if let Some(ip) = public_ip {
            existing_status.public_ip = Some(ip);
        }

        // 3. Store the merged status back as JSON
        container.status = Set(Some(serde_json::json!(existing_status)));
        container.updated_at = Set(chrono::Utc::now().into());

        // 4. Update in the database
        container.update(db).await
    }

    // Mutation to update multiple container fields
    pub async fn update_container(
        db: &DatabaseConnection,
        id: String,
        update_data: V1UpdateContainer,
    ) -> Result<containers::Model, DbErr> {
        let container = containers::Entity::find_by_id(id)
            .one(db)
            .await?
            .ok_or(DbErr::Custom("Container not found".to_string()))?;

        let mut container: containers::ActiveModel = container.into();

        if let Some(image) = update_data.image {
            container.image = Set(image);
        }

        if let Some(env_vars) = update_data.env_vars {
            container.env_vars = Set(Some(json!(env_vars).into()));
        }

        if let Some(command) = update_data.command {
            container.command = Set(Some(command));
        }

        if let Some(volumes) = update_data.volumes {
            container.volumes = Set(Some(json!(volumes).into()));
        }

        if let Some(accelerators) = update_data.accelerators {
            container.accelerators = Set(Some(accelerators));
        }

        if let Some(labels) = update_data.labels {
            container.labels = Set(Some(json!(labels).into()));
        }

        if let Some(cpu_request) = update_data.cpu_request {
            container.cpu_request = Set(Some(cpu_request));
        }

        if let Some(memory_request) = update_data.memory_request {
            container.memory_request = Set(Some(memory_request));
        }

        if let Some(platform) = update_data.platform {
            container.platform = Set(Some(platform));
        }

        // Always update the updated_at timestamp
        container.updated_at = Set(chrono::Utc::now().into());

        container.update(db).await
    }

    // Mutation to delete a container by ID
    pub async fn delete_container(
        db: &DatabaseConnection,
        id: String,
    ) -> Result<DeleteResult, DbErr> {
        let result = containers::Entity::delete_by_id(id).exec(db).await?;

        // Check if any row was actually deleted
        if result.rows_affected == 0 {
            return Err(DbErr::Custom("Container not found".to_string()));
        }

        Ok(result)
    }
}
