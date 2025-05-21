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
    info!(">>> Agent key created: {:?}", agent_key);

    // GET /v1/users/me with the new key
    let key_ref = agent_key
        .key
        .as_deref()
        .ok_or(anyhow!("Missing key in agent_key"))?;
    let user_me_url = format!("{}/v1/users/me", base_url);
    info!("Fetching /v1/users/me with new key...");
    let user_me_response = client
        .get(&user_me_url)
        .header("Authorization", format!("Bearer {}", key_ref))
        .send()
        .await?;

    let response_status = user_me_response.status();
    let response_text = user_me_response.text().await?;

    info!(
        "/v1/users/me response status: {}, body: {}",
        response_status, response_text
    );

    Ok(agent_key)
}
