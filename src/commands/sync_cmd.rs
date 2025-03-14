use nebulous::volumes::rclone;
use std::error::Error;

// Example config
//---
// paths:
//   - source_path: "/path/to/local/directory1"
//     destination_path: "s3://your-bucket/directory1"
//   - source_path: "/path/to/local/directory2"
//     destination_path: "s3://your-bucket/directory2"
//   - source_path: "/path/to/local/file.txt"
//     destination_path: "s3://your-bucket/file.txt"

pub async fn execute_sync(
    config_path: String,
    interval_seconds: u64,
    create_if_missing: bool,
    watch: bool,
    background: bool,
    block_once: bool,
    config_from_env: bool,
) -> Result<(), Box<dyn Error>> {
    // Get config path from environment variable if sync_from_env is true
    let config_path = if config_from_env {
        std::env::var("NEBU_SYNC_CONFIG").unwrap_or_else(|_| {
            println!("Warning: NEBU_SYNC_CONFIG environment variable not set, using provided config path");
            config_path
        })
    } else {
        config_path
    };

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
                "--interval",
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
