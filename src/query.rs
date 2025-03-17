// src/query.rs
use crate::container::base::ContainerStatus;
use crate::entities::containers;
use crate::models::V1ContainerStatus;
use sea_orm::sea_query::{Expr, Func};
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
        use sea_orm::sea_query::{Expr, Func};
        use sea_orm::{Condition, Value};

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

        // Build a condition for each status and combine them with OR
        let mut status_condition = Condition::any();
        for status in active_statuses {
            status_condition = status_condition.add(Expr::cust_with_values(
                "status->>'status' = $1",
                [Value::from(status)],
            ));
        }

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

    /// Return true if no other containers in the same queue
    /// are in an active or running state. Otherwise false.
    pub async fn is_queue_free(
        db: &DatabaseConnection,
        queue_name: &str,
        this_container_id: &str,
    ) -> Result<bool, DbErr> {
        // Define which statuses qualify as "active/running"
        // i.e., statuses that imply the container is still going or starting
        let active_like_statuses = vec![
            ContainerStatus::Defined.to_string(),
            ContainerStatus::Creating.to_string(),
            ContainerStatus::Created.to_string(),
            ContainerStatus::Queued.to_string(),
            ContainerStatus::Pending.to_string(),
            ContainerStatus::Running.to_string(),
            ContainerStatus::Restarting.to_string(),
        ];

        // Build a condition that checks if (status->>'status') is in one of these
        let mut active_condition = Condition::any();
        for status_str in active_like_statuses {
            active_condition = active_condition.add(Expr::cust_with_values(
                "status->>'status' = $1",
                [Value::from(status_str)],
            ));
        }

        // We'll find if there's any other record in the same queue
        // that is in an active/running state
        // (i.e., ignoring this_container_id in case it's already in that queue).
        let another_active_container = containers::Entity::find()
            .filter(containers::Column::Queue.eq(queue_name))
            .filter(containers::Column::Id.ne(this_container_id))
            .filter(active_condition)
            .one(db)
            .await?;

        Ok(another_active_container.is_none())
    }
}
