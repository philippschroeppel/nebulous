use crate::auth::db;
use crate::auth::models;
use crate::auth::models::SanitizedApiKey;
use argon2::{
    password_hash::{PasswordHash, PasswordVerifier, SaltString},
    Argon2, PasswordHasher,
};
use base64::{engine::general_purpose, Engine as _};
use hex;
use rand::rngs::OsRng;
use rand::RngCore;
use sea_orm::entity::*;
use sea_orm::DatabaseConnection;
use uuid::Uuid;

pub async fn get_api_key(
    db_conn: &DatabaseConnection,
    id: &str,
) -> Result<SanitizedApiKey, Box<dyn std::error::Error>> {
    let result = db::Entity::find_by_id(id).one(db_conn).await?;
    // TODO: Update usage timestamp
    match result {
        Some(api_key) => Ok(models::ApiKey::from(api_key).into()),
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
) -> Result<String, Box<dyn std::error::Error>> {
    let mut raw_key = [0u8; 32];
    OsRng.fill_bytes(&mut raw_key);
    let key = general_purpose::STANDARD.encode(&raw_key);
    let salt = SaltString::generate(&mut OsRng);

    let argon2 = Argon2::default();
    let hash = match argon2.hash_password(key.as_str().as_bytes(), salt.as_salt()) {
        Ok(hash) => hash.to_string(),
        Err(_) => return Err("Failed to hash API key.".to_string().into()),
    };

    let id = Uuid::new_v4().to_string();
    let api_key = models::ApiKey::new(id.clone(), hash);
    let new_api_key: db::ActiveModel = db::Model::from(api_key).into();
    new_api_key.insert(db_conn).await?;

    Ok(format!("nebu-{}.{}", id, key.as_str().to_string()))
}

pub async fn validate_api_key(
    db_conn: &DatabaseConnection,
    provided_key: &str,
) -> Result<bool, Box<dyn std::error::Error + Sync + Send>> {
    if let Some(full_key) = provided_key.strip_prefix("nebu-") {
        let parts: Vec<&str> = full_key.split('.').collect();
        if parts.len() == 2 {
            let (id, key) = (parts[0], parts[1]);
            if let Some(api_key) = db::Entity::find_by_id(id).one(db_conn).await? {
                if api_key.revoked_at.is_some() {
                    return Ok(false);
                }
                // TODO: Update usage timestamp
                let parsed_hash = match PasswordHash::new(&api_key.hash) {
                    Ok(hash) => hash,
                    Err(_) => return Ok(false),
                };
                let argon2 = Argon2::default();
                return Ok(argon2.verify_password(key.as_bytes(), &parsed_hash).is_ok());
            }
        }
    }
    Ok(false)
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
            Ok(models::ApiKey::from(result).into())
        }
        None => Err("Unknown API key.".to_string().into()),
    }
}
