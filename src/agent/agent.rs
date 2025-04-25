use crate::models::{V1AgentKey, V1CreateAgentKeyRequest};
use anyhow::{anyhow, Result};
use reqwest::Client;
use tracing::{debug, info};

/// Creates a new agent key by calling the `/v1/agent/keys` endpoint
///
/// # Arguments
///
/// * `client` - The HTTP client to use for the request
/// * `base_url` - The base URL of the API
/// * `api_key` - The API key to use for authentication
/// * `request` - The request parameters for creating the agent key
///
/// # Returns
///
/// Returns the created agent key on success, or an error if the request fails
pub async fn create_agent_key(
    base_url: &str,
    api_key: &str,
    request: V1CreateAgentKeyRequest,
) -> Result<V1AgentKey> {
    let client = Client::new();
    // Change return type to anyhow's Result
    let url = format!("{}/v1/agent/keys", base_url);

    debug!(
        "Creating agent key with request: {:?} and key: {}",
        request, api_key
    );
    let response = client
        .post(&url)
        .header("Authorization", format!("Bearer {}", api_key))
        .json(&request)
        .send()
        .await?;

    if !response.status().is_success() {
        let status = response.status();
        let error_text = response.text().await?;
        return Err(anyhow!("API error {}: {}", status.as_u16(), error_text));
    }

    let agent_key: V1AgentKey = response.json().await?;
    info!("Agent key created: {:?}", agent_key);
    Ok(agent_key)
}
