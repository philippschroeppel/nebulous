use nebulous::auth::server::handlers::{ApiKeyRequest, RawApiKeyResponse, ApiKeyListResponse};
use std::error::Error;
use nebulous::auth::models::SanitizedApiKey;
use crate::commands::request::{server_request};

// TODO: Make the auth server's port configurable
const SERVER: &str = "http://localhost:8080";


fn pretty_print_api_key(api_key: SanitizedApiKey) {
    println!("ID: {}", api_key.id);
    println!("Active: {}", api_key.is_active);
    println!("Created at: {}", api_key.created_at.to_string());
    println!("Last used at: {}", api_key.last_used_at.map_or("N/A".to_string(), |dt| dt.to_string()));
    println!("Revoked at: {}", api_key.revoked_at.map_or("N/A".to_string(), |dt| dt.to_string()));
    println!();
}

pub async fn list_api_keys() -> Result<(), Box<dyn Error>> {
    let response = server_request("/auth/api-keys", reqwest::Method::GET, None).await?;
    for api_key in response.json::<ApiKeyListResponse>().await?.api_keys {
        pretty_print_api_key(api_key);
    };
    Ok(())
}

pub async fn get_api_key(id: &str) -> Result<(), Box<dyn Error>> {
    let path = format!("/api-key/{}", id);
    let response= server_request(path.as_str(), reqwest::Method::GET, None).await?;
    let api_key = response.json::<SanitizedApiKey>().await?;
    pretty_print_api_key(api_key);
    Ok(())
}

pub async fn generate_api_key() -> Result<(), Box<dyn Error>> {
    let url = format!("{}/api-key/generate", SERVER);
    match reqwest::Client::new().get(&url).send().await {
        Ok(response) => {
            let api_key = response.json::<RawApiKeyResponse>().await?;
            println!("Generated a new API key:\n");
            println!("{}", api_key.api_key);
            println!(
                "\nPlease store this key securely. It cannot be displayed in plaintext again."
            );
        }
        Err(e) => {
            eprintln!("Error sending request: {}.", e);
            eprintln!("Note that the auth server is only reachable on localhost.");
        }
    }
    Ok(())
}

pub async fn revoke_api_key(id: &str) -> Result<(), Box<dyn Error>> {
    let url = format!("{}/api-key/revoke", SERVER);
    let payload = ApiKeyRequest { id: id.to_string() };
    match reqwest::Client::new()
        .post(&url)
        .json(&payload)
        .send()
        .await
    {
        Ok(response) => {
            if response.status().is_success() {
                println!("API key revoked successfully.");
            } else {
                eprintln!("Failed to revoke API key: {}", response.status());
            }
        }
        Err(e) => {
            eprintln!("Error sending request: {}.", e);
            eprintln!("Note that the auth server is only reachable on localhost.");
        }
    }
    Ok(())
}
