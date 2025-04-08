use nebulous::config::GlobalConfig;
use serde_json::Value;
use std::error::Error;
use std::fs;
use std::io::{self, Read};
use tracing::debug;

pub async fn send_messages(args: &crate::cli::SendMessageCommands) -> Result<(), Box<dyn Error>> {
    let config = GlobalConfig::read()?;
    debug!("Config: {:?}", config);
    let current_server = config.get_current_server_config().unwrap(); // Handle error more gracefully
    let server = current_server.server.as_ref().unwrap(); // Handle error
    let api_key = current_server.api_key.as_ref().unwrap(); // Handle error

    // Get required namespace
    let namespace = args
        .namespace
        .as_ref()
        .ok_or("Namespace must be provided with --namespace")?;
    let stream_name = &args.name;

    let bearer_token = format!("Bearer {}", api_key);

    // Construct stream message URL
    let mut url = format!(
        "{}/v1/processors/{}/{}/messages",
        server, namespace, stream_name
    );

    // Add wait query parameter if specified
    if args.wait {
        url.push_str("?wait=true");
    }

    // Read message content (from file or stdin)
    let content_str = if let Some(file_path) = &args.file {
        debug!("Reading message content from file: {}", file_path);
        fs::read_to_string(file_path)?
    } else {
        debug!("Reading message content from stdin");
        let mut stdin_content = String::new();
        io::stdin().read_to_string(&mut stdin_content)?;
        stdin_content
    };

    // Deserialize message content into a generic JSON Value
    // We assume the server will wrap this content into a V1StreamMessage
    let message_content: Value = serde_yaml::from_str(&content_str).map_err(|e| {
        format!(
            "Failed to parse YAML/JSON input into message content: {}",
            e
        )
    })?;

    // --- API Call ---
    let client = reqwest::Client::new();
    debug!(
        "Sending message to URL: {} with content: {:?}",
        url, message_content
    );
    let response = client
        .post(&url)
        .header("Authorization", &bearer_token)
        .header("Content-Type", "application/json")
        .json(&message_content) // Send the message content
        .send()
        .await?;

    // --- Response Handling ---
    if response.status().is_success() {
        println!(
            "Message sent successfully to stream '{}' in namespace '{}'.",
            stream_name, namespace
        );
        let response_body: Value = response.json().await?;
        println!("{:?}", response_body);
    } else {
        let status = response.status();
        let error_body = response.text().await?;
        eprintln!("Failed to send message: {} - {}", status, error_body);
        return Err(format!("API request failed with status {}", status).into());
    }

    Ok(())
}
