use nebulous::config::GlobalConfig;
use std::error::Error as StdError;

pub async fn fetch_container_logs(
    namespace: String,
    name: String,
) -> Result<String, Box<dyn StdError>> {
    // Load config the same way as in get_cmd.rs
    let config = GlobalConfig::read()?;
    let server = config.server.unwrap();
    let api_key = config.api_key.unwrap_or_default();

    let bearer_token = format!("Bearer {}", api_key);

    // Construct the URL using the configured server
    let url = format!("{}/v1/containers/{}/{}/logs", server, namespace, name);

    // Build and send the request with the authorization header
    let client = reqwest::Client::new();
    let request = client.get(&url).header("Authorization", &bearer_token);

    let response = request.send().await?;

    if !response.status().is_success() {
        return Err(format!("Failed to get container logs: {}", response.status()).into());
    }

    // Return the logs text
    Ok(response.text().await?)
}
