use nebulous::volumes::rclone;
use std::error::Error;
use tokio::time::{sleep, Duration};

// Example config
//---
// paths:
//   - source: "/path/to/local/directory1"
//     dest: "s3://your-bucket/directory1"
//   - source: "/path/to/local/directory2"
//     dest: "s3://your-bucket/directory2"
//   - source: "/path/to/local/file.txt"
//     dest: "s3://your-bucket/file.txt"

pub async fn execute_sync(
    config_path: String,
    interval_seconds: u64,
    create_if_missing: bool,
    watch: bool,
    background: bool,
    block_once: bool,
    config_from_env: bool,
) -> Result<(), Box<dyn Error>> {
    // If config_from_env is true, attempt to read from NEBU_SYNC_CONFIG
    // and write the contents to the config_path file.
    if config_from_env {
        match std::env::var("NEBU_SYNC_CONFIG") {
            Ok(env_config) => {
                // Create parent directories if they don't exist
                if let Some(parent) = std::path::Path::new(&config_path).parent() {
                    if !parent.exists() {
                        if let Err(e) = std::fs::create_dir_all(parent) {
                            eprintln!(
                                "Warning: Failed to create directory for config file {}: {}",
                                config_path, e
                            );
                        }
                    }
                }

                if let Err(e) = std::fs::write(&config_path, &env_config) {
                    eprintln!(
                        "Warning: Failed to write config from environment variable to {}: {}",
                        config_path, e
                    );
                }
            }
            Err(_) => {
                println!(
                    "Warning: NEBU_SYNC_CONFIG environment variable not set, using provided config path: {}",
                    config_path
                );
            }
        }
    }

    // Setup rclone configuration from environment variables if available
    // Keep the temp file alive for the duration of the function
    let _rclone_config = rclone::setup_rclone_config_from_env()?;

    // Create symlinks before starting any sync operations
    if let Err(e) = rclone::create_symlinks_from_config(&config_path) {
        println!("Warning: Failed to create symlinks: {}", e);
    }

    // If block_once is true, execute non-continuous paths in a blocking fashion
    if block_once {
        if let Err(e) = rclone::execute_non_continuous_sync(&config_path, create_if_missing).await {
            println!("Warning: Failed to execute non-continuous paths: {}", e);
        }
    }

    if watch {
        if background {
            // For Unix-like systems, spawn a detached process
            use std::fs::OpenOptions;
            use std::process::{Command, Stdio};

            println!("Starting continuous sync in background...");
            let create_arg = if create_if_missing {
                "--create-if-missing"
            } else {
                ""
            };
            let interval_str = interval_seconds.to_string();
            let args = vec![
                "sync",
                "--config",
                &config_path,
                "--interval-seconds",
                &interval_str,
                "--watch",
                create_arg,
            ];

            // Create log files for stdout and stderr
            let log_dir = std::env::var("NEBU_LOG_DIR").unwrap_or_else(|_| "./logs".to_string());
            std::fs::create_dir_all(&log_dir)?;

            let timestamp = chrono::Local::now().format("%Y%m%d_%H%M%S");
            let stdout_log = format!("{}/nebu_sync_stdout_{}.log", log_dir, timestamp);
            let stderr_log = format!("{}/nebu_sync_stderr_{}.log", log_dir, timestamp);

            let stdout_file = OpenOptions::new()
                .write(true)
                .create(true)
                .append(true)
                .open(&stdout_log)?;

            let stderr_file = OpenOptions::new()
                .write(true)
                .create(true)
                .append(true)
                .open(&stderr_log)?;

            Command::new("nohup")
                .arg(std::env::current_exe()?)
                .args(args.iter().filter(|&arg| !arg.is_empty()))
                .stdout(Stdio::from(stdout_file))
                .stderr(Stdio::from(stderr_file))
                .spawn()?;

            println!("Background sync process started. You can safely exit this program.");
            println!("Logs are stored at: {} and {}", stdout_log, stderr_log);
            Ok(())
        } else {
            // Normal continuous sync in foreground
            rclone::execute_continuous_sync(config_path, interval_seconds, create_if_missing).await
        }
    } else {
        if background {
            // For Unix-like systems, spawn a detached process for one-time sync
            use std::fs::OpenOptions;
            use std::process::{Command, Stdio};

            println!("Starting one-time sync in background...");
            let args = vec![
                "sync",
                "--config",
                &config_path,
                if create_if_missing {
                    "--create-if-missing"
                } else {
                    ""
                },
            ];

            // Create log files for stdout and stderr
            let log_dir = std::env::var("NEBU_LOG_DIR").unwrap_or_else(|_| "./logs".to_string());
            std::fs::create_dir_all(&log_dir)?;

            let timestamp = chrono::Local::now().format("%Y%m%d_%H%M%S");
            let stdout_log = format!("{}/nebu_sync_stdout_{}.log", log_dir, timestamp);
            let stderr_log = format!("{}/nebu_sync_stderr_{}.log", log_dir, timestamp);

            let stdout_file = OpenOptions::new()
                .write(true)
                .create(true)
                .append(true)
                .open(&stdout_log)?;

            let stderr_file = OpenOptions::new()
                .write(true)
                .create(true)
                .append(true)
                .open(&stderr_log)?;

            Command::new("nohup")
                .arg(std::env::current_exe()?)
                .args(args.iter().filter(|&arg| !arg.is_empty()))
                .stdout(Stdio::from(stdout_file))
                .stderr(Stdio::from(stderr_file))
                .spawn()?;

            println!("Background sync process started. You can safely exit this program.");
            println!("Logs are stored at: {} and {}", stdout_log, stderr_log);
            Ok(())
        } else {
            // Normal one-time sync in foreground
            rclone::execute_sync(config_path, create_if_missing).await
        }
    }
}

/// Continuously checks for differences between two paths using `rsync` in "check" mode (\
/// --dry-run + --itemize-changes). It will keep looping until no differences are found.
///
/// * `source` - The source path (e.g., "/path/to/source").
/// * `dest` - The destination path (e.g., "/path/to/destination").
/// * `poll_interval` - How long to wait (in seconds) between checks.
///
/// This function returns once differences are finally zero, or if it hits
/// an optional maximum iteration limit (if you implement one).
#[allow(dead_code)]
pub async fn execute_wait(
    config_path: &str,
    poll_interval: u64,
) -> Result<(), Box<dyn std::error::Error>> {
    use nebulous::volumes::rclone::{check_paths, VolumeConfig};

    loop {
        // Load the config (re-reads each loop in case it changes)
        let config = match VolumeConfig::read_from_file(config_path) {
            Ok(cfg) => cfg,
            Err(e) => {
                eprintln!("Failed to read config file {}: {}", config_path, e);
                // If you need to bail out when config is missing, return Err here.
                // Otherwise, just wait and retry.
                tokio::time::sleep(std::time::Duration::from_secs(poll_interval)).await;
                continue;
            }
        };

        if config.paths.is_empty() {
            println!("No paths found in {}. Nothing to check.", config_path);
            return Ok(());
        }

        let mut all_clean = true;

        // Compare each source/dest pair
        for path in &config.paths {
            // Now check_paths returns `bool`:
            let in_sync = check_paths(&path.source, &path.dest).await?;
            if !in_sync {
                // If any path is out of sync, we set all_clean to false.
                println!(
                    "Differences found in {} → {}. They are not currently matched.",
                    path.source, path.dest
                );
                all_clean = false;
            }
        }

        if all_clean {
            println!(
                "All entries in {} are now in sync! No differences remain.",
                config_path
            );
            break;
        } else {
            println!(
                "Some differences remain. Checking again in {} seconds...",
                poll_interval
            );
            tokio::time::sleep(std::time::Duration::from_secs(poll_interval)).await;
        }
    }

    Ok(())
}
