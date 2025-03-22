use axum::{extract::Json, routing::post, Router};
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::process::Command;

/// This struct represents the JSON payload expected in the /exec request body.
#[derive(Deserialize, Debug)]
struct ExecRequest {
    cmd: String,
}

/// This struct represents the JSON response body with the output.
#[derive(Serialize, Debug)]
struct ExecResponse {
    output: String,
}

/// Our handler for POST /exec:
/// Reads the JSON body with the `cmd` field, spawns it as a shell command,
/// and returns its stdout in JSON.
async fn handle_exec(Json(payload): Json<ExecRequest>) -> Json<ExecResponse> {
    println!("\n$ {}", payload.cmd);
    let output = Command::new("sh")
        .arg("-c")
        .arg(payload.cmd)
        .output()
        .expect("Failed to execute process");

    let stdout_str = String::from_utf8_lossy(&output.stdout).to_string();
    println!("{}", stdout_str);

    Json(ExecResponse { output: stdout_str })
}

/// Launches an Axum-based server to handle /exec requests
pub async fn run_sync_cmd_server(host: &str, port: u16) -> anyhow::Result<()> {
    let app = Router::new().route("/exec", post(handle_exec));

    let addr: SocketAddr = format!("{}:{}", host, port)
        .parse()
        .expect("Invalid host/port");

    println!("Starting sync cmd server at http://{}", addr);
    let listener = tokio::net::TcpListener::bind(&addr).await?;

    println!("Server running at http://{}", addr);
    axum::serve(listener, app).await?;
    Ok(())
}
