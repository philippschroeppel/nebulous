// src/query.rs
use crate::entities::containers;
use crate::models::V1ContainerStatus;
use sea_orm::sea_query::Expr;
use sea_orm::Value;
use sea_orm::*;
use sea_orm::{ColumnTrait, DatabaseConnection, DbErr, EntityTrait, QueryFilter};

pub struct Query;

impl Query {
    pub async fn find_containers_by_owners(
        db: &DatabaseConnection,
        owner_ids: &[&str],
    ) -> Result<Vec<containers::Model>, DbErr> {
        containers::Entity::find()
            .filter(containers::Column::OwnerId.is_in(owner_ids.iter().copied()))
            .all(db)
            .await
    }
    pub async fn find_container_by_id(
        db: &DatabaseConnection,
        id: String,
    ) -> Result<Option<containers::Model>, DbErr> {
        containers::Entity::find_by_id(id).one(db).await
    }

    pub async fn find_container_by_id_and_owners(
        db: &DatabaseConnection,
        id: &str,
        owner_ids: &[&str],
    ) -> Result<containers::Model, DbErr> {
        let result = containers::Entity::find()
            .filter(containers::Column::Id.eq(id))
            .filter(containers::Column::OwnerId.is_in(owner_ids.iter().copied()))
            .one(db)
            .await?;

        result.ok_or(DbErr::RecordNotFound(format!(
            "Container with id '{}' not found for the specified owners",
            id
        )))
    }

    /// Fetches an agent key from the secrets table by container ID
    pub async fn get_agent_key(
        db: &DatabaseConnection,
        container_id: String,
    ) -> Result<Option<String>, DbErr> {
        use crate::entities::secrets;

        // Look for a secret with the same ID as the container
        let secret = secrets::Entity::find_by_id(container_id).one(db).await?;

        // If found, decrypt the value and return it
        if let Some(secret) = secret {
            match secret.decrypt_value() {
                Ok(value) => Ok(Some(value)),
                Err(e) => Err(DbErr::Custom(format!("Failed to decrypt agent key: {}", e))),
            }
        } else {
            // No secret found for this container ID
            Ok(None)
        }
    }

    /// Fetches all containers from the database
    pub async fn find_all_containers(
        db: &DatabaseConnection,
    ) -> Result<Vec<containers::Model>, DbErr> {
        containers::Entity::find().all(db).await
    }

    /// Fetches the status of a container by its ID
    pub async fn get_container_status(
        db: &DatabaseConnection,
        container_id: &str,
    ) -> Result<Option<V1ContainerStatus>, DbErr> {
        let container = containers::Entity::find_by_id(container_id)
            .select_only()
            .column(containers::Column::Status)
            .one(db)
            .await?;

        // Deserialize the status string into V1ContainerStatus if container exists
        Ok(container.and_then(|c| {
            c.status
                .map(|json_value| serde_json::from_value::<V1ContainerStatus>(json_value))
                .transpose()
                .ok()
                .flatten()
        }))
    }

    /// Fetches all active containers from the database by inspecting the "status" key in the status JSON.
    pub async fn find_all_active_containers(
        db: &DatabaseConnection,
    ) -> Result<Vec<containers::Model>, DbErr> {
        use crate::container::base::ContainerStatus;

        // Convert your set of active statuses to strings
        let active_statuses = vec![
            ContainerStatus::Defined.to_string(),
            ContainerStatus::Creating.to_string(),
            ContainerStatus::Paused.to_string(),
            ContainerStatus::Queued.to_string(),
            ContainerStatus::Running.to_string(),
            ContainerStatus::Pending.to_string(),
            ContainerStatus::Restarting.to_string(),
            ContainerStatus::Created.to_string(),
        ];

        // Fold the list of acceptable statuses into a single OR condition
        // This checks the JSON column `status->>'status'`
        let status_condition = active_statuses.iter().fold(Condition::any(), |cond, s| {
            cond.add(Expr::cust_with_values(
                "status->>'status' = ?",
                vec![Value::String(Some(Box::new(s.to_string())))],
            ))
        });
        containers::Entity::find()
            .filter(status_condition)
            .all(db)
            .await
    }

    /// Fetches all containers with a specific status
    pub async fn find_containers_by_status(
        db: &DatabaseConnection,
        status: crate::container::base::ContainerStatus,
    ) -> Result<Vec<containers::Model>, DbErr> {
        containers::Entity::find()
            .filter(containers::Column::Status.eq(status.to_string()))
            .all(db)
            .await
    }
}
