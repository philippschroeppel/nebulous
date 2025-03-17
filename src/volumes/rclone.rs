use crate::models::{V1ContainerStatus, V1VolumeDriver};
use crate::query::Query;
use sea_orm::{DatabaseConnection, DbErr};
use serde::{Deserialize, Serialize};
use serde_json::from_str;
use std::collections::HashMap;
use std::error::Error;
use std::fs;
use std::path::Path;
use std::process::Command;
use std::process::Stdio;
use tokio::io::AsyncBufReadExt;
use tokio::io::BufReader;
use tokio::process::Command as TokioCommand;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct VolumeConfig {
    pub paths: Vec<VolumePath>,
    #[serde(default = "default_cache_dir")]
    pub cache_dir: String,
    #[serde(default)]
    pub symlinks: Vec<SymlinkConfig>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SymlinkConfig {
    pub source: String,
    pub symlink_path: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct VolumePath {
    pub source: String,
    pub dest: String,
    #[serde(default)]
    pub resync: bool,
    #[serde(default = "default_continuous")]
    pub continuous: bool,
    #[serde(default = "default_volume_driver")]
    pub driver: V1VolumeDriver,
}

fn default_volume_driver() -> V1VolumeDriver {
    V1VolumeDriver::RCLONE_SYNC
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
            symlinks: Vec::new(),
        }
    }

    /// Add a new path to the sync configuration
    pub fn add_path(
        &mut self,
        source: String,
        dest: String,
        resync: bool,
        driver: V1VolumeDriver,
        continuous: bool,
    ) {
        self.paths.push(VolumePath {
            source,
            dest,
            resync,
            continuous,
            driver,
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
    pub fn list_paths(&self) -> Vec<(&String, &String, bool, V1VolumeDriver, bool)> {
        self.paths
            .iter()
            .map(|path| {
                (
                    &path.source,
                    &path.dest,
                    path.resync,
                    path.driver.clone(),
                    path.continuous,
                )
            })
            .collect()
    }

    /// Add a symlink configuration
    pub fn add_symlink_config(
        &mut self,
        source: String,
        symlink_path: String,
    ) -> Result<(), Box<dyn Error>> {
        // Check if the source path exists in the configuration
        let source_exists = self.paths.iter().any(|path| path.source == source);

        if !source_exists {
            return Err(format!(
                "Source path '{}' does not exist in the configuration",
                source
            )
            .into());
        }

        // Check if we already have a symlink config for this source path
        for symlink_config in &self.symlinks {
            if symlink_config.source == source && symlink_config.symlink_path == symlink_path {
                return Err(format!(
                    "Symlink from '{}' to '{}' already exists",
                    source, symlink_path
                )
                .into());
            }
        }

        // Create a new symlink config
        self.symlinks.push(SymlinkConfig {
            source,
            symlink_path,
        });

        Ok(())
    }

    /// Remove a symlink from the configuration
    pub fn remove_symlink(
        &mut self,
        source: &str,
        symlink_path: &str,
    ) -> Result<(), Box<dyn Error>> {
        let position = self
            .symlinks
            .iter()
            .position(|config| config.source == source && config.symlink_path == symlink_path);

        if let Some(index) = position {
            self.symlinks.remove(index);
            return Ok(());
        }

        Err(format!("Symlink from '{}' to '{}' not found", source, symlink_path).into())
    }

    /// List all symlinks in the configuration
    pub fn list_all_symlinks(&self) -> Vec<(&str, &str)> {
        self.symlinks
            .iter()
            .map(|config| (config.source.as_str(), config.symlink_path.as_str()))
            .collect()
    }

    /// Get symlinks for a specific source path
    pub fn get_symlinks_for_source(&self, source: &str) -> Vec<&str> {
        self.symlinks
            .iter()
            .filter(|config| config.source == source)
            .map(|config| config.symlink_path.as_str())
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

    // Map to track running processes: (source, dest) -> process
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

        {
            let mut finished = vec![];
            for (path_key, child) in &mut running_processes {
                match child.try_wait() {
                    Ok(Some(status)) => {
                        println!(
                            "Rclone subprocess for {} ⟷ {} exited with code: {:?}",
                            path_key.0,
                            path_key.1,
                            status.code()
                        );
                        finished.push(path_key.clone());
                    }
                    Ok(None) => {
                        // Child is still running
                    }
                    Err(e) => {
                        println!(
                            "Error checking status of {} ⟷ {}: {}",
                            path_key.0, path_key.1, e
                        );
                    }
                }
            }
            // Remove any that have exited
            for f in finished {
                running_processes.remove(&f);
            }
        }

        // Check for removed paths and stop their processes
        let mut paths_to_remove = Vec::new();
        for (path_key, process) in &mut running_processes {
            let still_exists = current_config
                .paths
                .iter()
                .any(|path| (&path.source, &path.dest) == (&path_key.0, &path_key.1));

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

            let path_key = (path.source.clone(), path.dest.clone());

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
                        println!("Started sync process for {} ⟷ {}", path.source, path.dest);
                        running_processes.insert(path_key, process);

                        // If this was a resync operation, mark it as completed
                        if path.resync {
                            // Update the config to set resync to false
                            let mut updated_config = current_config.clone();
                            for p in &mut updated_config.paths {
                                if p.source == path.source && p.dest == path.dest {
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
                            path.source, path.dest, e
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

    // Normalize S3 paths
    let source = normalize_s3_path(&path.source);
    let dest = normalize_s3_path(&path.dest);

    // Create source and destination directories if they don't exist
    ensure_path_exists(&source).await?;
    ensure_path_exists(&dest).await?;

    if path.driver == V1VolumeDriver::RCLONE_BISYNC {
        // Use bisync for bidirectional sync
        cmd.arg("bisync");
        cmd.arg(&source);
        cmd.arg(&dest);

        cmd.arg("--resync");
    } else {
        // Use sync for unidirectional sync
        cmd.arg("sync");
        cmd.arg(&source);
        cmd.arg(&dest);
    }

    // Add resync flag if needed and it's a bidirectional sync
    if path.resync && path.driver == V1VolumeDriver::RCLONE_BISYNC {
        cmd.arg("--resync");
    }

    // cmd.arg("--create-empty-src-dirs");

    // // Add common options
    // cmd.arg("--verbose");
    // cmd.arg("--fast-list");

    // // Add cache directory
    // cmd.arg("--cache-dir");
    // cmd.arg(cache_dir);

    // Add resilient mode for bidirectional sync
    if path.driver == V1VolumeDriver::RCLONE_BISYNC {
        // cmd.arg("--resilient");

        // Add --force flag to help with empty directory issues
        cmd.arg("--force");
    }

    // Spawn the process

    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());

    let mut child = cmd.spawn()?;

    // NEW: Hook up tasks to read the child’s stdout/stderr
    if let Some(stdout) = child.stdout.take() {
        let source_clone = path.source.clone();
        let dest_clone = path.dest.clone();
        tokio::spawn(async move {
            let mut reader = BufReader::new(stdout).lines();
            while let Ok(Some(line)) = reader.next_line().await {
                println!(
                    "[rclone stdout: {} ⟷ {}] {}",
                    source_clone, dest_clone, line
                );
            }
        });
    }
    if let Some(stderr) = child.stderr.take() {
        let source_clone = path.source.clone();
        let dest_clone = path.dest.clone();
        tokio::spawn(async move {
            let mut reader = BufReader::new(stderr).lines();
            while let Ok(Some(line)) = reader.next_line().await {
                println!(
                    "[rclone stderr: {} ⟷ {}] {}",
                    source_clone, dest_clone, line
                );
            }
        });
    }

    Ok(child)
}

/// Normalize an S3 path to ensure it uses the format expected by rclone (s3:bucket/path)
fn normalize_s3_path(path: &str) -> String {
    if path.starts_with("s3://") {
        // Convert s3://bucket/path to s3:bucket/path
        format!("s3:{}", &path[5..])
    } else {
        // Already in the correct format or not an S3 path
        path.to_string()
    }
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
        let sync_type = if path.driver == V1VolumeDriver::RCLONE_BISYNC {
            "Bidirectional"
        } else {
            "Unidirectional"
        };

        println!(
            "[{}/{}] {} syncing between {} and {}",
            index + 1,
            config.paths.len(),
            sync_type,
            path.source,
            path.dest
        );

        // Normalize S3 paths
        let source = normalize_s3_path(&path.source);
        let dest = normalize_s3_path(&path.dest);

        // Create source and destination directories if they don't exist
        ensure_path_exists(&source).await?;
        ensure_path_exists(&dest).await?;

        // Build the rclone command
        let mut cmd = Command::new("rclone");

        // Normalize S3 paths
        let source = normalize_s3_path(&path.source);
        let dest = normalize_s3_path(&path.dest);

        if path.driver == V1VolumeDriver::RCLONE_BISYNC {
            cmd.arg("bisync");
            cmd.arg(&source);
            cmd.arg(&dest);

            // Add --resync flag if needed
            if path.resync {
                cmd.arg("--resync");
            }

            // Add --force flag to help with empty directory issues
            cmd.arg("--force");
        } else {
            cmd.arg("sync");
            cmd.arg(&source);
            cmd.arg(&dest);
        }

        // Add common options
        // cmd.arg("--verbose");
        // cmd.arg("--fast-list");
        // cmd.arg("--create-empty-src-dirs");

        // // Add cache directory
        // cmd.arg("--cache-dir");
        // cmd.arg(&config.cache_dir);

        // Execute the command
        let output = cmd.output()?;
        if output.status.success() {
            println!(
                "Successfully synced between {} and {}",
                path.source, path.dest
            );

            // If this was a resync operation, mark it as completed
            if path.resync && path.driver == V1VolumeDriver::RCLONE_BISYNC {
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

            // Check if this is the "empty prior listing" error and we need to resync
            if error.contains("empty prior Path1 listing")
                || error.contains("Must run --resync to recover")
            {
                println!("Detected need for resync. Attempting to resync...");

                // Create a new command with --resync flag
                let mut resync_cmd = Command::new("rclone");
                resync_cmd.arg("bisync");
                resync_cmd.arg(&source);
                resync_cmd.arg(&dest);
                resync_cmd.arg("--resync");
                resync_cmd.arg("--force");
                // resync_cmd.arg("--verbose");
                // resync_cmd.arg("--fast-list");
                // resync_cmd.arg("--create-empty-src-dirs");
                // resync_cmd.arg("--cache-dir");
                // resync_cmd.arg(&config.cache_dir);

                // Execute the resync command
                let resync_output = resync_cmd.output()?;
                if resync_output.status.success() {
                    println!("Resync successful between {} and {}", source, dest);
                } else {
                    let resync_error = String::from_utf8_lossy(&resync_output.stderr);
                    println!("Resync failed: {}", resync_error);
                }
            }
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
        true,                          // Initial sync should use resync
        V1VolumeDriver::RCLONE_BISYNC, // bidirectional
        true,                          // continuous
    );

    // Add example for unidirectional one-time sync
    config.add_path(
        "/path/to/local/directory2".to_string(),
        "s3:your-bucket/directory2".to_string(),
        false,                       // No resync needed for unidirectional
        V1VolumeDriver::RCLONE_SYNC, // unidirectional
        false,                       // one-time
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
    volume_type: V1VolumeDriver,
    continuous: bool,
) -> Result<(), Box<dyn Error>> {
    // Read the existing config or create a new one if it doesn't exist
    let mut config = if Path::new(config_path).exists() {
        VolumeConfig::read_from_file(config_path)?
    } else {
        VolumeConfig::new()
    };

    // Add the new path with resync set to true for initial bidirectional sync
    let resync = volume_type.clone() == V1VolumeDriver::RCLONE_BISYNC; // Only set resync true if bidirectional
    config.add_path(
        source.clone(),
        dest.clone(),
        resync,
        volume_type.clone(),
        continuous,
    );

    // Write the updated config back to the file
    config.write_to_file(config_path)?;

    let sync_type = if volume_type.clone() == V1VolumeDriver::RCLONE_BISYNC {
        "bidirectional"
    } else {
        "unidirectional"
    };
    let sync_mode = if continuous { "continuous" } else { "once" };

    println!(
        "Added {} {} sync path between {} and {}",
        sync_type, sync_mode, source, dest
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
    for (i, (source, destination, resync, volume_type, continuous)) in
        config.list_paths().iter().enumerate()
    {
        let direction = if *volume_type == V1VolumeDriver::RCLONE_BISYNC {
            "bidirectional"
        } else {
            "unidirectional"
        };
        let mode = if *continuous { "continuous" } else { "once" };
        let resync_status = if *resync && *volume_type == V1VolumeDriver::RCLONE_BISYNC {
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

/// ------------------------------------------------------------------------------------------------
/// Symlinks
/// ------------------------------------------------------------------------------------------------

/// Add a symlink to a path in the configuration
pub fn add_symlink_to_path(
    config_path: &str,
    path_index: usize,
    symlink_path: &str,
) -> Result<(), Box<dyn Error>> {
    // Read the existing config
    let mut config = VolumeConfig::read_from_file(config_path)?;

    // Add the symlink to the configuration
    let source = config.paths[path_index].source.clone();
    config.add_symlink_config(source, symlink_path.to_string())?;

    // Write the updated config back to the file
    config.write_to_file(config_path)?;

    println!(
        "Added symlink {} to path index {}",
        symlink_path, path_index
    );
    Ok(())
}

/// Remove a symlink from a path in the configuration
pub fn remove_symlink_from_path(
    config_path: &str,
    path_index: usize,
    symlink_index: usize,
) -> Result<(), Box<dyn Error>> {
    // Read the existing config
    let mut config = VolumeConfig::read_from_file(config_path)?;

    // Get the source path for the specified path index
    if path_index >= config.paths.len() {
        return Err(format!("Path index {} out of bounds", path_index).into());
    }
    let source = config.paths[path_index].source.clone();

    // Find all symlinks for this source path
    let matching_symlinks: Vec<_> = config
        .symlinks
        .iter()
        .filter(|s| s.source == source)
        .collect();

    if symlink_index >= matching_symlinks.len() {
        return Err(format!("Symlink index {} out of bounds", symlink_index).into());
    }

    // Get the symlink path before removing it
    let symlink_path = matching_symlinks[symlink_index].symlink_path.clone();

    // Remove the symlink
    config.remove_symlink(&source, &symlink_path)?;

    // Write the updated config back to the file
    config.write_to_file(config_path)?;

    println!(
        "Removed symlink {} from path index {}",
        symlink_path, path_index
    );
    Ok(())
}

/// List all symlinks for a path in the configuration
pub fn list_symlinks_for_path(config_path: &str, path_index: usize) -> Result<(), Box<dyn Error>> {
    // Read the existing config
    let config = VolumeConfig::read_from_file(config_path)?;

    // Get the source path for the specified path index
    if path_index >= config.paths.len() {
        return Err(format!("Path index {} out of bounds", path_index).into());
    }
    let source = &config.paths[path_index].source;

    // Get the symlinks for the specified path
    let symlinks = config.get_symlinks_for_source(source);

    if symlinks.is_empty() {
        println!("No symlinks found for path index {}", path_index);
        return Ok(());
    }

    println!("Symlinks for path index {}:", path_index);
    for (i, symlink) in symlinks.iter().enumerate() {
        println!("[{}] {}", i, symlink);
    }

    Ok(())
}

/// Create symlinks defined in the configuration
pub fn create_symlinks_from_config(config_path: &str) -> Result<(), Box<dyn Error>> {
    // Read the existing config
    let config = VolumeConfig::read_from_file(config_path)?;

    let mut created_count = 0;
    let mut error_count = 0;

    // Process each symlink configuration
    for symlink_config in &config.symlinks {
        // Skip remote paths
        if symlink_config.source.contains(":") {
            println!(
                "Skipping symlink for remote path: {}",
                symlink_config.source
            );
            continue;
        }

        // Create parent directories for the symlink if they don't exist
        if let Some(parent) = Path::new(&symlink_config.symlink_path).parent() {
            if let Err(e) = fs::create_dir_all(parent) {
                println!(
                    "Failed to create parent directories for symlink {}: {}",
                    symlink_config.symlink_path, e
                );
                error_count += 1;
                continue;
            }
        }

        // Remove existing symlink if it exists
        if Path::new(&symlink_config.symlink_path).exists() {
            if Path::new(&symlink_config.symlink_path).is_symlink() {
                if let Err(e) = fs::remove_file(&symlink_config.symlink_path) {
                    println!(
                        "Failed to remove existing symlink {}: {}",
                        symlink_config.symlink_path, e
                    );
                    error_count += 1;
                    continue;
                }
            } else {
                println!(
                    "Destination path exists and is not a symlink: {}",
                    symlink_config.symlink_path
                );
                error_count += 1;
                continue;
            }
        }

        // Create the symlink
        let result = {
            #[cfg(unix)]
            {
                std::os::unix::fs::symlink(&symlink_config.source, &symlink_config.symlink_path)
            }
            #[cfg(windows)]
            {
                match fs::metadata(&symlink_config.source) {
                    Ok(metadata) => {
                        if metadata.is_dir() {
                            std::os::windows::fs::symlink_dir(
                                &symlink_config.source,
                                &symlink_config.symlink_path,
                            )
                        } else {
                            std::os::windows::fs::symlink_file(
                                &symlink_config.source,
                                &symlink_config.symlink_path,
                            )
                        }
                    }
                    Err(e) => Err(e),
                }
            }
        };

        match result {
            Ok(_) => {
                println!(
                    "Created symlink from {} to {}",
                    symlink_config.source, symlink_config.symlink_path
                );
                created_count += 1;
            }
            Err(e) => {
                println!(
                    "Failed to create symlink from {} to {}: {}",
                    symlink_config.source, symlink_config.symlink_path, e
                );
                error_count += 1;
            }
        }
    }

    println!(
        "Symlink creation completed: {} created, {} failed",
        created_count, error_count
    );
    Ok(())
}

/// Add a symlink to the configuration
pub fn add_symlink(
    config_path: &str,
    source: &str,
    symlink_path: &str,
) -> Result<(), Box<dyn Error>> {
    // Read the existing config
    let mut config = VolumeConfig::read_from_file(config_path)?;

    // Add the symlink to the configuration
    config.add_symlink_config(source.to_string(), symlink_path.to_string())?;

    // Write the updated config back to the file
    config.write_to_file(config_path)?;

    println!("Added symlink from {} to {}", source, symlink_path);
    Ok(())
}

/// Remove a symlink from the configuration
pub fn remove_symlink(
    config_path: &str,
    source: &str,
    symlink_path: &str,
) -> Result<(), Box<dyn Error>> {
    // Read the existing config
    let mut config = VolumeConfig::read_from_file(config_path)?;

    // Remove the symlink from the configuration
    config.remove_symlink(source, symlink_path)?;

    // Write the updated config back to the file
    config.write_to_file(config_path)?;

    println!("Removed symlink from {} to {}", source, symlink_path);
    Ok(())
}

/// List all symlinks in the configuration
pub fn list_symlinks(config_path: &str) -> Result<(), Box<dyn Error>> {
    // Read the existing config
    let config = VolumeConfig::read_from_file(config_path)?;

    let all_symlinks = config.list_all_symlinks();

    if all_symlinks.is_empty() {
        println!("No symlinks found in the configuration.");
        return Ok(());
    }

    println!("Symlinks in the configuration:");
    for (i, (source, symlink_path)) in all_symlinks.iter().enumerate() {
        println!("[{}] {} -> {}", i, source, symlink_path);
    }

    Ok(())
}

/// List symlinks for a specific source path
pub fn list_symlinks_for_source(config_path: &str, source: &str) -> Result<(), Box<dyn Error>> {
    // Read the existing config
    let config = VolumeConfig::read_from_file(config_path)?;

    let symlinks = config.get_symlinks_for_source(source);

    if symlinks.is_empty() {
        println!("No symlinks found for source path: {}", source);
        return Ok(());
    }

    println!("Symlinks for source path: {}", source);
    for (i, symlink_path) in symlinks.iter().enumerate() {
        println!("[{}] {}", i, symlink_path);
    }

    Ok(())
}

/// Execute one-time sync for non-continuous paths in the configuration
pub async fn execute_non_continuous_sync(
    config_path: &str,
    create_if_missing: bool,
) -> Result<(), Box<dyn Error>> {
    println!(
        "Executing one-time sync for non-continuous paths from: {}",
        config_path
    );

    if !Path::new(config_path).exists() {
        if create_if_missing {
            create_empty_config(config_path)?;
        } else {
            return Err(format!("Config file not found: {}", config_path).into());
        }
    }

    // Read the configuration
    let config = VolumeConfig::read_from_file(config_path)?;

    // Filter out non-continuous paths
    let once_paths: Vec<_> = config
        .paths
        .iter()
        .filter(|path| !path.continuous)
        .collect();

    if once_paths.is_empty() {
        println!("No non-continuous paths found in the configuration.");
        return Ok(());
    }

    println!("Found {} non-continuous paths to sync", once_paths.len());

    // Process each non-continuous path
    for (index, path) in once_paths.iter().enumerate() {
        let sync_type = if path.driver.clone() == V1VolumeDriver::RCLONE_BISYNC {
            "Bidirectional"
        } else {
            "Unidirectional"
        };

        println!(
            "[{}/{}] {} syncing between {} and {}",
            index + 1,
            once_paths.len(),
            sync_type,
            path.source,
            path.dest
        );

        // Normalize S3 paths
        let source = normalize_s3_path(&path.source);
        let dest = normalize_s3_path(&path.dest);

        // Create source and destination directories if they don't exist
        ensure_path_exists(&source).await?;
        ensure_path_exists(&dest).await?;

        // Build the rclone command
        let mut cmd = Command::new("rclone");

        // Normalize S3 paths
        let source = normalize_s3_path(&path.source);
        let dest = normalize_s3_path(&path.dest);

        if path.driver.clone() == V1VolumeDriver::RCLONE_BISYNC {
            cmd.arg("bisync");
            cmd.arg(&source);
            cmd.arg(&dest);

            cmd.arg("--resync");

            // Add --force flag to help with empty directory issues
            cmd.arg("--force");
        } else {
            cmd.arg("sync");
            cmd.arg(&source);
            cmd.arg(&dest);
        }

        // Add common options
        // cmd.arg("--verbose");
        // cmd.arg("--fast-list");
        // cmd.arg("--create-empty-src-dirs");

        // Add cache directory
        // cmd.arg("--cache-dir");
        // cmd.arg(&config.cache_dir);

        // Execute the command
        let output = cmd.output()?;
        if output.status.success() {
            println!("Successfully synced between {} and {}", source, dest);

            // If this was a resync operation, mark it as completed
            if path.resync && path.driver.clone() == V1VolumeDriver::RCLONE_BISYNC {
                // Find the index in the original config
                if let Some(original_index) = config
                    .paths
                    .iter()
                    .position(|p| p.source == path.source && p.dest == path.dest)
                {
                    // Read the config again to get the latest version
                    let mut updated_config = VolumeConfig::read_from_file(config_path)?;
                    if original_index < updated_config.paths.len() {
                        updated_config.paths[original_index].resync = false;
                        updated_config.write_to_file(config_path)?;
                    }
                }
            }
        } else {
            let error = String::from_utf8_lossy(&output.stderr);
            println!("Failed to sync: {}", error);

            // Check if this is the "empty prior listing" error and we need to resync
            if error.contains("empty prior Path1 listing")
                || error.contains("Must run --resync to recover")
            {
                println!("Detected need for resync. Attempting to resync...");

                // Create a new command with --resync flag
                let mut resync_cmd = Command::new("rclone");
                resync_cmd.arg("bisync");
                resync_cmd.arg(&source);
                resync_cmd.arg(&dest);
                resync_cmd.arg("--resync");
                resync_cmd.arg("--force");
                // resync_cmd.arg("--verbose");
                // resync_cmd.arg("--fast-list");
                // resync_cmd.arg("--create-empty-src-dirs");
                // resync_cmd.arg("--cache-dir");
                // resync_cmd.arg(&config.cache_dir);

                // Execute the resync command
                let resync_output = resync_cmd.output()?;
                if resync_output.status.success() {
                    println!("Resync successful between {} and {}", source, dest);
                } else {
                    let resync_error = String::from_utf8_lossy(&resync_output.stderr);
                    println!("Resync failed: {}", resync_error);
                }
            }
        }
    }

    println!("Non-continuous sync operation completed");
    Ok(())
}

/// ------------------------------------------------------------------------------------------------
/// Sync checkers
/// ------------------------------------------------------------------------------------------------

/// Checks if two volume configs both bidirectionally sync from the same S3 path or child paths.
///
/// Returns true if both configs have at least one bidirectional sync path that
/// shares the same S3 path or where one path is a child of the other.
pub fn has_overlapping_s3_bidirectional_sync(
    config1: &VolumeConfig,
    config2: &VolumeConfig,
) -> bool {
    // Get all bidirectional paths with S3 sources from both configs
    let config1_s3_paths: Vec<String> = config1
        .paths
        .iter()
        .filter(|path| {
            path.driver.clone() == V1VolumeDriver::RCLONE_BISYNC
                && (path.source.starts_with("s3://") || path.source.starts_with("s3:"))
        })
        .map(|path| normalize_s3_path(&path.source))
        .collect();

    let config2_s3_paths: Vec<String> = config2
        .paths
        .iter()
        .filter(|path| {
            path.driver.clone() == V1VolumeDriver::RCLONE_BISYNC
                && (path.source.starts_with("s3://") || path.source.starts_with("s3:"))
        })
        .map(|path| normalize_s3_path(&path.source))
        .collect();

    // Check for overlapping paths
    for path1 in &config1_s3_paths {
        for path2 in &config2_s3_paths {
            // Check if paths are the same
            if path1 == path2 {
                return true;
            }

            // Check if one path is a child of the other
            // We need to ensure we're comparing paths with trailing slashes to avoid partial matches
            let path1_normalized = if path1.ends_with('/') {
                path1.to_string()
            } else {
                format!("{}/", path1)
            };

            let path2_normalized = if path2.ends_with('/') {
                path2.to_string()
            } else {
                format!("{}/", path2)
            };

            if path1_normalized.starts_with(&path2_normalized)
                || path2_normalized.starts_with(&path1_normalized)
            {
                return true;
            }
        }
    }

    false
}

/// Checks if a volume configuration has overlapping S3 bidirectional syncs with any active container in the database.
/// Active containers are those with status "running", "queued", "started", or "waiting".
///
/// Returns a vector of container IDs that have overlapping configurations.
pub async fn find_active_containers_with_overlapping_s3_sync(
    db: &DatabaseConnection,
    new_config: &VolumeConfig,
    exclude_container_id: Option<&str>,
) -> Result<Vec<String>, DbErr> {
    // Get all containers from the database
    let all_containers = Query::find_all_containers(db).await?;

    let mut overlapping_container_ids = Vec::new();

    for container in all_containers {
        // Skip the container we're updating (if provided)
        if let Some(exclude_id) = exclude_container_id {
            if container.id == exclude_id {
                continue;
            }
        }

        // Check if container has an active status
        if let Some(status_json) = &container.status {
            // Deserialize the JSON to V1ContainerStatus
            match serde_json::from_value::<V1ContainerStatus>(status_json.clone()) {
                Ok(status) => {
                    let status_str = match status.status {
                        Some(s) => s.to_lowercase(),
                        None => continue, // Skip if status is None
                    };

                    if ![
                        "running",
                        "queued",
                        "started",
                        "waiting",
                        "pending",
                        "suspended",
                    ]
                    .contains(&status_str.as_str())
                    {
                        continue; // Skip containers that aren't active
                    }
                }
                Err(_) => continue, // Skip if deserialization fails
            }
        } else {
            continue; // Skip containers with no status
        }

        // Parse the volumes JSON from the database
        if let Some(volumes_json) = &container.volumes {
            match from_str::<VolumeConfig>(&volumes_json.to_string()) {
                Ok(existing_config) => {
                    // Check for overlapping S3 bidirectional syncs
                    if has_overlapping_s3_bidirectional_sync(new_config, &existing_config) {
                        overlapping_container_ids.push(container.id.clone());
                    }
                }
                Err(_) => {
                    // Skip containers with invalid volume configurations
                    continue;
                }
            }
        }
    }

    Ok(overlapping_container_ids)
}

/// Validates that a volume configuration doesn't overlap with any existing container's S3 bidirectional syncs.
///
/// Returns Ok(()) if no overlaps are found, or an error with details about the overlapping containers.
pub async fn validate_no_overlapping_s3_syncs(
    db: &DatabaseConnection,
    new_config: &VolumeConfig,
    exclude_container_id: Option<&str>,
) -> Result<(), DbErr> {
    let overlapping_containers =
        find_active_containers_with_overlapping_s3_sync(db, new_config, exclude_container_id)
            .await?;

    if overlapping_containers.is_empty() {
        Ok(())
    } else {
        Err(DbErr::Custom(format!(
            "The volume configuration has overlapping S3 bidirectional syncs with existing containers: {}",
            overlapping_containers.join(", ")
        )))
    }
}

/// Sets up rclone configuration from environment variables if available
/// Returns a temporary file handle if a config was created (to keep it alive)
pub fn setup_rclone_config_from_env() -> Result<Option<std::fs::File>, Box<dyn Error>> {
    // Check if we have rclone config environment variables
    let has_s3_env_vars = std::env::var("RCLONE_CONFIG_S3REMOTE_TYPE").is_ok()
        || std::env::var("AWS_ACCESS_KEY_ID").is_ok();

    if has_s3_env_vars {
        // Create a temporary rclone config file
        let temp_path = std::env::temp_dir().join("rclone_config.conf");
        let mut temp_file = std::fs::File::create(&temp_path)?;
        use std::io::Write;

        // Write S3 remote configuration
        writeln!(temp_file, "[s3]")?;
        writeln!(temp_file, "type = s3")?;

        // Add region if available
        if let Ok(region) = std::env::var("RCLONE_CONFIG_S3REMOTE_REGION") {
            writeln!(temp_file, "region = {}", region)?;
        } else if let Ok(region) = std::env::var("AWS_REGION") {
            writeln!(temp_file, "region = {}", region)?;
        }

        // Add provider if available
        if let Ok(provider) = std::env::var("RCLONE_CONFIG_S3REMOTE_PROVIDER") {
            writeln!(temp_file, "provider = {}", provider)?;
        }

        // Add env_auth if available
        if let Ok(env_auth) = std::env::var("RCLONE_CONFIG_S3REMOTE_ENV_AUTH") {
            writeln!(temp_file, "env_auth = {}", env_auth)?;
        } else {
            // Default to true if AWS credentials are in environment
            if std::env::var("AWS_ACCESS_KEY_ID").is_ok()
                && std::env::var("AWS_SECRET_ACCESS_KEY").is_ok()
            {
                writeln!(temp_file, "env_auth = true")?;
            }
        }

        // Set the RCLONE_CONFIG environment variable to point to our temporary file
        std::env::set_var("RCLONE_CONFIG", temp_path.to_string_lossy().to_string());

        println!("Created temporary rclone configuration from environment variables");
        Ok(Some(temp_file))
    } else {
        Ok(None)
    }
}

/// Create a directory in S3 if it doesn't exist
async fn create_s3_directory(path: &str) -> Result<(), Box<dyn Error>> {
    // Extract bucket and prefix from s3:bucket/path format
    let path = path.trim_start_matches("s3:");
    let parts: Vec<&str> = path.splitn(2, '/').collect();

    if parts.len() < 2 {
        // Just a bucket name, no need to create anything
        return Ok(());
    }

    let bucket = parts[0];
    let prefix = parts[1];

    // Use rclone to check if the directory exists
    let check_cmd = TokioCommand::new("rclone")
        .arg("lsf")
        .arg(format!("s3:{}/{}", bucket, prefix))
        .output()
        .await?;

    // If the command was successful and returned output, the directory exists
    if check_cmd.status.success() && !check_cmd.stdout.is_empty() {
        return Ok(());
    }

    println!("Creating S3 directory: s3:{}/{}", bucket, prefix);

    // Create an empty directory marker object
    // We'll create a temporary empty file locally
    let temp_dir = tempfile::tempdir()?;
    let temp_file_path = temp_dir.path().join(".rclone_directory_marker");
    std::fs::write(&temp_file_path, "")?;

    // Use rclone to copy the empty file to S3 as a directory marker
    let create_cmd = TokioCommand::new("rclone")
        .arg("copy")
        .arg(&temp_file_path)
        .arg(format!("s3:{}/{}/", bucket, prefix))
        .output()
        .await?;

    if !create_cmd.status.success() {
        let error = String::from_utf8_lossy(&create_cmd.stderr);
        return Err(format!("Failed to create S3 directory: {}", error).into());
    }

    Ok(())
}

/// Ensure a path exists, creating it if necessary (works for both local and S3 paths)
async fn ensure_path_exists(path: &str) -> Result<(), Box<dyn Error>> {
    let normalized_path = normalize_s3_path(path);

    if normalized_path.starts_with("s3:") {
        // S3 path
        create_s3_directory(&normalized_path).await?;
    } else if !Path::new(&normalized_path).exists() {
        // Local path
        println!("Creating local directory: {}", normalized_path);
        fs::create_dir_all(&normalized_path)?;
    }

    Ok(())
}
