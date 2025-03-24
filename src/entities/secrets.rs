use aes_gcm::{
    aead::{Aead, KeyInit},
    Aes256Gcm, Nonce,
};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use rand::{rngs::OsRng, RngCore};
use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};
use std::env;

#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "secrets")]
pub struct Model {
    #[sea_orm(primary_key, column_type = "Text", auto_increment = false)]
    pub id: String,
    pub name: String,
    pub namespace: String,
    #[sea_orm(unique, column_type = "Text")]
    pub full_name: String,
    pub owner: String,
    pub owner_ref: Option<String>,
    pub encrypted_value: String,
    pub nonce: String, // Store the nonce used for encryption
    pub labels: Option<Json>,
    pub created_by: Option<String>,
    pub updated_at: DateTimeWithTimeZone,
    pub created_at: DateTimeWithTimeZone,
    pub expires_at: Option<i32>,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}

impl Model {
    // Get encryption key from environment
    fn get_encryption_key() -> Result<[u8; 32], String> {
        let key = env::var("NEBU_ENCRYPTION_KEY")
            .map_err(|_| "NEBU_ENCRYPTION_KEY environment variable not set".to_string())?;

        // Ensure the key is exactly 32 bytes (256 bits)
        if key.len() != 32 {
            return Err("NEBU_ENCRYPTION_KEY must be exactly 32 bytes".to_string());
        }

        let mut result = [0u8; 32];
        result.copy_from_slice(key.as_bytes());
        Ok(result)
    }

    // Encrypt a value
    pub fn encrypt_value(value: &str) -> Result<(String, String), String> {
        let key = Self::get_encryption_key()?;
        let cipher = Aes256Gcm::new_from_slice(&key)
            .map_err(|e| format!("Failed to create cipher: {}", e))?;

        // Generate a random nonce
        let mut nonce_bytes = [0u8; 12]; // 96 bits
        OsRng.fill_bytes(&mut nonce_bytes);
        let nonce = Nonce::from_slice(&nonce_bytes);

        // Encrypt the value
        let ciphertext = cipher
            .encrypt(nonce, value.as_bytes())
            .map_err(|e| format!("Encryption failed: {}", e))?;

        // Encode as base64 for storage
        let encrypted_value = BASE64.encode(ciphertext);
        let nonce_value = BASE64.encode(nonce_bytes);

        Ok((encrypted_value, nonce_value))
    }

    // Decrypt a value
    pub fn decrypt_value(&self) -> Result<String, String> {
        let key = Self::get_encryption_key()?;
        let cipher = Aes256Gcm::new_from_slice(&key)
            .map_err(|e| format!("Failed to create cipher: {}", e))?;

        // Decode the nonce and ciphertext from base64
        let nonce_bytes = BASE64
            .decode(self.nonce.as_bytes())
            .map_err(|e| format!("Failed to decode nonce: {}", e))?;
        let ciphertext = BASE64
            .decode(self.encrypted_value.as_bytes())
            .map_err(|e| format!("Failed to decode ciphertext: {}", e))?;

        if nonce_bytes.len() != 12 {
            return Err("Invalid nonce length".to_string());
        }

        let nonce = Nonce::from_slice(&nonce_bytes);

        // Decrypt the value
        let plaintext = cipher
            .decrypt(nonce, ciphertext.as_ref())
            .map_err(|e| format!("Decryption failed: {}", e))?;

        String::from_utf8(plaintext)
            .map_err(|e| format!("Failed to convert decrypted bytes to string: {}", e))
    }

    // Create a new secret with encrypted value
    pub fn new(
        id: String,
        name: String,
        namespace: String,
        owner: String,
        value: &str,
        created_by: Option<String>,
        labels: Option<Json>,
        expires_at: Option<i32>,
    ) -> Result<Self, String> {
        let (encrypted_value, nonce) = Self::encrypt_value(value)?;
        let now = chrono::Utc::now().into();

        Ok(Self {
            id,
            name: name.clone(),
            namespace: namespace.clone(),
            full_name: format!("{namespace}/{name}"),
            owner,
            owner_ref: None,
            encrypted_value,
            nonce,
            labels,
            created_by,
            updated_at: now,
            created_at: now,
            expires_at,
        })
    }
}
