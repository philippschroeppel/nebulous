use nebulous::config::GlobalConfig;
use nebulous::models::{ContainerRequest, VolumeConfig, VolumePath};
use reqwest::Client;
use serde_json::Value;
use std::collections::HashMap;
use std::error::Error;

pub async fn create_container(
    command: crate::cli::ContainerCommands,
) -> Result<(), Box<dyn Error>> {
    // Build volume configuration if source and destination are provided
    let volumes = if let (Some(source), Some(destination)) =
        (&command.volume_source, &command.volume_destination)
    {
        Some(VolumeConfig {
            paths: vec![VolumePath {
                source_path: source.clone(),
                destination_path: destination.clone(),
                resync: command.volume_resync,
                bidirectional: command.volume_bidirectional,
                continuous: command.volume_continuous,
            }],
            cache_dir: command.volume_cache_dir,
        })
    } else {
        None
    };

    // Convert Vec<(String, String)> to HashMap<String, String> for env vars
    let env_vars = command
        .env
        .map(|env_vec| env_vec.into_iter().collect::<HashMap<String, String>>());

    // Convert Vec<(String, String)> to HashMap<String, String> for labels
    let labels = command
        .label
        .map(|label_vec| label_vec.into_iter().collect::<HashMap<String, String>>());

    // Build ContainerRequest
    let container_request = ContainerRequest {
        name: command.name,
        image: command.image,
        command: command.command,
        accelerators: command.accelerators,
        platform: command.platform,
        namespace: command.namespace,
        env_vars,
        volumes,
        labels,
    };

    let client = Client::new();
    let config = GlobalConfig::read()?;
    let server = config.server.unwrap();

    let url = format!("{}/v1/containers", server);
    let response = client.post(&url).json(&container_request).send().await?;

    if response.status().is_success() {
        let container: Value = response.json().await?;
        println!("Container created successfully:");
        println!("ID: {}", container["metadata"]["id"]);
        println!("Name: {}", container["name"]);
    } else {
        let error_text = response.text().await?;
        return Err(format!("Failed to create container: {}", error_text).into());
    }

    Ok(())
}
