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

    // Mutation to update only the container status
    pub async fn update_container_status(
        db: &DatabaseConnection,
        id: String,
        status: String,
        message: Option<String>,
    ) -> Result<containers::Model, DbErr> {
        let container = containers::Entity::find_by_id(id)
            .one(db)
            .await?
            .ok_or(DbErr::Custom("Container not found".to_string()))?;

        let mut container: containers::ActiveModel = container.into();

        let status = V1ContainerStatus {
            status: Some(status),
            message: message,
        };

        container.status = Set(Some(json!(status).into()));
        container.updated_at = Set(chrono::Utc::now().into());

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
