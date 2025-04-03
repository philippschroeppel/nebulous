use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};
use serde_json;

#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "volumes")]
pub struct Model {
    #[sea_orm(primary_key, column_type = "Text", auto_increment = false)]
    pub id: String,
    pub name: String,
    pub namespace: String,
    #[sea_orm(unique, column_type = "Text")]
    pub full_name: String,
    pub owner: String,
    pub owner_ref: Option<String>,
    pub source: String,
    pub labels: Option<Json>,
    pub created_by: String,
    pub updated_at: DateTimeWithTimeZone,
    pub created_at: DateTimeWithTimeZone,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}

impl Model {
    // Create a new secret with encrypted value
    pub fn new(
        id: String,
        name: String,
        namespace: String,
        owner: String,
        created_by: String,
        labels: Option<Json>,
        source: String,
    ) -> Result<Self, String> {
        let now = chrono::Utc::now().into();

        Ok(Self {
            id,
            name: name.clone(),
            namespace: namespace.clone(),
            full_name: format!("{namespace}/{name}"),
            owner,
            owner_ref: None,
            source,
            labels,
            created_by,
            updated_at: now,
            created_at: now,
        })
    }

    pub fn to_v1(&self) -> crate::resources::v1::volumes::models::V1Volume {
        crate::resources::v1::volumes::models::V1Volume {
            kind: "Volume".to_string(),
            metadata: crate::models::V1ResourceMeta {
                id: self.id.clone(),
                name: self.name.clone(),
                namespace: self.namespace.clone(),
                labels: self
                    .labels
                    .as_ref()
                    .map(|json| serde_json::from_value(json.clone()).unwrap_or_default()),
                owner: self.owner.clone(),
                owner_ref: self.owner_ref.clone(),
                created_by: self.created_by.clone(),
                created_at: self.created_at.timestamp(),
                updated_at: self.updated_at.timestamp(),
            },
            source: self.source.clone(),
        }
    }
}
