use nebulous::config::GlobalConfig;
use nebulous::models::{
    RestartPolicy, V1ContainerMetaRequest, V1ContainerRequest, V1ContainerResources, V1EnvVar,
    V1Meter, V1VolumeConfig, V1VolumeDriver, V1VolumePath,
};
use reqwest::Client;
use serde_json::Value;
use std::collections::HashMap;
use std::error::Error;
use std::str::FromStr;

pub async fn create_container(
    command: crate::cli::ContainerCommands,
) -> Result<(), Box<dyn Error>> {
    println!("Creating container");
    let container_request = if let Some(file) = command.file {
        println!("Reading file: {}", file);
        let file_content = std::fs::read_to_string(file)?;
        println!("File content read");
        let container_request: V1ContainerRequest = serde_yaml::from_str(&file_content)?;
        println!("Container request: {:?}", container_request);
        container_request
    } else {
        // Build volume configuration if source and destination are provided
        let volumes = if let (Some(source), Some(destination)) =
            (&command.volume_source, &command.volume_destination)
        {
            Some(V1VolumeConfig {
                paths: vec![V1VolumePath {
                    source: source.clone(),
                    dest: destination.clone(),
                    resync: command.volume_resync,
                    driver: V1VolumeDriver::from_str(&command.volume_type.unwrap())?,
                    continuous: command.volume_continuous,
                    ..Default::default()
                }],
                cache_dir: command.volume_cache_dir,
            })
        } else {
            None
        };

        // Convert Vec<(String, String)> to HashMap<String, String> for env vars
        let env_vars = command.env.map(|env_vec| {
            env_vec
                .into_iter()
                .map(|(key, value)| V1EnvVar { key, value })
                .collect::<Vec<V1EnvVar>>()
        });

        // Convert Vec<(String, String)> to HashMap<String, String> for labels
        let labels = command
            .label
            .map(|label_vec| label_vec.into_iter().collect::<HashMap<String, String>>());

        let meters = if command.meter_cost.is_some() || command.meter_cost_plus.is_some() {
            Some(vec![V1Meter {
                cost: command.meter_cost,
                costp: command.meter_cost_plus,
                unit: command.meter_unit.clone().unwrap_or_default(),
                metric: command.meter_metric.clone().unwrap_or_default(),
                currency: command.meter_currency.clone().unwrap_or_default(),
            }])
        } else {
            None
        };
        if command.image.is_none() {
            return Err("Image is required".into());
        }

        // Build ContainerRequest
        V1ContainerRequest {
            kind: "Container".to_string(),
            image: command.image.unwrap(),
            command: command.cmd,
            accelerators: command.accelerators,
            platform: command.platform,
            env_vars: env_vars,
            volumes: Some(volumes.unwrap().paths),
            metadata: Some(V1ContainerMetaRequest {
                name: command.name,
                namespace: command.namespace,
                owner_id: None,
                labels: labels,
            }),
            meters: meters,
            restart: command.restart.unwrap_or(RestartPolicy::Always.to_string()),
            queue: command.queue,
            resources: Some(V1ContainerResources {
                min_cpu: command.min_cpu,
                min_memory: command.min_memory,
                max_cpu: command.max_cpu,
                max_memory: command.max_memory,
            }),
            ssh_keys: None,
        }
    };

    let client = Client::new();
    let config = GlobalConfig::read()?;
    let server = config.server.unwrap();
    let api_key = config.api_key.ok_or("API key not found in configuration")?;

    let url = format!("{}/v1/containers", server);
    let response = client
        .post(&url)
        .header("Authorization", format!("Bearer {}", api_key))
        .json(&container_request)
        .send()
        .await?;

    if response.status().is_success() {
        let container: Value = response.json().await?;
        println!("Container created successfully!");
        println!("ID: {}", container["metadata"]["id"]);
        println!("Name: {}", container["metadata"]["name"]);
    } else {
        let error_text = response.text().await?;
        return Err(format!("Failed to create container: {}", error_text).into());
    }

    Ok(())
}
