use nebulous::config::GlobalConfig;
use std::error::Error as StdError;

use futures::{SinkExt, StreamExt};
use reqwest::Client;
use serde::Deserialize;
use tokio_tungstenite::{
    connect_async, tungstenite::http::Request, tungstenite::protocol::Message,
};

pub async fn fetch_container_logs(
    name: String,
    namespace: String,
    follow: bool,
) -> Result<String, Box<dyn StdError>> {
    // Load config
    let config = GlobalConfig::read()?;
    let current_server = config.get_current_server_config().unwrap();
    let server = current_server.server.as_ref().unwrap();
    let api_key = current_server.api_key.as_ref().unwrap();

    if follow {
        // Use WebSocket to stream logs
        let ws_url = server
            .replace("http://", "ws://")
            .replace("https://", "wss://");
        let ws_url = format!(
            "{}/v1/containers/{}/{}/logs/stream",
            ws_url, namespace, name
        );

        // Create a request with authorization header for the handshake
        let request = Request::builder()
            .uri(ws_url)
            .header("Authorization", format!("Bearer {}", api_key))
            // Add necessary WebSocket handshake headers (tungstenite builder handles Sec-WebSocket-Key, etc.)
            .header("Connection", "Upgrade")
            .header("Upgrade", "websocket")
            .header("Sec-WebSocket-Version", "13") // Standard version
            .body(())?;

        // Connect to the WebSocket endpoint with the request
        let (ws_stream, response) = connect_async(request).await?;

        // Check the response status code from the handshake
        if !response.status().is_success() {
            return Err(format!("WebSocket handshake failed: Status {}", response.status()).into());
        }
        println!("WebSocket connection established");

        // Split the stream
        let (mut write, mut read) = ws_stream.split();

        // Print incoming messages until connection is closed
        while let Some(message) = read.next().await {
            match message {
                Ok(Message::Text(text)) => {
                    println!("{}", text);
                }
                Ok(Message::Close(_)) => {
                    println!("Connection closed by server");
                    break;
                }
                Ok(Message::Ping(ping_data)) => {
                    // Respond to pings to keep the connection alive
                    write.send(Message::Pong(ping_data)).await?;
                }
                Err(e) => {
                    eprintln!("WebSocket error: {}", e);
                    return Err(e.into());
                }
                _ => {}
            }
        }

        Ok("Log streaming finished.".to_string())
    } else {
        // Call the REST API endpoint
        // Step 1: Fetch container ID by calling your server's HTTP GET /v1/containers/:namespace/:name
        let container_id = fetch_container_id_from_api(&namespace, &name).await?;

        let _bearer_token = format!("Bearer {}", api_key);

        // Step 2: Run the local SSH command using the ID as the SSH host (e.g. Tailscale address).
        //         This uses the synchronous `run_ssh_command_ts` from your existing code.
        let mut cmd = vec![
            "cat".to_string(),
            "$HOME/.logs/nebu_container.log".to_string(),
        ];

        let output = nebulous::ssh::exec::run_ssh_command_ts(
            &format!("container-{}", container_id),
            cmd,
            false,
            false,
            Some("root"), // TODO: need to fetch from the API
        )?;

        // Output the SSH command's stdout
        println!("{}", output);
        Ok(output)
    }
}

/// Helper function: calls GET /v1/containers/<namespace>/<name>
/// and returns the container's `.metadata.id`.
async fn fetch_container_id_from_api(
    namespace: &str,
    name: &str,
) -> Result<String, Box<dyn StdError>> {
    let config = nebulous::config::GlobalConfig::read()?;
    let current_server = config.get_current_server_config().unwrap();
    let server = current_server.server.as_ref().unwrap();
    let api_key = current_server.api_key.as_ref().unwrap();
    // Adjust base URL/host as needed:
    let url = format!("{}/v1/containers/{}/{}", server, namespace, name);

    // Use reqwest to fetch container JSON
    let client = Client::new();
    let response = client
        .get(&url)
        .header("Authorization", format!("Bearer {}", api_key))
        .send()
        .await?;

    let container = response
        .error_for_status()? // Return Err if e.g. 404 or 500
        .json::<V1Container>()
        .await?;

    Ok(container.metadata.id)
}

/// Minimal struct matching the server's "Container" JSON shape.
/// We only need the `metadata.id` field for this flow.
#[derive(Deserialize)]
struct V1Container {
    metadata: V1ResourceMeta,
}

/// Minimal struct for container's metadata (includes ID).
#[derive(Deserialize)]
struct V1ResourceMeta {
    pub id: String,
}
