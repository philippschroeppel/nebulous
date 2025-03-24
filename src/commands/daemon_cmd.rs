use std::error::Error;
use std::fs::OpenOptions;
use std::process::{Command, Stdio};

pub async fn execute_daemon(host: &str, port: u16, background: bool) -> Result<(), Box<dyn Error>> {
    if background {
        println!("Starting server in the background...");

        // Create log directory (same approach as in the sync_cmd example)
        let log_dir = std::env::var("NEBU_LOG_DIR").unwrap_or_else(|_| "./logs".to_string());
        std::fs::create_dir_all(&log_dir)?;

        let timestamp = chrono::Local::now().format("%Y%m%d_%H%M%S");
        let stdout_log = format!("{}/neblet_stdout_{}.log", log_dir, timestamp);
        let stderr_log = format!("{}/neblet_stderr_{}.log", log_dir, timestamp);

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

        // Build args for spawning. Similar to how sync_cmd handles background logic,
        // we'll re-invoke the same binary with subcommand "serve", omitting `--background`.
        let exe = std::env::current_exe()?;
        let port_str = format!("{}", port);
        let args = vec!["daemon", "--host", host, "--port", &port_str];

        Command::new("nohup")
            .arg(exe.clone())
            .args(args.clone())
            .spawn()?;

        Command::new("nohup")
            .arg(exe)
            .args(args)
            .stdout(Stdio::from(stdout_file))
            .stderr(Stdio::from(stderr_file))
            .spawn()?;

        println!("Background server process started. You can safely exit this program.");
        println!("Logs: stdout → {} | stderr → {}", stdout_log, stderr_log);

        Ok(())
    } else {
        // Foreground server, mirrors "rclone::execute_continuous_sync" style in sync_cmd.
        // Example "run_server" function from your "src/neblet/server.rs".
        nebulous::neblet::daemon::run_server(host, port).await
    }
}
