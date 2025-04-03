use crate::config::CONFIG;
use crate::entities::namespaces;
use anyhow::Result;
use sea_orm::{ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter};

pub async fn auth_ns(
    db_pool: &DatabaseConnection,
    owner_ids: &Vec<String>,
    namespace: &str,
) -> Result<String> {
    let namespace_entity = match namespaces::Entity::find()
        .filter(namespaces::Column::Name.eq(namespace))
        .one(db_pool)
        .await?
    {
        Some(namespace) => namespace,
        None => return Err(anyhow::anyhow!("Namespace not found")),
    };

    if namespace == "root" {
        let root_owner = CONFIG.root_owner.clone();
        if !owner_ids.contains(&root_owner) {
            return Err(anyhow::anyhow!("User not authorized to access namespace"));
        }
    }

    if !owner_ids.contains(&namespace_entity.owner) {
        return Err(anyhow::anyhow!("User not authorized to access namespace"));
    }

    Ok(namespace_entity.owner)
}
