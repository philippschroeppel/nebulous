use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::error::Error;
use std::fs;
use std::path::Path;
use std::process::Command;
use tokio::process::Command as TokioCommand;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct VolumeConfig {
    pub paths: Vec<VolumePath>,
    #[serde(default = "default_cache_dir")]
    pub cache_dir: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct VolumePath {
    pub source_path: String,
    pub destination_path: String,
    #[serde(default)]
    pub resync: bool,
    #[serde(default = "default_bidirectional")]
    pub bidirectional: bool,
    #[serde(default = "default_continuous")]
    pub continuous: bool,
}

// Add default functions for new fields
fn default_bidirectional() -> bool {
    true
}

fn default_continuous() -> bool {
    true
}

// Add this function to provide a default cache directory
fn default_cache_dir() -> String {
    // Use a sensible default location for the cache
    format!("/cache/rclone")
}

impl VolumeConfig {
    /// Read a sync configuration from a file path
    pub fn read_from_file(path: &str) -> Result<Self, Box<dyn Error>> {
        // Check if the config file exists
        if !Path::new(path).exists() {
            return Err(format!("Config file not found: {}", path).into());
        }

        // Read the YAML file
        let yaml_content = fs::read_to_string(path)?;

        // Parse the YAML content
        let config: VolumeConfig = serde_yaml::from_str(&yaml_content)
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
        VolumeConfig {
            paths: Vec::new(),
            cache_dir: default_cache_dir(),
        }
    }

    /// Add a new path to the sync configuration
    pub fn add_path(
        &mut self,
        source_path: String,
        destination_path: String,
        resync: bool,
        bidirectional: bool,
        continuous: bool,
    ) {
        self.paths.push(VolumePath {
            source_path,
            destination_path,
            resync,
            bidirectional,
            continuous,
        });
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
    pub fn list_paths(&self) -> Vec<(&String, &String, bool, bool, bool)> {
        self.paths
            .iter()
            .map(|path| {
                (
                    &path.source_path,
                    &path.destination_path,
                    path.resync,
                    path.bidirectional,
                    path.continuous,
                )
            })
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

    // Map to track running processes: (source_path, destination_path) -> process
    let mut running_processes: HashMap<(String, String), tokio::process::Child> = HashMap::new();

    loop {
        // Read the current configuration
        let current_config = match VolumeConfig::read_from_file(&config_path) {
            Ok(config) => config,
            Err(e) => {
                if !Path::new(&config_path).exists() && create_if_missing {
                    match create_empty_config(&config_path) {
                        Ok(_) => VolumeConfig::new(),
                        Err(e) => {
                            println!(
                                "Failed to create config file: {}. Will retry on next interval.",
                                e
                            );
                            tokio::time::sleep(tokio::time::Duration::from_secs(interval_seconds))
                                .await;
                            continue;
                        }
                    }
                } else {
                    println!("Error reading config: {}. Will retry on next interval.", e);
                    tokio::time::sleep(tokio::time::Duration::from_secs(interval_seconds)).await;
                    continue;
                }
            }
        };

        // Check for removed paths and stop their processes
        let mut paths_to_remove = Vec::new();
        for (path_key, process) in &mut running_processes {
            let still_exists = current_config.paths.iter().any(|path| {
                (&path.source_path, &path.destination_path) == (&path_key.0, &path_key.1)
            });

            if !still_exists {
                println!(
                    "Path removed from config: {} ⟷ {}, stopping sync process",
                    path_key.0, path_key.1
                );
                // Attempt to kill the process
                let _ = process.kill();
                paths_to_remove.push(path_key.clone());
            }
        }

        // Remove stopped processes from our map
        for path_key in paths_to_remove {
            running_processes.remove(&path_key);
        }

        // Process each path in the current configuration
        for path in &current_config.paths {
            // Skip paths that are not marked for continuous sync
            if !path.continuous {
                continue;
            }

            let path_key = (path.source_path.clone(), path.destination_path.clone());

            // Check if this path needs a new process (new path or resync requested)
            let needs_new_process = path.resync || !running_processes.contains_key(&path_key);

            if needs_new_process {
                // If there's an existing process, stop it first
                if running_processes.contains_key(&path_key) {
                    if let Some(mut process) = running_processes.remove(&path_key) {
                        println!(
                            "Stopping existing sync process for {} ⟷ {}",
                            path_key.0, path_key.1
                        );
                        let _ = process.kill();
                    }
                }

                // Start a new process for this path
                match start_sync_process(path, &current_config.cache_dir).await {
                    Ok(process) => {
                        println!(
                            "Started sync process for {} ⟷ {}",
                            path.source_path, path.destination_path
                        );
                        running_processes.insert(path_key, process);

                        // If this was a resync operation, mark it as completed
                        if path.resync {
                            // Update the config to set resync to false
                            let mut updated_config = current_config.clone();
                            for p in &mut updated_config.paths {
                                if p.source_path == path.source_path
                                    && p.destination_path == path.destination_path
                                {
                                    p.resync = false;
                                }
                            }
                            // Write the updated config back to file
                            if let Err(e) = updated_config.write_to_file(&config_path) {
                                println!("Failed to update config after resync: {}", e);
                            }
                        }
                    }
                    Err(e) => {
                        println!(
                            "Failed to start sync process for {} ⟷ {}: {}",
                            path.source_path, path.destination_path, e
                        );
                    }
                }
            }
        }

        // Report status
        println!(
            "Currently managing {} sync processes. Waiting for next check...",
            running_processes.len()
        );

        // Sleep for the specified interval
        tokio::time::sleep(tokio::time::Duration::from_secs(interval_seconds)).await;
    }
}

/// Start a new rclone sync process for a path
async fn start_sync_process(
    path: &VolumePath,
    cache_dir: &str,
) -> Result<tokio::process::Child, Box<dyn Error>> {
    // Build the rclone command
    let mut cmd = TokioCommand::new("rclone");

    if path.bidirectional {
        // Use bisync for bidirectional sync
        cmd.arg("bisync");
        cmd.arg(&path.source_path);
        cmd.arg(&path.destination_path);
    } else {
        // Use sync for unidirectional sync
        cmd.arg("sync");
        cmd.arg(&path.source_path);
        cmd.arg(&path.destination_path);
    }

    // Add resync flag if needed and it's a bidirectional sync
    if path.resync && path.bidirectional {
        cmd.arg("--resync");
    }

    // Add common options
    cmd.arg("--verbose");
    cmd.arg("--fast-list");

    // Add cache directory
    cmd.arg("--cache-dir");
    cmd.arg(cache_dir);

    // Add resilient mode for bidirectional sync
    if path.bidirectional {
        cmd.arg("--resilient");
    }

    // Spawn the process
    let child = cmd.spawn()?;

    Ok(child)
}

/// Execute rclone bisync for all paths in the configuration
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

    // Use the read_from_file method
    let config = VolumeConfig::read_from_file(&config_path)?;

    if config.paths.is_empty() {
        println!("No paths to sync found in the configuration file.");
        return Ok(());
    }

    println!("Found {} paths to sync", config.paths.len());

    // Process each path
    for (index, path) in config.paths.iter().enumerate() {
        let sync_type = if path.bidirectional {
            "Bidirectional"
        } else {
            "Unidirectional"
        };

        println!(
            "[{}/{}] {} syncing between {} and {}",
            index + 1,
            config.paths.len(),
            sync_type,
            path.source_path,
            path.destination_path
        );

        // Verify source path exists if it's a local path
        if !path.source_path.starts_with("s3:") && !Path::new(&path.source_path).exists() {
            println!("Warning: Source path does not exist: {}", path.source_path);
            continue;
        }

        // Build the rclone command
        let mut cmd = Command::new("rclone");

        if path.bidirectional {
            cmd.arg("bisync");
            cmd.arg(&path.source_path);
            cmd.arg(&path.destination_path);

            // Add --resync flag if needed
            if path.resync {
                cmd.arg("--resync");
            }
        } else {
            cmd.arg("sync");
            cmd.arg(&path.source_path);
            cmd.arg(&path.destination_path);
        }

        // Add common options
        cmd.arg("--verbose");
        cmd.arg("--fast-list");

        // Add cache directory
        cmd.arg("--cache-dir");
        cmd.arg(&config.cache_dir);

        // Execute the command
        let output = cmd.output()?;
        if output.status.success() {
            println!(
                "Successfully synced between {} and {}",
                path.source_path, path.destination_path
            );

            // If this was a resync operation, mark it as completed
            if path.resync && path.bidirectional {
                // Read the config again to get the latest version
                let mut updated_config = VolumeConfig::read_from_file(&config_path)?;
                if index < updated_config.paths.len() {
                    updated_config.paths[index].resync = false;
                    updated_config.write_to_file(&config_path)?;
                }
            }
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
    let config = VolumeConfig::new();

    // Write the empty config to the file
    config.write_to_file(path)?;

    println!("Created empty sync configuration at: {}", path);
    Ok(())
}

/// Create a new example sync configuration file with various sync types
pub fn create_example_config(path: &str) -> Result<(), Box<dyn Error>> {
    let mut config = VolumeConfig::new();

    // Add example for bidirectional continuous sync
    config.add_path(
        "/path/to/local/directory1".to_string(),
        "s3:your-bucket/directory1".to_string(),
        true, // Initial sync should use resync
        true, // bidirectional
        true, // continuous
    );

    // Add example for unidirectional one-time sync
    config.add_path(
        "/path/to/local/directory2".to_string(),
        "s3:your-bucket/directory2".to_string(),
        false, // No resync needed for unidirectional
        false, // unidirectional
        false, // one-time
    );

    // Write the config to the file
    config.write_to_file(path)?;

    println!("Created example sync configuration at: {}", path);
    Ok(())
}

/// Add a new path to an existing sync configuration
pub fn add_sync_path(
    config_path: &str,
    source_path: String,
    destination_path: String,
    bidirectional: bool,
    continuous: bool,
) -> Result<(), Box<dyn Error>> {
    // Read the existing config or create a new one if it doesn't exist
    let mut config = if Path::new(config_path).exists() {
        VolumeConfig::read_from_file(config_path)?
    } else {
        VolumeConfig::new()
    };

    // Add the new path with resync set to true for initial bidirectional sync
    let resync = bidirectional; // Only set resync true if bidirectional
    config.add_path(
        source_path.clone(),
        destination_path.clone(),
        resync,
        bidirectional,
        continuous,
    );

    // Write the updated config back to the file
    config.write_to_file(config_path)?;

    let sync_type = if bidirectional {
        "bidirectional"
    } else {
        "unidirectional"
    };
    let sync_mode = if continuous { "continuous" } else { "once" };

    println!(
        "Added {} {} sync path between {} and {}",
        sync_type, sync_mode, source_path, destination_path
    );
    Ok(())
}

/// Remove a path from an existing sync configuration by index
pub fn remove_sync_path(config_path: &str, index: usize) -> Result<(), Box<dyn Error>> {
    // Read the existing config
    let mut config = VolumeConfig::read_from_file(config_path)?;

    // Get the path that will be removed (for the message)
    let path_to_remove = if index < config.paths.len() {
        // Clone the strings to avoid the borrow conflict
        Some((
            config.paths[index].source_path.clone(),
            config.paths[index].destination_path.clone(),
        ))
    } else {
        None
    };

    // Remove the path
    config.remove_path(index)?;

    // Write the updated config back to the file
    config.write_to_file(config_path)?;

    if let Some((source, destination)) = path_to_remove {
        println!("Removed sync path between {} and {}", source, destination);
    }

    Ok(())
}

/// List all paths in a sync configuration
pub fn list_sync_paths(config_path: &str) -> Result<(), Box<dyn Error>> {
    // Read the existing config
    let config = VolumeConfig::read_from_file(config_path)?;

    if config.paths.is_empty() {
        println!("No sync paths found in the configuration.");
        return Ok(());
    }

    println!("Sync paths in {}:", config_path);
    for (i, (source, destination, resync, bidirectional, continuous)) in
        config.list_paths().iter().enumerate()
    {
        let direction = if *bidirectional {
            "bidirectional"
        } else {
            "unidirectional"
        };
        let mode = if *continuous { "continuous" } else { "once" };
        let resync_status = if *resync && *bidirectional {
            " (resync pending)"
        } else {
            ""
        };

        println!(
            "[{}] {} ⟷ {} ({}, {}){}",
            i, source, destination, direction, mode, resync_status
        );
    }

    Ok(())
}
