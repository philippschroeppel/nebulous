use axum::{
    response::{IntoResponse, Response},
    routing::get,
    Router,
};
use hyper::StatusCode;
use std::net::SocketAddr;
use tokio::process::Command;

pub async fn run_server(host: &str, port: u16) -> Result<(), Box<dyn std::error::Error>> {
    // Build our application.
    let app = Router::new()
        .route("/metrics", get(metrics_handler))
        .route("/whoami", get(user_handler));

    // Convert host and port into a SocketAddr
    let addr_str = format!("{}:{}", host, port);
    let addr: SocketAddr = addr_str.parse()?;

    println!("Listening on {}", addr);

    let listener = tokio::net::TcpListener::bind(&addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}

/// Handler for the `"/metrics"` endpoint
async fn metrics_handler() -> Response {
    let output = Command::new("nvidia-smi")
        .args([
            "--query-gpu=uuid,name,memory.used,memory.total,utilization.gpu,utilization.memory",
            "--format=csv,noheader",
        ])
        .output()
        .await;

    match output {
        Ok(out) => {
            if out.status.success() {
                let output_str = String::from_utf8_lossy(&out.stdout).to_string();
                (StatusCode::OK, output_str).into_response()
            } else {
                let error_str = String::from_utf8_lossy(&out.stderr).to_string();
                (StatusCode::INTERNAL_SERVER_ERROR, error_str).into_response()
            }
        }
        Err(e) => {
            let msg = format!("Failed to run nvidia-smi: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, msg).into_response()
        }
    }
}

/// Handler for the `"/whoami"` endpoint
async fn user_handler() -> Response {
    let user = std::env::var("USER")
        .or_else(|_| std::env::var("USERNAME"))
        .unwrap_or_else(|_| "unknown".to_string());
    (StatusCode::OK, user).into_response()
}
