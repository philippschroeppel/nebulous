// src/query.rs
use crate::entities::containers;
use crate::models::VolumeConfig;
use sea_orm::*;
use sea_orm::{ColumnTrait, DatabaseConnection, DbErr, EntityTrait, QueryFilter};
use serde_json::from_str;

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

    /// Fetches all containers from the database
    pub async fn find_all_containers(
        db: &DatabaseConnection,
    ) -> Result<Vec<containers::Model>, DbErr> {
        containers::Entity::find().all(db).await
    }
}
