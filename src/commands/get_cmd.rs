use chrono::{DateTime, Utc};

use nebulous::config::GlobalConfig;
use serde::Serialize;
use serde_json::Value;
use std::error::Error;

pub async fn get_containers(id: Option<String>) -> Result<(), Box<dyn Error>> {
    let config = GlobalConfig::read()?;
    let server = config.server.unwrap();
    let api_key = config.api_key.unwrap_or_default();

    let bearer_token = format!("Bearer {}", api_key);

    let url = format!("{}/v1/containers", server);

    // Create HTTP client
    let client = reqwest::Client::new();

    // Add name filter if provided
    if let Some(container_id) = &id {
        let url = format!("{}/v1/containers/{}", server, container_id);

        // Build the request
        let request = client.get(&url).header("Authorization", &bearer_token);

        // Execute the request
        let response = request.send().await?;

        // Check if the request was successful
        if !response.status().is_success() {
            return Err(format!("Failed to get containers: {}", response.status()).into());
        }
        // Parse the response
        let mut containers: Value = response.json().await?;

        // Remove null values for cleaner output
        remove_null_values(&mut containers);

        // Alternative approach using lower-level API
        let mut buf = Vec::new();
        {
            let mut serializer = serde_yaml::Serializer::new(&mut buf);
            containers.serialize(&mut serializer)?;
        }
        let yaml = String::from_utf8(buf)?;

        println!("{}", yaml);
        return Ok(());
    }

    // Build the request
    let request = client.get(&url).header("Authorization", &bearer_token);

    // Execute the request
    let response = request.send().await?;

    // Check if the request was successful
    if !response.status().is_success() {
        return Err(format!("Failed to get containers: {}", response.status()).into());
    }

    // Parse the response
    let mut containers: Value = response.json().await?;

    // Remove null values for cleaner output
    remove_null_values(&mut containers);

    // Create a table to display the containers
    let mut table = prettytable::Table::new();

    // Add table headers
    table.add_row(prettytable::Row::new(vec![
        prettytable::Cell::new("ID"),
        prettytable::Cell::new("NAME"),
        prettytable::Cell::new("STATUS"),
        prettytable::Cell::new("CREATED"),
    ]));

    let empty_vec = Vec::new();
    let container_list = containers
        .get("containers")
        .and_then(Value::as_array)
        .unwrap_or(&empty_vec);

    // Process containers data
    for container in container_list {
        if let Value::Object(container_obj) = container {
            // Extract container details with defaults for missing values
            let id = container_obj
                .get("metadata")
                .and_then(Value::as_object)
                .and_then(|metadata| metadata.get("id"))
                .and_then(Value::as_str)
                .unwrap_or("N/A");
            let name = container_obj
                .get("metadata")
                .and_then(Value::as_object)
                .and_then(|metadata| metadata.get("name"))
                .and_then(Value::as_str)
                .unwrap_or("N/A");
            let status = container_obj
                .get("status")
                .and_then(Value::as_str)
                .unwrap_or("N/A");

            // Format creation time if available
            let created = container_obj
                .get("metadata")
                .and_then(Value::as_object)
                .and_then(|metadata| metadata.get("created_at"))
                .and_then(|v| v.as_i64().or_else(|| v.as_u64().map(|n| n as i64)))
                .map(|timestamp| {
                    let dt = DateTime::<Utc>::from_timestamp(timestamp, 0).unwrap_or_default();
                    dt.format("%Y-%m-%d %H:%M:%S").to_string()
                })
                .unwrap_or_else(|| "N/A".to_string());

            // Add row to table
            table.add_row(prettytable::Row::new(vec![
                prettytable::Cell::new(id),
                prettytable::Cell::new(name),
                prettytable::Cell::new(status),
                prettytable::Cell::new(&created),
            ]));
        }
    }

    // Set table format and print
    table.set_format(*prettytable::format::consts::FORMAT_CLEAN);
    table.printstd();

    Ok(())
}

pub async fn get_accelerators(platform: Option<String>) -> Result<(), Box<dyn Error>> {
    use nebulous::accelerator::aws::AwsProvider;
    use nebulous::accelerator::base::{AcceleratorProvider, Config};
    use nebulous::accelerator::runpod::RunPodProvider;
    use prettytable::{format, Cell, Row, Table};

    // Load the default accelerator configuration
    let config = Config::default();

    // Create a table to display the accelerators
    let mut table = Table::new();

    // Add table headers
    let mut headers = vec![Cell::new("NAME"), Cell::new("MEMORY (GB)")];

    // If a platform is specified, add a column for platform-specific names
    if platform.is_some() {
        headers.push(Cell::new("PLATFORM NAME"));
    }

    table.add_row(Row::new(headers));

    // Get the appropriate provider based on the platform parameter
    let provider: Option<Box<dyn AcceleratorProvider>> = match platform.as_deref() {
        Some("aws") => Some(Box::new(AwsProvider::new())),
        Some("runpod") => Some(Box::new(RunPodProvider::new())),
        Some(unknown) => {
            eprintln!(
                "Unknown platform: {}. Supported platforms are 'aws' and 'runpod'.",
                unknown
            );
            return Ok(());
        }
        None => None,
    };

    // Add rows for each accelerator
    for acc in &config.accelerators.supported {
        let mut row = vec![Cell::new(&acc.name), Cell::new(&acc.memory.to_string())];

        // If a platform is specified, add the platform-specific name
        if let Some(ref provider) = provider {
            let platform_name = provider
                .get_platform_name(&acc.name)
                .map(|s| s.as_str())
                .unwrap_or("N/A");
            row.push(Cell::new(platform_name));
        }

        table.add_row(Row::new(row));
    }

    table.set_format(*format::consts::FORMAT_CLEAN);
    table.printstd();

    Ok(())
}

// Function to recursively remove null values from serde_json::Value
fn remove_null_values(value: &mut Value) {
    match value {
        Value::Object(map) => {
            // Collect keys with null values
            let keys_with_nulls: Vec<_> = map
                .iter()
                .filter_map(|(k, v)| if v.is_null() { Some(k.clone()) } else { None })
                .collect();

            // Remove keys with null values
            for k in keys_with_nulls {
                map.remove(&k);
            }

            // Recursively process the remaining values
            for v in map.values_mut() {
                remove_null_values(v);
            }
        }
        Value::Array(arr) => {
            // Recursively process each item in the array
            for v in arr.iter_mut() {
                remove_null_values(v);
            }
        }
        _ => {}
    }
}

pub async fn get_platforms() -> Result<(), Box<dyn Error>> {
    let platforms = vec!["gce", "runpod", "ec2"];
    println!("{:?}", platforms);
    Ok(())
}
