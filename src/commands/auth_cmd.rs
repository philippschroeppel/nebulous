use nebulous::auth::server::handlers::{ApiKeyRequest, RawApiKeyResponse};
use std::error::Error;

// TODO: Make the auth server's port configurable
const SERVER: &str = "http://localhost:8080";

pub async fn list_api_keys() -> Result<(), Box<dyn Error>> {
    Ok(())
}

pub async fn get_api_key(id: &str) -> Result<(), Box<dyn Error>> {
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
