use crate::auth::db;
use crate::auth::models;
use crate::auth::models::SanitizedApiKey;
use sea_orm::entity::*;
use sea_orm::DatabaseConnection;

pub async fn get_api_key(
    db_conn: &DatabaseConnection,
    id: &str,
) -> Result<models::ApiKey, Box<dyn std::error::Error>> {
    let result = db::Entity::find_by_id(id).one(db_conn).await?;
    // TODO: Update usage timestamp
    match result {
        Some(api_key) => Ok(models::ApiKey::from(api_key)),
        None => Err("API key not found".into()),
    }
}

pub async fn get_sanitized_api_key(
    db_conn: &DatabaseConnection,
    id: &str,
) -> Result<SanitizedApiKey, Box<dyn std::error::Error>> {
    let api_key = get_api_key(db_conn, id).await?;
    Ok(models::SanitizedApiKey::from(api_key))
}

pub async fn list_api_keys(
    db_conn: &DatabaseConnection,
) -> Result<Vec<SanitizedApiKey>, Box<dyn std::error::Error>> {
    let result = db::Entity::find().all(db_conn).await?;
    Ok(result
        .into_iter()
        .map(models::ApiKey::from)
        .map(models::SanitizedApiKey::from)
        .collect())
}

pub async fn generate_api_key(
    db_conn: &DatabaseConnection,
) -> Result<models::ApiKey, Box<dyn std::error::Error>> {
    let new_id = "abcd1234".to_string(); // TODO: Generate a unique ID
    let new_key = "abcd1234".to_string(); // TODO: Generate a unique key

    let api_key = models::ApiKey::new(new_id, new_key);
    let new_api_key: db::ActiveModel = db::Model::from(api_key).into();
    let result = new_api_key.insert(db_conn).await?;
    Ok(models::ApiKey::from(result))
}

pub async fn revoke_api_key(
    db_conn: &DatabaseConnection,
    id: &str,
) -> Result<SanitizedApiKey, Box<dyn std::error::Error>> {
    let result = db::Entity::find_by_id(id).one(db_conn).await?;
    match result {
        Some(api_key) => {
            let mut current_api_key = models::ApiKey::from(api_key);
            current_api_key.revoke();
            let revoked_api_key: db::ActiveModel = db::Model::from(current_api_key).into();
            let result = revoked_api_key.update(db_conn).await?;
            Ok(models::SanitizedApiKey::from(models::ApiKey::from(result)))
        }
        None => Err("Unknown API key.".to_string().into()),
    }
}
