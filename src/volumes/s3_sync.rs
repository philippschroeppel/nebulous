use serde::{Deserialize, Serialize};
use std::error::Error;
use std::fs;
use std::path::Path;
use std::process::Command;

#[derive(Debug, Serialize, Deserialize)]
pub struct SyncConfig {
    pub paths: Vec<SyncPath>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SyncPath {
    pub source: String,
    pub dest: String,
}

#[derive(Debug, Clone, Copy)]
pub enum SyncDirection {
    UploadToS3,     // Local to S3
    DownloadFromS3, // S3 to Local
}

impl SyncPath {
    /// Determine the sync direction based on the source and destination paths
    pub fn get_direction(&self) -> SyncDirection {
        if self.source.starts_with("s3://") {
            SyncDirection::DownloadFromS3
        } else {
            SyncDirection::UploadToS3
        }
    }
}

impl SyncConfig {
    /// Read a sync configuration from a file path
    pub fn read_from_file(path: &str) -> Result<Self, Box<dyn Error>> {
        // Check if the config file exists
        if !Path::new(path).exists() {
            return Err(format!("Config file not found: {}", path).into());
        }

        // Read the YAML file
        let yaml_content = fs::read_to_string(path)?;

        // Parse the YAML content
        let config: SyncConfig = serde_yaml::from_str(&yaml_content)
            .map_err(|e| format!("Failed to parse YAML: {}", e))?;

        Ok(config)
    }

    /// Write the sync configuration to a file path
    pub fn write_to_file(&self, path: &str) -> Result<(), Box<dyn Error>> {
        // Convert the config to YAML
        let yaml_content = serde_yaml::to_string(self)
            .map_err(|e| format!("Failed to serialize to YAML: {}", e))?;

        // Create parent directories if they don't exist
        if let Some(parent) = Path::new(path).parent() {
            fs::create_dir_all(parent)?;
        }

        // Write the YAML content to the file
        fs::write(path, yaml_content)?;

        Ok(())
    }

    /// Create a new empty sync configuration
    pub fn new() -> Self {
        SyncConfig { paths: Vec::new() }
    }

    /// Add a new path to the sync configuration
    pub fn add_path(&mut self, source: String, dest: String) {
        self.paths.push(SyncPath { source, dest });
    }

    /// Remove a path from the sync configuration by index
    pub fn remove_path(&mut self, index: usize) -> Result<(), Box<dyn Error>> {
        if index >= self.paths.len() {
            return Err(format!(
                "Index {} out of bounds (max: {})",
                index,
                self.paths.len() - 1
            )
            .into());
        }

        self.paths.remove(index);
        Ok(())
    }

    /// List all paths in the sync configuration
    pub fn list_paths(&self) -> Vec<(&String, &String, SyncDirection)> {
        self.paths
            .iter()
            .map(|path| (&path.source, &path.dest, path.get_direction()))
            .collect()
    }
}

/// Continuously sync paths from the configuration file at regular intervals
pub async fn execute_continuous_sync(
    config_path: String,
    interval_seconds: u64,
    create_if_missing: bool,
) -> Result<(), Box<dyn Error>> {
    println!(
        "Starting continuous sync from configuration: {}",
        config_path
    );
    println!("Sync interval: {} seconds", interval_seconds);

    loop {
        match execute_sync(config_path.clone(), create_if_missing).await {
            Ok(_) => println!("Sync completed successfully. Waiting for next sync..."),
            Err(e) => println!("Sync error: {}. Will retry on next interval.", e),
        }

        // Sleep for the specified interval
        tokio::time::sleep(tokio::time::Duration::from_secs(interval_seconds)).await;
    }
}

/// Update the execute_sync function to use the new utility methods
pub async fn execute_sync(
    config_path: String,
    create_if_missing: bool,
) -> Result<(), Box<dyn Error>> {
    println!("Reading sync configuration from: {}", config_path);

    if !Path::new(&config_path).exists() {
        if create_if_missing {
            create_empty_config(&config_path)?;
        } else {
            return Err(format!("Config file not found: {}", config_path).into());
        }
    }

    // Use the new read_from_file method
    let config = SyncConfig::read_from_file(&config_path)?;

    if config.paths.is_empty() {
        println!("No paths to sync found in the configuration file.");
        return Ok(());
    }

    println!("Found {} paths to sync", config.paths.len());

    // Process each path
    for (index, path) in config.paths.iter().enumerate() {
        let direction = path.get_direction();
        let direction_str = match direction {
            SyncDirection::UploadToS3 => "Upload to S3",
            SyncDirection::DownloadFromS3 => "Download from S3",
        };

        println!(
            "[{}/{}] Syncing {} to {} ({})",
            index + 1,
            config.paths.len(),
            path.source,
            path.dest,
            direction_str
        );

        // Verify source path exists
        let source_exists = if path.source.starts_with("s3://") {
            // For S3 paths, we can't easily check existence here
            // Could add an aws s3 ls check, but for now we'll assume it exists
            true
        } else {
            Path::new(&path.source).exists()
        };

        if !source_exists {
            println!("Warning: Source path does not exist: {}", path.source);
            continue;
        }

        // Execute the AWS S3 sync command
        // This will recursively sync all files and subdirectories
        let output = Command::new("aws")
            .arg("s3")
            .arg("sync")
            .arg(&path.source)
            .arg(&path.dest)
            .output()?;

        if output.status.success() {
            println!("Successfully synced {} to {}", path.source, path.dest);
        } else {
            let error = String::from_utf8_lossy(&output.stderr);
            println!("Failed to sync: {}", error);
        }
    }

    println!("Sync operation completed");
    Ok(())
}

/// Create a new empty sync configuration file
pub fn create_empty_config(path: &str) -> Result<(), Box<dyn Error>> {
    let config = SyncConfig::new();

    // Write the empty config to the file
    config.write_to_file(path)?;

    println!("Created empty sync configuration at: {}", path);
    Ok(())
}

/// Create a new sync configuration file with example paths
pub fn create_example_config(path: &str) -> Result<(), Box<dyn Error>> {
    let mut config = SyncConfig::new();

    // Add some example paths
    config.add_path(
        "/path/to/local/directory1".to_string(),
        "s3://your-bucket/directory1".to_string(),
    );
    config.add_path(
        "s3://your-bucket/directory2".to_string(),
        "/path/to/local/directory2".to_string(),
    );
    config.add_path(
        "/path/to/local/file.txt".to_string(),
        "s3://your-bucket/file.txt".to_string(),
    );

    // Write the config to the file
    config.write_to_file(path)?;

    println!("Created example sync configuration at: {}", path);
    Ok(())
}

/// Add a new path to an existing sync configuration
pub fn add_sync_path(
    config_path: &str,
    source: String,
    dest: String,
) -> Result<(), Box<dyn Error>> {
    // Read the existing config or create a new one if it doesn't exist
    let mut config = if Path::new(config_path).exists() {
        SyncConfig::read_from_file(config_path)?
    } else {
        SyncConfig::new()
    };

    // Determine direction for display purposes
    let direction_str = if source.starts_with("s3://") {
        "Download from S3"
    } else {
        "Upload to S3"
    };

    // Add the new path
    config.add_path(source.clone(), dest.clone());

    // Write the updated config back to the file
    config.write_to_file(config_path)?;

    println!(
        "Added sync path: {} -> {} ({})",
        source, dest, direction_str
    );
    Ok(())
}

/// Remove a path from an existing sync configuration by index
pub fn remove_sync_path(config_path: &str, index: usize) -> Result<(), Box<dyn Error>> {
    // Read the existing config
    let mut config = SyncConfig::read_from_file(config_path)?;

    // Get the path that will be removed (for the message)
    let path_to_remove = if index < config.paths.len() {
        // Clone the strings to avoid the borrow conflict
        Some((
            config.paths[index].source.clone(),
            config.paths[index].dest.clone(),
        ))
    } else {
        None
    };

    // Remove the path
    config.remove_path(index)?;

    // Write the updated config back to the file
    config.write_to_file(config_path)?;

    if let Some((source, destination)) = path_to_remove {
        println!("Removed sync path: {} -> {}", source, destination);
    }

    Ok(())
}

/// List all paths in a sync configuration
pub fn list_sync_paths(config_path: &str) -> Result<(), Box<dyn Error>> {
    // Read the existing config
    let config = SyncConfig::read_from_file(config_path)?;

    if config.paths.is_empty() {
        println!("No sync paths found in the configuration.");
        return Ok(());
    }

    println!("Sync paths in {}:", config_path);
    for (i, (source, destination, direction)) in config.list_paths().iter().enumerate() {
        let direction_str = match direction {
            SyncDirection::UploadToS3 => "Upload to S3",
            SyncDirection::DownloadFromS3 => "Download from S3",
        };

        println!("[{}] {} -> {} ({})", i, source, destination, direction_str);
    }

    Ok(())
}
