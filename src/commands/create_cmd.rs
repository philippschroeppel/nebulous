use nebulous::config::GlobalConfig;
use nebulous::models::{
    RestartPolicy, V1ContainerRequest, V1ContainerResources, V1EnvVar, V1Meter,
    V1ResourceMetaRequest, V1VolumeConfig, V1VolumeDriver, V1VolumePath,
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
        let env = command.env.map(|env_vec| {
            env_vec
                .into_iter()
                .map(|(key, value)| V1EnvVar {
                    key,
                    value: Some(value),
                    ..Default::default()
                })
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
                request_json_path: None,
                response_json_path: None,
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
            args: None, // TODO
            accelerators: command.accelerators,
            platform: command.platform,
            env: env,
            volumes: Some(volumes.unwrap().paths),
            metadata: Some(V1ResourceMetaRequest {
                name: command.name,
                namespace: command.namespace,
                owner: None,
                owner_ref: None,
                labels: labels,
            }),
            meters: meters,
            restart: command.restart.unwrap_or(RestartPolicy::Always.to_string()),
            queue: command.queue,
            timeout: command.timeout,
            resources: Some(V1ContainerResources {
                min_cpu: command.min_cpu,
                min_memory: command.min_memory,
                max_cpu: command.max_cpu,
                max_memory: command.max_memory,
            }),
            ssh_keys: None,
            ports: None,
            public_ip: None,
        }
    };

    let client = Client::new();
    let config = GlobalConfig::read()?;
    let current_server = config.get_current_server_config().unwrap();
    let server = current_server.server.as_ref().unwrap();
    let api_key = current_server.api_key.as_ref().unwrap();

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

pub async fn create_secret(
    command: crate::cli::SecretCommands, // define your CLI struct accordingly
) -> Result<(), Box<dyn std::error::Error>> {
    println!("Creating secret...");

    // Build the metadata (reused whether file is provided or not)
    let metadata = nebulous::models::V1ResourceMetaRequest {
        name: Some(command.name.clone()),
        namespace: Some(command.namespace.clone().unwrap_or("default".to_string())),
        ..Default::default()
    };

    // Construct the request object depending on whether a file is provided
    let secret_request = if let Some(file) = command.file {
        // If the user provided a file, read *raw* contents as the secret value
        println!("Reading secret file: {}", file);
        let file_content = std::fs::read_to_string(&file)?;
        println!("File content read");

        nebulous::models::V1SecretRequest {
            metadata,
            value: file_content,
            expires_at: command.expires_at,
        }
    } else {
        // Otherwise, ensure a `--value` was provided on the CLI
        if command.value.is_none() {
            return Err("Missing required secret value (or a file)".into());
        }

        // Construct the request object directly from CLI arguments
        nebulous::models::V1SecretRequest {
            metadata,
            value: command.value.clone().unwrap(),
            expires_at: command.expires_at,
        }
    };

    // Issue the request to your server
    let client = reqwest::Client::new();
    let config = nebulous::config::GlobalConfig::read()?;
    let current_server = config.get_current_server_config().unwrap();
    let server = current_server.server.as_ref().unwrap();
    let api_key = current_server.api_key.as_ref().unwrap();

    // POST /v1/secrets with Authorization header
    let url = format!("{}/v1/secrets", server);
    let response = client
        .post(&url)
        .header("Authorization", format!("Bearer {}", api_key))
        .json(&secret_request)
        .send()
        .await?;

    // Check the server response
    if response.status().is_success() {
        let secret_response: serde_json::Value = response.json().await?;
        println!("Secret created successfully!");
        println!("ID: {}", secret_response["metadata"]["id"]);
        println!("Name: {}", secret_response["metadata"]["name"]);
        Ok(())
    } else {
        let error_text = response.text().await?;
        Err(format!("Failed to create secret: {}", error_text).into())
    }
}
