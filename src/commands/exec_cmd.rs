use crate::cli::ExecArgs;
use reqwest::Client;
use serde::Deserialize;
use std::error::Error as StdError;

/// This is your main “exec” function, to be called from your CLI command.  
/// 1) Fetch container via HTTP API to retrieve its ID.  
/// 2) Run local SSH command using `run_ssh_command_ts`.
pub async fn exec_cmd(args: ExecArgs) -> Result<(), Box<dyn StdError>> {
    // Step 1: Fetch container ID by calling your server’s HTTP GET /v1/containers/:namespace/:name
    let container_id = fetch_container_id_from_api(&args.namespace, &args.name).await?;

    // Step 2: Run the local SSH command using the ID as the SSH host (e.g. Tailscale address).
    //         This uses the synchronous `run_ssh_command_ts` from your existing code.
    let output = nebulous::ssh::exec::run_ssh_command_ts(
        &format!("container-{}", container_id),
        args.command
            .split_whitespace()
            .map(|s| s.to_string())
            .collect(),
        args.interactive,
        args.tty,
        Some("root"), // Example: pass Some("root") if you need a specific user
    )?;

    // Output the SSH command’s stdout
    println!("{}", output);
    Ok(())
}

/// Helper function: calls GET /v1/containers/<namespace>/<name>
/// and returns the container’s `.metadata.id`.
async fn fetch_container_id_from_api(
    namespace: &str,
    name: &str,
) -> Result<String, Box<dyn StdError>> {
    let config = nebulous::config::GlobalConfig::read()?;
    let current_server = config.get_current_server_config().unwrap();
    let server = current_server.server.as_ref().unwrap();
    let api_key = current_server.api_key.as_ref().unwrap();
    // Adjust base URL/host as needed:
    let url = format!("{}/v1/containers/{}/{}", server, namespace, name);

    // Use reqwest to fetch container JSON
    let client = Client::new();
    let response = client
        .get(&url)
        .header("Authorization", format!("Bearer {}", api_key))
        .send()
        .await?;

    let container = response
        .error_for_status()? // Return Err if e.g. 404 or 500
        .json::<V1Container>()
        .await?;

    Ok(container.metadata.id)
}

/// Minimal struct matching the server’s “Container” JSON shape.
/// We only need the `metadata.id` field for this flow.
#[derive(Deserialize)]
struct V1Container {
    metadata: V1ResourceMeta,
}

/// Minimal struct for container’s metadata (includes ID).
#[derive(Deserialize)]
struct V1ResourceMeta {
    pub id: String,
}
