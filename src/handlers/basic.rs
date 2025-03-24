use axum::{response::IntoResponse, Json};
use serde_json::json;

pub async fn root_handler() -> impl IntoResponse {
    let response = json!({
        "name": "nebulous",
        "version": env!("CARGO_PKG_VERSION"),
        "description": "A cross-cloud container orchestration platform",
        "documentation": "https://docs.nebu.sh",
    });
    Json(response)
}

pub async fn health_handler() -> impl IntoResponse {
    let response = json!({
        "status": "ok",
        "timestamp": chrono::Utc::now().to_rfc3339(),
    });
    Json(response)
}
