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
) -> Result<(), Box<dyn Error>> {
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

            Command::new("nohup")
                .arg(std::env::current_exe()?)
                .args(args.iter().filter(|&arg| !arg.is_empty()))
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()?;

            println!("Background sync process started. You can safely exit this program.");
            Ok(())
        } else {
            // Normal continuous sync in foreground
            rclone::execute_continuous_sync(config_path, interval_seconds, create_if_missing).await
        }
    } else {
        if background {
            // For Unix-like systems, spawn a detached process for one-time sync
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

            Command::new("nohup")
                .arg(std::env::current_exe()?)
                .args(args.iter().filter(|&arg| !arg.is_empty()))
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()?;

            println!("Background sync process started. You can safely exit this program.");
            Ok(())
        } else {
            // Normal one-time sync in foreground
            rclone::execute_sync(config_path, create_if_missing).await
        }
    }
}
